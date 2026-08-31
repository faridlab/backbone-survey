//! Probes 1–2: the Tier A capability lifecycle and the Tier B
//! session-code lockout (fail-hard; fresh scratch DB per test).

use chrono::{Duration, Utc};
use uuid::Uuid;

use backbone_survey::application::service::attempt_service::{
    lockout_until, CODE_ATTEMPT_SPACING, CODE_LOCK_BASE_SECONDS, CODE_LOCK_CAP_SECONDS,
    CODE_MAX_FAILURES,
};
use backbone_survey::application::service::intake_service::AnswerDraft;
use backbone_survey::application::service::survey_write_service::SurveyWriteError;

use super::common::*;

// ─── probe 1: token_tier_a_lifecycle ──────────────────────────────────────────

/// The full Tier A arc: parse → verify (multi-use) → forged/expired/malformed
/// refusals share one code → rotation kills the old link → a finished
/// attempt refuses → the survey URL key never authorizes by itself.
#[tokio::test]
async fn p01_token_tier_a_lifecycle() {
    let db = TestDb::new("tok_a").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, access_token) = seed_survey(&db.pool).await;
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;

    // Entry mints a capability; the attempt row exists BEFORE any submit
    // (pre-creation), state = new.
    let link = start_attempt(&svc, &access_token).await;
    let input = input_of(&db.pool, &link).await;
    assert_eq!(state_of(&db.pool, input.id).await, "new", "entry must pre-create the row in new");
    assert!(!input.test_entry);

    // Begin + one submit: the SAME link verifies again (multi-use within
    // the attempt's life — rating is single-use, survey is not).
    svc.intake.begin(&link).await.expect("begin");
    let outcome = svc
        .intake
        .submit_answer(&link, q1, AnswerDraft::Number(0.0))
        .await
        .expect("submit");
    assert_eq!(outcome.totals.total, 0.0);

    // Forged MAC: same id/nonce/exp, wrong key — the shared refusal.
    let id = input.id;
    let forged = forge_link(id, &input.token_nonce, input.token_expires_at.timestamp());
    match svc.attempts.verify_capability(&forged).await {
        Err(e @ SurveyWriteError::AttemptNotSubmittable) => {
            assert_eq!(e.code(), "survey_attempt_not_submittable");
        }
        other => panic!("forged MAC must refuse, got {other:?}"),
    }

    // Malformed shape: same refusal, never a panic.
    for bad in ["", "abc", "x.y.z", &format!("{id}.deadbeef.999.0")] {
        assert!(
            matches!(
                svc.attempts.verify_capability(bad).await,
                Err(SurveyWriteError::AttemptNotSubmittable)
            ),
            "malformed {bad:?} must refuse with the shared body"
        );
    }

    // Rotation: fresh nonce + expiry; the OLD link dies with the UPDATE,
    // the fresh one verifies.
    let ticket = svc.attempts.rotate_token(id, Some(30)).await.expect("rotate");
    assert_ne!(ticket.input.token_nonce, input.token_nonce, "rotation must mint a new nonce");
    match svc.attempts.verify_capability(&link).await {
        Err(SurveyWriteError::AttemptNotSubmittable) => {}
        other => panic!("the rotated-away link must refuse, got {other:?}"),
    }
    svc.attempts.verify_capability(&ticket.link).await.expect("fresh link verifies");

    // The survey's URL key is NOT an authorizer: an unknown access token
    // (or a known one — it is a locator, not a capability) never yields a
    // capability by itself.
    match svc.attempts.public_start("definitely-not-a-real-key", None, None, None).await {
        Err(SurveyWriteError::SurveyNotPublicAccess) => {}
        other => panic!("unknown survey key must refuse, got {other:?}"),
    }

    // Expiry: the typed 410 (shared body, distinct status).
    let expired_id = {
        let iid = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO survey.survey_user_inputs
                 (id, survey_id, token_nonce, token_expires_at)
               VALUES ($1, $2, $3, now() - interval '1 second')"#,
        )
        .bind(iid)
        .bind(survey_id)
        .bind(Uuid::new_v4().simple().to_string())
        .execute(&db.pool)
        .await
        .expect("seed expired input");
        iid
    };
    let expired_row = input_by_id(&db.pool, expired_id).await;
    let expired_link = svc.attempts.mint_capability(&expired_row).expect("render expired link");
    match svc.attempts.verify_capability(&expired_link).await {
        Err(e) => assert_eq!(e.http_status(), 410, "expired attempt must 410"),
        Ok(_) => panic!("expired link verified"),
    }

    // Done: the multi-use window closes with the attempt.
    svc.intake.finish(&ticket.link).await.expect("finish");
    match svc.attempts.verify_capability(&ticket.link).await {
        Err(SurveyWriteError::AttemptNotSubmittable) => {}
        other => panic!("finished attempt must refuse, got {other:?}"),
    }

    db.dispose().await;
}

// ─── probe 2: tier_b_code_lockout ─────────────────────────────────────────────

