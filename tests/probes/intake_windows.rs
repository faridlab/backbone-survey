//! Probes 7, 8, 16, 18: the intake contract, the conditional-clearing
//! edge, the anti-cheat grace windows, and the UTC deadline semantics
//! (fail-hard; fresh scratch DB per test).

use chrono::{Duration, Utc};
use uuid::Uuid;

use backbone_survey::application::service::intake_service::AnswerDraft;
use backbone_survey::application::service::survey_write_service::SurveyWriteError;

use super::common::*;

/// The intake contract leg by leg: validation dispatch (range, length,
/// scale band), mandatory refusal, frozen-set membership, the overwrite
/// gate (refused without go-back, allowed with it), and the page
/// refusal.
#[tokio::test]
async fn p07_intake_contract() {
    let db = TestDb::new("intake_c").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_survey_opts(&db.pool, "no_scoring", 80.0, None, None, false, None).await;

    // q1: validated numerical range 5..=10.
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;
    sqlx::query(
        r#"UPDATE survey.survey_questions
           SET validation_required = TRUE,
               validation_min_float_value = 5.0, validation_max_float_value = 10.0
           WHERE id = $1"#,
    )
    .bind(q1)
    .execute(&db.pool)
    .await
    .expect("range flags");

    // q2: validated char length 3..=8.
    let q2 = seed_question(&db.pool, survey_id, 20, "char_box", 1.0, false).await;
    sqlx::query(
        r#"UPDATE survey.survey_questions
           SET validation_required = TRUE,
               validation_length_min = 3, validation_length_max = 8
           WHERE id = $1"#,
    )
    .bind(q2)
    .execute(&db.pool)
    .await
    .expect("length flags");

    // q3: mandatory.
    let q3 = seed_question(&db.pool, survey_id, 30, "char_box", 1.0, false).await;
    sqlx::query(r#"UPDATE survey.survey_questions SET constr_mandatory = TRUE WHERE id = $1"#)
        .bind(q3)
        .execute(&db.pool)
        .await
        .expect("mandatory flag");

    // q4: scale with band 1..=5.
    let q4 = seed_question(&db.pool, survey_id, 40, "scale", 1.0, false).await;
    sqlx::query(
        r#"UPDATE survey.survey_questions
           SET validation_required = TRUE, scale_min = 1, scale_max = 5
           WHERE id = $1"#,
    )
    .bind(q4)
    .execute(&db.pool)
    .await
    .expect("scale band");

    // A page row (is_page) exists in the survey but is not answerable.
    let page = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO survey.survey_questions (id, survey_id, sequence, is_page, title)
           VALUES ($1, $2, 5, TRUE, 'the page')"#,
    )
    .bind(page)
    .bind(survey_id)
    .execute(&db.pool)
    .await
    .expect("seed page");

    let link = start_attempt(&svc, &token).await;
    svc.intake.begin(&link).await.expect("begin");

    // Range: below/above refuse with the typed 422; inside passes.
    match svc.intake.submit_answer(&link, q1, AnswerDraft::Number(3.0)).await {
        Err(SurveyWriteError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("minimum"), "range refusal names the bound: {reason}");
        }
        other => panic!("below-range must refuse, got {other:?}"),
    }
    match svc.intake.submit_answer(&link, q1, AnswerDraft::Number(42.0)).await {
        Err(SurveyWriteError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("maximum"), "range refusal names the bound: {reason}");
        }
        other => panic!("above-range must refuse, got {other:?}"),
    }
    svc.intake
        .submit_answer(&link, q1, AnswerDraft::Number(7.0))
        .await
        .expect("inside range passes");

    // Length: too short / too long refuse; in-band passes.
    match svc.intake.submit_answer(&link, q2, AnswerDraft::Char("ab".into())).await {
        Err(SurveyWriteError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("short"), "length refusal: {reason}");
        }
        other => panic!("too-short must refuse, got {other:?}"),
    }
    match svc.intake.submit_answer(&link, q2, AnswerDraft::Char("way-too-long-value".into())).await {
        Err(SurveyWriteError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("long"), "length refusal: {reason}");
        }
        other => panic!("too-long must refuse, got {other:?}"),
    }
    svc.intake
        .submit_answer(&link, q2, AnswerDraft::Char("valid".into()))
        .await
        .expect("in-band length passes");

    // Mandatory: an explicit skip refuses.
    match svc.intake.submit_answer(&link, q3, AnswerDraft::Skipped).await {
        Err(SurveyWriteError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("mandatory"), "mandatory refusal: {reason}");
        }
        other => panic!("skipping a mandatory question must refuse, got {other:?}"),
    }
    svc.intake
        .submit_answer(&link, q3, AnswerDraft::Char("answer".into()))
        .await
        .expect("answering a mandatory question passes");

    // Scale band: outside refuses, inside passes.
    match svc.intake.submit_answer(&link, q4, AnswerDraft::Scale(9)).await {
        Err(SurveyWriteError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("outside"), "scale refusal: {reason}");
        }
        other => panic!("out-of-band scale must refuse, got {other:?}"),
    }
    svc.intake
        .submit_answer(&link, q4, AnswerDraft::Scale(4))
        .await
        .expect("in-band scale passes");

    // Pages are not answerable (a distinct refusal from frozen-set).
    match svc.intake.submit_answer(&link, page, AnswerDraft::Char("x".into())).await {
        Err(SurveyWriteError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("pages"), "page refusal: {reason}");
        }
        other => panic!("a page row must refuse, got {other:?}"),
    }

    // Overwrite without go-back: refused with the typed error.
    match svc.intake.submit_answer(&link, q1, AnswerDraft::Number(8.0)).await {
        Err(SurveyWriteError::OverwriteRefused { question_id }) => assert_eq!(question_id, q1),
        other => panic!("overwrite without go-back must refuse, got {other:?}"),
    }

    // Overwrite WITH go-back: allowed; the value moves, the score triple
    // stays write-once (scalar overwrite touches value columns only).
    let (survey2, token2) = seed_survey_opts(&db.pool, "no_scoring", 80.0, None, None, true, None).await;
    let vq = seed_question(&db.pool, survey2, 10, "numerical_box", 1.0, false).await;
    let link2 = start_attempt(&svc, &token2).await;
    svc.intake
        .submit_answer(&link2, vq, AnswerDraft::Number(1.0))
        .await
        .expect("first answer");
    svc.intake
        .submit_answer(&link2, vq, AnswerDraft::Number(2.0))
        .await
        .expect("go-back overwrite passes");
    let (val, lines): (Option<f64>, i64) = sqlx::query_as(
        r#"SELECT value_numerical_box, count(*) FROM survey.survey_user_input_lines
           WHERE question_id = $1 GROUP BY value_numerical_box"#,
    )
    .bind(vq)
    .fetch_one(&db.pool)
    .await
    .expect("line after overwrite");
    assert_eq!(val, Some(2.0), "the VALUE moved");
    assert_eq!(lines, 1, "still exactly one line — update, not a second row");

    db.dispose().await;
}

