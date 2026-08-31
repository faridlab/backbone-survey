//! Probes 9, 10, 14, 15: the live-session runtime — the serialized
//! advance, the forward-only end, the realtime channel grammar, and the
//! resolver/payload compose contract (fail-hard; fresh scratch DB per
//! test).

use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use backbone_survey::application::service::event_sink::Fact;
use backbone_survey::application::service::session_read_service::{
    parse_record_channel, record_channel, STORED_CLOCK_GRACE_MS,
};
use backbone_survey::application::service::survey_write_service::{
    SurveyWriteError, SurveyWriteService,
};

use super::common::*;

/// The advance lock: concurrent advances serialize on the survey row —
/// every question is landed EXACTLY once, the lazy open pushes exactly
/// one SessionStarted, and the end of the question list answers
/// NoNextQuestion (never a wrap, never a skip).
#[tokio::test]
async fn p09_session_advance_lock() {
    let db = TestDb::new("adv_lock").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, _token) = seed_survey(&db.pool).await;
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;
    let q2 = seed_question(&db.pool, survey_id, 20, "numerical_box", 1.0, false).await;
    let q3 = seed_question(&db.pool, survey_id, 30, "numerical_box", 1.0, false).await;

    let armed = svc.writes.arm_session(survey_id).await.expect("arm");
    let code = armed.session_code.clone().expect("code");
    let _attendee = svc
        .attempts
        .join_by_code(&code, "pia", "10.0.0.11", "wire-pia", None)
        .await
        .expect("join");

    // The opening advance: lazy in_progress + cursor on q1.
    let opened = svc.writes.advance_session(survey_id).await.expect("first advance");
    assert_eq!(opened.session_question_id, Some(q1));
    assert_eq!(opened.session_state.unwrap().to_string(), "in_progress");

    // 8 SIMULTANEOUS advances: exactly two can still win (q2, q3).
    let writes = Arc::new(SurveyWriteService::new(db.pool.clone(), svc.sink.clone()));
    let barrier = Arc::new(tokio::sync::Barrier::new(8));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let w = writes.clone();
        let sid = survey_id;
        let gate = barrier.clone();
        tasks.spawn(async move {
            gate.wait().await;
            w.advance_session(sid).await
        });
    }
    let mut ok = 0usize;
    let mut no_next = 0usize;
    let mut other = Vec::new();
    while let Some(res) = tasks.join_next().await {
        match res.expect("task join").map(|s| s.session_question_id) {
            Ok(_) => ok += 1,
            Err(SurveyWriteError::NoNextQuestion) => no_next += 1,
            Err(e) => other.push(e),
        }
    }
    assert!(other.is_empty(), "no advance may fail otherwise: {other:?}");
    assert_eq!(ok, 2, "exactly the two remaining questions were landed");
    assert_eq!(no_next, 6, "the rest hit the exhausted cursor");

    // The final cursor is the LAST question; a further advance refuses.
    let final_cursor = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT session_question_id FROM survey.survey_surveys WHERE id = $1"#,
    )
    .bind(survey_id)
    .fetch_one(&db.pool)
    .await
    .expect("cursor");
    assert_eq!(final_cursor, q3);
    match svc.writes.advance_session(survey_id).await {
        Err(SurveyWriteError::NoNextQuestion) => {}
        Err(e) => panic!("expected NoNextQuestion, got {e:?}"),
        Ok(_) => panic!("advancing past the last question must refuse"),
    }

    // The fact stream: exactly ONE SessionStarted (the lazy open under
    // the row lock), one SessionAdvanced per successful advance.
    let started = svc.facts().iter().filter(|f| matches!(f, Fact::SessionStarted { .. })).count();
    let advanced = svc.facts().iter().filter(|f| matches!(f, Fact::SessionAdvanced { .. })).count();
    assert_eq!(started, 1, "the lazy open pushed exactly one SessionStarted");
    assert_eq!(advanced, 3, "one SessionAdvanced per landed question (1 + 2)");
    let _ = (q1, q2);

    db.dispose().await;
}