/// The escalation table as a pure function: 3 → 30 s, doubling per extra
/// failure, capped at 15 min; below 3 → never locked.
#[test]
fn p02a_lockout_table() {
    let t0 = Utc::now();
    for failures in 0..CODE_MAX_FAILURES {
        assert!(lockout_until(failures, t0).is_none(), "{failures} failures must not lock");
    }
    assert_eq!(
        lockout_until(3, t0).map(|t| (t - t0).num_seconds()),
        Some(CODE_LOCK_BASE_SECONDS),
        "third failure locks for the base window"
    );
    assert_eq!(lockout_until(4, t0).map(|t| (t - t0).num_seconds()), Some(60));
    assert_eq!(lockout_until(5, t0).map(|t| (t - t0).num_seconds()), Some(120));
    assert_eq!(lockout_until(6, t0).map(|t| (t - t0).num_seconds()), Some(240));
    // The cap binds long before the shift overflows.
    assert_eq!(lockout_until(20, t0).map(|t| (t - t0).num_seconds()), Some(CODE_LOCK_CAP_SECONDS));
    assert_eq!(CODE_ATTEMPT_SPACING, Duration::seconds(1), "1 s minimum spacing");
}

/// The book + the join verb: wrong codes escalate to the locked refusal
/// (429), a live code resets BOTH books, spacing gates hammering, and a
/// dead (TTL-expired or stateless) code answers the same shared refusal
/// as an unknown one — no oracle.
#[tokio::test]
async fn p02b_code_verify_and_lockout() {
    let db = TestDb::new("tok_b").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, _token) = seed_survey(&db.pool).await;
    seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;

    // Arm: code minted, session ready.
    let armed = svc.writes.arm_session(survey_id).await.expect("arm");
    let code = armed.session_code.clone().expect("code minted");
    let book = svc.attempts.failure_book();

    // Two wrong answers from one identity: not locked yet. The 1 s
    // per-identity spacing gates EVERY attempt (valid or not), so the
    // successive attempts here are spaced just over it — the lockout leg
    // under test, not the spacing refusal, is what answers.
    for i in 0..2 {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(1050)).await;
        }
        match svc.attempts.join_by_code("0000", "alice", "10.0.0.1", "wire-alice", None).await {
            Err(SurveyWriteError::SessionCodeNotValid) => {}
            other => panic!("wrong code must refuse the shared body, got {other:?}"),
        }
    }
    assert_eq!(book.failures("code:0000|id:alice"), 2);

    // Third wrong answer: the book crosses the threshold; the FOURTH
    // attempt (even with the RIGHT code shape) hits the locked refusal.
    tokio::time::sleep(std::time::Duration::from_millis(1050)).await;
    let _ = svc.attempts.join_by_code("0000", "alice", "10.0.0.1", "wire-alice", None).await;
    assert_eq!(book.failures("code:0000|id:alice"), 3);
    tokio::time::sleep(std::time::Duration::from_millis(1050)).await;
    match svc.attempts.join_by_code("0000", "alice", "10.0.0.1", "wire-alice", None).await {
        Err(SurveyWriteError::SessionCodeLocked { retry_after_seconds }) => {
            assert!(retry_after_seconds > 0 && retry_after_seconds <= CODE_LOCK_CAP_SECONDS);
        }
        other => panic!("locked identity must 429, got {other:?}"),
    }

    // Spacing: a different identity, twice in the same second — the
    // second attempt answers the spacing refusal (anti-hammering).
    let _ = svc.attempts.join_by_code("0000", "bob", "10.0.0.2", "wire-bob", None).await;
    match svc.attempts.join_by_code("0000", "bob", "10.0.0.2", "wire-bob", None).await {
        Err(SurveyWriteError::SessionCodeSpacing) => {}
        other => panic!("hammering must hit the spacing refusal, got {other:?}"),
    }

    // The right code from a THIRD identity (clean books): join succeeds,
    // is_session_answer, the guest handle stamped, and BOTH books reset.
    let ticket = svc
        .attempts
        .join_by_code(&code, "carol", "10.0.0.3", "wire-carol", Some("carol"))
        .await
        .expect("live code joins");
    let row = input_of(&db.pool, &ticket.link).await;
    assert!(row.is_session_answer, "code join is a session answer");
    assert_eq!(row.wire_identity_key.as_deref(), Some("wire-carol"));
    assert_eq!(state_of(&db.pool, row.id).await, "new", "armed-not-running session admits as new");

    // A dead code (session ended) and an unknown code share ONE refusal.
    svc.writes.end_session(survey_id).await.expect("end");
    match svc.attempts.join_by_code(&code, "dave", "10.0.0.4", "wire-dave", None).await {
        Err(SurveyWriteError::SessionCodeNotValid) => {}
        other => panic!("dead code must refuse identically to unknown, got {other:?}"),
    }

    db.dispose().await;
}

/// Rotation on a terminal (done) attempt must refuse — the link of a
/// finished attempt dies, it does not rotate.
#[tokio::test]
async fn p02c_rotate_refuses_terminal() {
    let db = TestDb::new("tok_c").await;
    let svc = Svc::new(db.pool.clone());
    let (_survey_id, access_token) = seed_survey(&db.pool).await;
    let link = start_attempt(&svc, &access_token).await;
    let id = input_of(&db.pool, &link).await.id;
    svc.intake.begin(&link).await.expect("begin");
    svc.intake.finish(&link).await.expect("finish");
    match svc.attempts.rotate_token(id, Some(30)).await {
        Err(SurveyWriteError::AttemptNotSubmittable) => {}
        other => panic!("rotating a finished attempt must refuse, got {other:?}"),
    }
    db.dispose().await;
}