/// The conditional edge: a choice answer keeps the dependent lines of
/// every trigger STILL chosen and clears the dependents of every trigger
/// that fell out of the chosen set.
#[tokio::test]
async fn p08_conditional_edge() {
    let db = TestDb::new("cond_edge").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_survey_opts(&db.pool, "no_scoring", 80.0, None, None, true, None).await;

    let parent = seed_question(&db.pool, survey_id, 10, "simple_choice", 1.0, false).await;
    let label_a = seed_label(&db.pool, parent, "A", false, 0.0).await;
    let label_b = seed_label(&db.pool, parent, "B", false, 0.0).await;
    let dep_a = seed_question(&db.pool, survey_id, 20, "char_box", 1.0, false).await;
    let dep_b = seed_question(&db.pool, survey_id, 30, "char_box", 1.0, false).await;
    seed_trigger(&db.pool, dep_a, label_a).await;
    seed_trigger(&db.pool, dep_b, label_b).await;

    let link = start_attempt(&svc, &token).await;

    let lines_of = |qid: Uuid| {
        let pool = db.pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                r#"SELECT count(*) FROM survey.survey_user_input_lines
                   WHERE question_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
            )
            .bind(qid)
            .fetch_one(&pool)
            .await
            .expect("line count")
        }
    };

    // Choose A: dep_a is answerable, dep_b has nothing yet either way.
    svc.intake
        .submit_answer(&link, parent, AnswerDraft::Choice(vec![label_a]))
        .await
        .expect("choose A");
    svc.intake
        .submit_answer(&link, dep_a, AnswerDraft::Char("for-A".into()))
        .await
        .expect("answer dep_a");
    assert_eq!(lines_of(dep_a).await, 1);

    // Broaden to A+B: dep_a's line is KEPT (A still chosen); dep_b opens.
    svc.intake
        .submit_answer(&link, parent, AnswerDraft::Choice(vec![label_a, label_b]))
        .await
        .expect("choose A+B");
    assert_eq!(lines_of(dep_a).await, 1, "still-chosen trigger keeps its dependent");
    svc.intake
        .submit_answer(&link, dep_b, AnswerDraft::Char("for-B".into()))
        .await
        .expect("answer dep_b");

    // Narrow to B only: dep_a's line is CLEARED, dep_b's stays.
    svc.intake
        .submit_answer(&link, parent, AnswerDraft::Choice(vec![label_b]))
        .await
        .expect("choose B only");
    assert_eq!(lines_of(dep_a).await, 0, "dropped trigger clears its dependent");
    assert_eq!(lines_of(dep_b).await, 1, "kept trigger keeps its dependent");

    // The parent's own line count follows delete-and-recreate: one line
    // per currently-chosen label.
    assert_eq!(lines_of(parent).await, 1);

    db.dispose().await;
}