/// The end verb: forward-only bulk-done (an in_progress attendee lands
/// done; an already-done attendee keeps her ORIGINAL end stamp; a
/// never-begun attendee is not force-finished), the state NULLs, the
/// code dies with it, and further advances refuse.
#[tokio::test]
async fn p10_session_end_bulk_done() {
    let db = TestDb::new("end_bulk").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, _token) = seed_survey(&db.pool).await;
    seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;

    let armed = svc.writes.arm_session(survey_id).await.expect("arm");
    let code = armed.session_code.clone().expect("code");
    // Distinct identities AND IPs (the 1 s spacing is per code+identity).
    let alice = svc
        .attempts
        .join_by_code(&code, "alice", "10.0.1.1", "wire-alice", None)
        .await
        .expect("alice joins");
    let bob = svc
        .attempts
        .join_by_code(&code, "bob", "10.0.1.2", "wire-bob", None)
        .await
        .expect("bob joins");
    let carol = svc
        .attempts
        .join_by_code(&code, "carol", "10.0.1.3", "wire-carol", None)
        .await
        .expect("carol joins");

    // alice finishes on her own; bob begins; carol never does.
    svc.intake.begin(&alice.link).await.expect("alice begin");
    svc.intake.finish(&alice.link).await.expect("alice finish");
    svc.intake.begin(&bob.link).await.expect("bob begin");
    let alice_end: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
        r#"SELECT end_datetime FROM survey.survey_user_inputs WHERE id = $1"#,
    )
    .bind(alice.input.id)
    .fetch_one(&db.pool)
    .await
    .expect("alice end stamp");

    let done = svc.writes.end_session(survey_id).await.expect("end session");
    assert_eq!(done, 1, "exactly the one in_progress attendee was bulk-done");

    // States after: alice done (original stamp), bob done (new stamp),
    // carol still new (never begun — not force-finished).
    assert_eq!(state_of(&db.pool, alice.input.id).await, "done");
    assert_eq!(state_of(&db.pool, bob.input.id).await, "done");
    assert_eq!(state_of(&db.pool, carol.input.id).await, "new");
    let alice_end2: Option<chrono::DateTime<Utc>> = sqlx::query_scalar(
        r#"SELECT end_datetime FROM survey.survey_user_inputs WHERE id = $1"#,
    )
    .bind(alice.input.id)
    .fetch_one(&db.pool)
    .await
    .expect("alice end stamp 2");
    assert_eq!(alice_end, alice_end2, "the already-done attendee's stamp was not rewritten");
    assert!(
        sqlx::query_scalar::<_, Option<chrono::DateTime<Utc>>>(
            r#"SELECT end_datetime FROM survey.survey_user_inputs WHERE id = $1"#,
        )
        .bind(bob.input.id)
        .fetch_one(&db.pool)
        .await
        .expect("bob end stamp")
        .is_some(),
        "the bulk-done attendee carries an end stamp"
    );

    // The session state is gone; the code is dead; advances refuse.
    let state: Option<String> = sqlx::query_scalar(
        r#"SELECT session_state::text FROM survey.survey_surveys WHERE id = $1"#,
    )
    .bind(survey_id)
    .fetch_one(&db.pool)
    .await
    .expect("session state");
    assert!(state.is_none(), "end NULLs the session state");
    match svc.attempts.join_by_code(&code, "late", "10.9.9.9", "wire-late", None).await {
        Err(SurveyWriteError::SessionCodeNotValid) => {}
        other => panic!("the ended code must refuse like an unknown one, got {other:?}"),
    }
    match svc.writes.advance_session(survey_id).await {
        Err(SurveyWriteError::SessionNotRunning) => {}
        other => panic!("advance after end must refuse, got {other:?}"),
    }
    match svc.writes.end_session(survey_id).await {
        Err(SurveyWriteError::SessionNotRunning) => {}
        other => panic!("double end must refuse, got {other:?}"),
    }

    // The closing fact carries the (now dead) code.
    let facts = svc.facts();
    let ended = facts
        .iter()
        .filter(|f| matches!(f, Fact::SessionEnded { .. }))
        .count();
    assert_eq!(ended, 1);
    assert!(facts.iter().any(|f| matches!(f, Fact::SessionEnded { session_code, .. } if session_code == &code)));

    db.dispose().await;
}