/// The anti-cheat windows: attempt pre-creation (the row exists in `new`
/// before any submit), the survey-wide deadline + 10 s grace, and the
/// per-question limit + 3 s grace on the live cursor — beyond the limit
/// but inside the grace, validation is SUPPRESSED (a late answer saves
/// unvalidated rather than being lost).
#[tokio::test]
async fn p16_anti_cheat_windows() {
    let db = TestDb::new("windows").await;
    let svc = Svc::new(db.pool.clone());

    // ── attempt pre-creation + the survey-wide window ───────────────────
    // (time-limited 1 min: the survey-wide deadline gate is armed by the
    // survey's own flag, then the deadline column is warped per leg)
    let (survey_id, token) = seed_survey_opts(&db.pool, "no_scoring", 80.0, Some(1.0), None, true, None).await;
    let q1 = seed_question(&db.pool, survey_id, 10, "char_box", 1.0, false).await;
    let link = start_attempt(&svc, &token).await;
    let input_id = input_of(&db.pool, &link).await.id;
    assert_eq!(state_of(&db.pool, input_id).await, "new", "the attempt row pre-exists any submit");
    assert!(
        sqlx::query_scalar::<_, Option<chrono::DateTime<Utc>>>(
            r#"SELECT start_datetime FROM survey.survey_user_inputs WHERE id = $1"#,
        )
        .bind(input_id)
        .fetch_one(&db.pool)
        .await
        .expect("start stamp")
        .is_none(),
        "the clock has NOT started at mint"
    );

    // Lazy begin: the first submit stamps the start.
    svc.intake
        .submit_answer(&link, q1, AnswerDraft::Char("first".into()))
        .await
        .expect("first submit");
    assert!(
        sqlx::query_scalar::<_, Option<chrono::DateTime<Utc>>>(
            r#"SELECT start_datetime FROM survey.survey_user_inputs WHERE id = $1"#,
        )
        .bind(input_id)
        .fetch_one(&db.pool)
        .await
        .expect("start stamp 2")
        .is_some(),
        "the clock starts at the first interaction"
    );

    // Survey deadline: 5 s past it (inside the 10 s grace) — accepted.
    sqlx::query(r#"UPDATE survey.survey_user_inputs SET deadline = now() - interval '5 seconds' WHERE id = $1"#)
        .bind(input_id)
        .execute(&db.pool)
        .await
        .expect("warp deadline -5s");
    svc.intake
        .submit_answer(&link, q1, AnswerDraft::Char("late-but-inside".into()))
        .await
        .expect("inside the survey grace window");

    // 11 s past it — beyond the grace, refused.
    sqlx::query(r#"UPDATE survey.survey_user_inputs SET deadline = now() - interval '11 seconds' WHERE id = $1"#)
        .bind(input_id)
        .execute(&db.pool)
        .await
        .expect("warp deadline -11s");
    match svc.intake.submit_answer(&link, q1, AnswerDraft::Char("too-late".into())).await {
        Err(SurveyWriteError::SurveyTimeLimitExceeded) => {}
        other => panic!("beyond the survey grace must refuse, got {other:?}"),
    }

    // ── the per-question window on the live session cursor ─────────────
    let (survey2, _t2) = seed_survey_opts(&db.pool, "no_scoring", 80.0, None, None, true, None).await;
    let sq = seed_question(&db.pool, survey2, 10, "char_box", 1.0, false).await;
    sqlx::query(
        r#"UPDATE survey.survey_questions SET is_time_limited = TRUE, time_limit = 3 WHERE id = $1"#,
    )
    .bind(sq)
    .execute(&db.pool)
    .await
    .expect("question time limit");
    // A validated mandatory-ish question to prove suppression: min length 3
    // (the check constraint requires the max whenever the min is set).
    sqlx::query(
        r#"UPDATE survey.survey_questions
           SET validation_required = TRUE, validation_length_min = 3, validation_length_max = 10
           WHERE id = $1"#,
    )
    .bind(sq)
    .execute(&db.pool)
    .await
    .expect("validation flags");

    let armed = svc.writes.arm_session(survey2).await.expect("arm");
    let code = armed.session_code.clone().expect("code");
    let ticket = svc
        .attempts
        .join_by_code(&code, "wanda", "10.0.0.7", "wire-wanda", Some("wanda"))
        .await
        .expect("join");
    let slink = ticket.link.clone();
    svc.writes.advance_session(survey2).await.expect("advance");

    // Warp the session clock: 2 s ago → inside the limit, validation LIVE.
    sqlx::query(
        r#"UPDATE survey.survey_surveys SET session_question_start_time = now() - interval '2 seconds' WHERE id = $1"#,
    )
    .bind(survey2)
    .execute(&db.pool)
    .await
    .expect("clock -2s");
    match svc.intake.submit_answer(&slink, sq, AnswerDraft::Char("ab".into())).await {
        Err(SurveyWriteError::ValidationFailed { .. }) => {}
        other => panic!("inside the limit, validation must be live, got {other:?}"),
    }
    svc.intake
        .submit_answer(&slink, sq, AnswerDraft::Char("in-limit".into()))
        .await
        .expect("valid submit inside the limit");

    // 4 s ago: beyond the 3 s limit, inside the 3 s grace — an INVALID
    // answer is ACCEPTED (validation suppressed, the answer saved).
    sqlx::query(
        r#"UPDATE survey.survey_surveys SET session_question_start_time = now() - interval '4 seconds' WHERE id = $1"#,
    )
    .bind(survey2)
    .execute(&db.pool)
    .await
    .expect("clock -4s");
    let out = svc
        .intake
        .submit_answer(&slink, sq, AnswerDraft::Char("ab".into()))
        .await
        .expect("late-but-inside: saved unvalidated");
    assert_eq!(out.line.answer_score, None, "an unvalidated save carries no score claim");

    // 7 s ago: beyond limit + grace — refused.
    sqlx::query(
        r#"UPDATE survey.survey_surveys SET session_question_start_time = now() - interval '7 seconds' WHERE id = $1"#,
    )
    .bind(survey2)
    .execute(&db.pool)
    .await
    .expect("clock -7s");
    match svc.intake.submit_answer(&slink, sq, AnswerDraft::Char("gone".into())).await {
        Err(SurveyWriteError::QuestionTimeLimitExceeded) => {}
        other => panic!("beyond the question grace must refuse, got {other:?}"),
    }

    db.dispose().await;
}

/// The deadline is a UTC INSTANT: under a different session time zone
/// the stored deadline renders differently but the underlying epoch is
/// identical — deadlines never drift with the viewer's zone.
#[tokio::test]
async fn p18_deadline_utc() {
    let db = TestDb::new("deadline_utc").await;
    let svc = Svc::new(db.pool.clone());
    // A time-limited survey: 2 minutes.
    let (_survey_id, token) = seed_survey_opts(&db.pool, "no_scoring", 80.0, Some(2.0), None, false, None).await;
    let t0 = Utc::now();
    let link = start_attempt(&svc, &token).await;
    let input_id = input_of(&db.pool, &link).await.id;

    // Minted deadline ≈ entry + 120 s (the survey-wide limit).
    let (deadline, epoch_utc): (Option<chrono::DateTime<Utc>>, f64) = sqlx::query_as(
        r#"SELECT deadline, extract(epoch from deadline)::float8
           FROM survey.survey_user_inputs WHERE id = $1"#,
    )
    .bind(input_id)
    .fetch_one(&db.pool)
    .await
    .expect("deadline row");
    let deadline = deadline.expect("time-limited survey stamps a deadline");
    let delta = deadline - t0;
    assert!(
        delta >= Duration::seconds(119) && delta <= Duration::seconds(121),
        "deadline is the mint-time limit away, got {delta:?}"
    );

    // Same instant under Asia/Jakarta (+7): epoch identical, rendering +7h.
    let mut tz = db.pool.acquire().await.expect("tz conn");
    sqlx::query("SET TIME ZONE 'Asia/Jakarta'")
        .execute(&mut *tz)
        .await
        .expect("set zone");
    let (epoch_jkt, rendered_jkt): (f64, String) = sqlx::query_as(
        r#"SELECT extract(epoch from deadline)::float8,
                  to_char(deadline, 'YYYY-MM-DD HH24:MI:SS')
           FROM survey.survey_user_inputs WHERE id = $1"#,
    )
    .bind(input_id)
    .fetch_one(&mut *tz)
    .await
    .expect("jakarta row");
    drop(tz);

    assert!(
        (epoch_utc - epoch_jkt).abs() < 0.001,
        "the stored instant is zone-independent: {epoch_utc} vs {epoch_jkt}"
    );
    let naive_jkt = chrono::NaiveDateTime::parse_from_str(&rendered_jkt, "%Y-%m-%d %H:%M:%S")
        .expect("jakarta rendering parses");
    // to_char's format keeps whole seconds only.
    let expected_jkt = chrono::SubsecRound::trunc_subsecs(deadline.naive_utc() + Duration::hours(7), 0);
    assert_eq!(
        naive_jkt, expected_jkt,
        "the Jakarta rendering is exactly the UTC instant + 7h"
    );

    db.dispose().await;
}