/// The realtime channel grammar: strict round-trip and strict rejection
/// (the resolver must never half-match), and the resolver's allow-rule
/// over live session answers.
#[test]
fn p14a_realtime_channel_grammar() {
    let id = Uuid::new_v4();
    let channel = record_channel(id);
    assert_eq!(channel, format!("survey.survey_{id}"));
    assert_eq!(parse_record_channel(&channel), Some(id), "round-trips");

    // Wrong kind / wrong prefix / malformed uuid / trailing junk.
    assert_eq!(parse_record_channel(""), None);
    assert_eq!(parse_record_channel("survey.survey_"), None, "empty record id");
    assert_eq!(parse_record_channel("survey.survey_not-a-uuid"), None);
    assert_eq!(parse_record_channel("survey.survey_abc"), None);
    assert_eq!(parse_record_channel(&format!("survey.survey_{id}x")), None, "uuid + junk");
    assert_eq!(
        parse_record_channel(&format!("survey.survey_{id}.tail")),
        None,
        "channel-shaped but namespaced"
    );
    assert_eq!(parse_record_channel(&format!("mail.survey_{id}")), None, "foreign kind");
    assert_eq!(parse_record_channel(&format!("survey.{id}")), None, "record without the survey kind");
    assert_eq!(parse_record_channel(&format!("survey.SURVEY_{id}")), None, "case matters");
    assert_eq!(
        parse_record_channel(&format!("survey.survey_survey.survey_{id}")),
        None,
        "a doubled prefix never half-matches"
    );
    // Uuid-adjacent strings that parse as OTHER uuid formats still pass
    // only when they are plain uuids.
    assert_eq!(parse_record_channel(&format!("survey.survey_{}", id.simple())), Some(id));
}

/// The resolver's allow-rule against the database: a joined wire
/// identity may read its survey's channel; a foreign handle may not;
/// a finished attempt may not.
#[tokio::test]
async fn p14b_realtime_resolver_rule() {
    let db = TestDb::new("resolver").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, _token) = seed_survey(&db.pool).await;
    seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;

    let armed = svc.writes.arm_session(survey_id).await.expect("arm");
    let code = armed.session_code.clone().expect("code");
    let t = svc
        .attempts
        .join_by_code(&code, "riva", "10.0.0.5", "wire-riva", None)
        .await
        .expect("join");

    assert!(svc.reads.resolver_allows(survey_id, "wire-riva").await.expect("allow riva"));
    assert!(!svc.reads.resolver_allows(survey_id, "wire-nobody").await.expect("deny foreign"));
    // Another survey's session answer must not leak across.
    let (other, _ot) = seed_survey(&db.pool).await;
    assert!(!svc.reads.resolver_allows(other, "wire-riva").await.expect("deny cross-survey"));

    // Finishing the attempt closes the read.
    svc.intake.begin(&t.link).await.expect("begin");
    svc.intake.finish(&t.link).await.expect("finish");
    assert!(
        !svc.reads.resolver_allows(survey_id, "wire-riva").await.expect("deny finished"),
        "a done attempt no longer reads the channel"
    );

    db.dispose().await;
}

/// The compose contract: the pushed SessionAdvanced payload carries the
/// PRE-write clock (the fact's payload_millis), the read-side
/// next_question payload is the STORED clock minus the server-delay
/// grace, and the disallowed identity is denied at the resolver.
#[tokio::test]
async fn p15_resolver_compose() {
    let db = TestDb::new("compose").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, _token) = seed_survey(&db.pool).await;
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;

    let armed = svc.writes.arm_session(survey_id).await.expect("arm");
    let code = armed.session_code.clone().expect("code");
    let _t = svc
        .attempts
        .join_by_code(&code, "olve", "10.0.0.6", "wire-olve", None)
        .await
        .expect("join");

    // No question live before the first advance.
    let pre = svc.reads.snapshot(survey_id).await.expect("pre snapshot");
    assert!(pre.next_question_payload().is_none());

    // The pre/post bounds of the advance write.
    let t0 = Utc::now();
    let advanced = svc.writes.advance_session(survey_id).await.expect("advance");
    let t1 = Utc::now();

    // Fact side: payload_millis is the PRE-write instant.
    let fact = svc
        .facts()
        .iter()
        .find_map(|f| match f {
            Fact::SessionAdvanced { payload_millis, question_id, .. } => {
                Some((*payload_millis, *question_id))
            }
            _ => None,
        })
        .expect("SessionAdvanced fact");
    assert_eq!(fact.1, q1);
    assert!(
        fact.0 >= t0.timestamp_millis() && fact.0 <= t1.timestamp_millis(),
        "payload_millis is the pre-write wall instant"
    );

    // Read side: the payload is the stored clock minus the grace.
    let stored_clock: chrono::DateTime<Utc> = sqlx::query_scalar(
        r#"SELECT session_question_start_time FROM survey.survey_surveys WHERE id = $1"#,
    )
    .bind(survey_id)
    .fetch_one(&db.pool)
    .await
    .expect("stored clock");
    assert_eq!(advanced.session_question_start_time, Some(stored_clock));

    let snap = svc.reads.snapshot(survey_id).await.expect("post snapshot");
    let payload = snap.next_question_payload().expect("payload after advance");
    assert_eq!(payload.question_id, q1);
    assert_eq!(payload.sequence, 10);
    assert_eq!(
        payload.question_start_ms,
        stored_clock.timestamp_millis() - STORED_CLOCK_GRACE_MS,
        "the push payload strips the +1 s server-delay grace"
    );
    assert_eq!(
        payload.question_start_ms, fact.0,
        "the fact's pre-write clock and the read payload agree"
    );

    // The disallowed identity is denied; the attendee is allowed.
    assert!(!svc.reads.resolver_allows(survey_id, "wire-stranger").await.expect("deny"));
    assert!(svc.reads.resolver_allows(survey_id, "wire-olve").await.expect("allow"));

    // After end: no live question, no payload.
    svc.writes.end_session(survey_id).await.expect("end");
    let post = svc.reads.snapshot(survey_id).await.expect("post-end snapshot");
    assert!(post.next_question_payload().is_none());
    assert!(post.session_state.is_none());

    db.dispose().await;
}

/// The certification/session disjointness: a certification survey
/// refuses to arm (typed, BEFORE any code is minted), a plain survey on
/// the same database still arms, and the DB CHECK holds the line against
/// a direct write that bypasses the service.
#[tokio::test]
async fn p10b_certification_arm_refused() {
    let db = TestDb::new("cert_arm").await;
    let svc = Svc::new(db.pool.clone());

    // The certification survey: refused, typed, and nothing minted.
    let (cert_id, _cert_token) = seed_cert_survey(&db.pool, 50.0).await;
    match svc.writes.arm_session(cert_id).await {
        Err(SurveyWriteError::SessionCertificationConflict) => {}
        Err(SurveyWriteError::SessionCodeExhausted) => {
            skipped("mint ladder exhausted — refused path never reached")
        }
        other => panic!("certification arm must refuse, got {other:?}"),
    }
    let armed_row: Option<String> = sqlx::query_scalar(
        r#"SELECT session_code FROM survey.survey_surveys WHERE id = $1"#,
    )
    .bind(cert_id)
    .fetch_one(&db.pool)
    .await
    .expect("cert survey row");
    assert!(armed_row.is_none(), "no session code was minted for the refused arm");

    // A plain survey on the same database arms normally.
    let (plain_id, _plain_token) = seed_survey(&db.pool).await;
    let armed = svc.writes.arm_session(plain_id).await.expect("plain survey arms");
    assert!(armed.session_code.is_some(), "the control arm minted a code");

    // The DB CHECK: a direct write bypassing the service is refused by
    // the constraint itself (SQLSTATE 23514 check_violation).
    let direct = sqlx::query(
        r#"UPDATE survey.survey_surveys
           SET session_state = 'ready', session_code = '1234'
           WHERE id = $1"#,
    )
    .bind(cert_id)
    .execute(&db.pool)
    .await;
    match direct {
        Err(e) => {
            let code = match &e {
                sqlx::Error::Database(db) => db.code(),
                _ => None,
            };
            assert_eq!(
                code.as_deref(),
                Some("23514"),
                "the certification/session overlap must trip the CHECK, got {e}"
            );
        }
        Ok(_) => panic!("direct session_state write on a certification survey must trip the CHECK"),
    }

    db.dispose().await;
}
