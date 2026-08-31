//! Probes 4–6: the frozen scoring denominator, the immutable speed
//! basis, the pure speed-formula table, and 0.0-correct scoring
//! (fail-hard; fresh scratch DB per test).

use uuid::Uuid;

use backbone_survey::application::service::intake_service::AnswerDraft;
use backbone_survey::application::service::scoring_service::{
    speed_factor, SPEED_FULL_CREDIT_SECONDS,
};
use backbone_survey::application::service::survey_write_service::SurveyWriteError;

use super::common::*;

fn pg_code(e: &sqlx::Error, code: &str) -> bool {
    match e {
        sqlx::Error::Database(db) => db.code().as_deref() == Some(code),
        _ => false,
    }
}

/// The denominator freezes VALUES at entry: a mid-attempt weight edit
/// and a mid-attempt question add cannot move an in-flight attempt's
/// math — the recompute stays byte-identical against the snapshot, and
/// the new question is unanswerable (frozen-set membership).
#[tokio::test]
async fn p04_denominator_frozen() {
    let db = TestDb::new("scor_frozen").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_scored_survey(&db.pool, 50.0).await;
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 10.0, true).await;
    let q2 = seed_question(&db.pool, survey_id, 20, "numerical_box", 10.0, true).await;

    let link = start_attempt(&svc, &token).await;
    let input_id = input_of(&db.pool, &link).await.id;
    svc.intake.begin(&link).await.expect("begin");

    // Correct answer on q1: 10/20 = 50% — exactly at the threshold.
    let out = svc
        .intake
        .submit_answer(&link, q1, AnswerDraft::Number(0.0))
        .await
        .expect("submit q1");
    assert_eq!(out.totals.total, 10.0);
    assert_eq!(out.totals.denominator, 20.0, "denominator is the frozen weight sum");
    assert_eq!(out.totals.percentage, 50.0);
    assert!(out.totals.success, "50% >= 50 clears the threshold mid-attempt");

    // ── mid-attempt mutations: weight edit + brand-new question ────────
    sqlx::query(r#"UPDATE survey.survey_questions SET answer_score = 999 WHERE id = $1"#)
        .bind(q1)
        .execute(&db.pool)
        .await
        .expect("edit q1 weight");
    let q3 = seed_question(&db.pool, survey_id, 30, "numerical_box", 50.0, true).await;

    // The recompute (triggered by the next submit) must be byte-identical
    // against the FROZEN values — not 1009/1029, not a 70 denominator.
    let out2 = svc
        .intake
        .submit_answer(&link, q2, AnswerDraft::Number(0.0))
        .await
        .expect("submit q2");
    assert_eq!(out2.totals.total, 20.0, "frozen weights: total unchanged by the edit");
    assert_eq!(
        out2.totals.denominator, 20.0,
        "frozen denominator: neither the 999 weight nor the new 50-weight question moved it"
    );
    assert_eq!(out2.totals.percentage, 100.0);

    // A question added after entry is NOT in the frozen set: unanswerable.
    match svc.intake.submit_answer(&link, q3, AnswerDraft::Number(0.0)).await {
        Err(SurveyWriteError::ValidationFailed { reason, .. }) => {
            assert!(reason.contains("frozen"), "the refusal names the frozen set: {reason}");
        }
        other => panic!("post-entry question must refuse, got {other:?}"),
    }

    // A soft-deleted question disappears from the answerable set entirely.
    sqlx::query(
        r#"UPDATE survey.survey_questions
           SET metadata = metadata || '{"deleted_at": "2026-01-01T00:00:00Z"}'::jsonb
           WHERE id = $1"#,
    )
    .bind(q2)
    .execute(&db.pool)
    .await
    .expect("soft-delete q2");
    match svc.intake.submit_answer(&link, q2, AnswerDraft::Number(0.0)).await {
        Err(SurveyWriteError::QuestionNotFound(_)) => {}
        other => panic!("soft-deleted question must refuse, got {other:?}"),
    }

    // The frozen snapshot itself survives untouched: the metadata still
    // carries the mint-time weights for both questions.
    let w1: f64 = sqlx::query_scalar(
        r#"SELECT (metadata->>'frozen_answer_score')::float8
           FROM survey.survey_user_input_predefined_questions
           WHERE user_input_id = $1 AND question_id = $2"#,
    )
    .bind(input_id)
    .bind(q1)
    .fetch_one(&db.pool)
    .await
    .expect("frozen weight q1");
    let w2: f64 = sqlx::query_scalar(
        r#"SELECT (metadata->>'frozen_answer_score')::float8
           FROM survey.survey_user_input_predefined_questions
           WHERE user_input_id = $1 AND question_id = $2"#,
    )
    .bind(input_id)
    .bind(q2)
    .fetch_one(&db.pool)
    .await
    .expect("frozen weight q2");
    assert_eq!((w1, w2), (10.0, 10.0), "the snapshot kept the mint-time weights");

    db.dispose().await;
}

/// The stored speed basis is immutable: after the wall clock moves (the
/// session clock warped an hour back), a recompute folds the STORED line
/// scores unchanged and the sanctioned regrade reproduces the score from
/// the STORED speed_seconds — never a fresh wall read. The write-once
/// drift trigger refuses any other score rewrite (23514).
#[tokio::test]
async fn p05_speed_snapshot_immutable() {
    let db = TestDb::new("scor_speed").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, _token) =
        seed_survey_opts(&db.pool, "scoring_with_answers", 50.0, None, None, true, Some(30)).await;
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 10.0, true).await;
    let q2 = seed_question(&db.pool, survey_id, 20, "numerical_box", 10.0, true).await;
    // The speed window rides the question's time limit.
    sqlx::query(
        r#"UPDATE survey.survey_questions SET is_time_limited = TRUE, time_limit = 30 WHERE id = $1"#,
    )
    .bind(q1)
    .execute(&db.pool)
    .await
    .expect("time-limit q1");

    // Session attendee: arm, join, advance onto q1 (stored clock = now+1s).
    let armed = svc.writes.arm_session(survey_id).await.expect("arm");
    let code = armed.session_code.clone().expect("code");
    let ticket = svc
        .attempts
        .join_by_code(&code, "zoe", "10.0.0.9", "wire-zoe", Some("zoe"))
        .await
        .expect("join");
    let link = ticket.link.clone();
    svc.writes.advance_session(survey_id).await.expect("advance to q1");

    // Let a real interval elapse so the stored basis is non-zero.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let out = svc
        .intake
        .submit_answer(&link, q1, AnswerDraft::Number(0.0))
        .await
        .expect("submit under speed rating");
    let stored_speed = out.line.speed_seconds.expect("speed captured");
    assert!((1..=4).contains(&stored_speed), "elapsed basis ~2s, got {stored_speed}");
    let graded = out.line.answer_score.expect("scored under the speed window");
    // The submit grades from the millisecond elapsed; the stored basis is
    // the truncated second — the graded value must sit inside that
    // one-second band around the stored basis's factors.
    let band_lo = 10.0 * speed_factor((stored_speed + 1) as f64, 30.0) - 1e-9;
    let band_hi = 10.0 * speed_factor(stored_speed.saturating_sub(1).max(0) as f64, 30.0) + 1e-9;
    assert!(
        (band_lo..=band_hi).contains(&graded),
        "graded {graded} inside the stored-basis band [{band_lo}, {band_hi}]"
    );

    let line_id: Uuid = sqlx::query_scalar(
        r#"SELECT id FROM survey.survey_user_input_lines
           WHERE question_id = $1 AND user_input_id = $2"#,
    )
    .bind(q1)
    .bind(input_of(&db.pool, &link).await.id)
    .fetch_one(&db.pool)
    .await
    .expect("line id");

    // ── the write-once drift trigger: any other score rewrite refuses ──
    let err = sqlx::query(
        r#"UPDATE survey.survey_user_input_lines SET answer_score = 999 WHERE id = $1"#,
    )
    .bind(line_id)
    .execute(&db.pool)
    .await
    .expect_err("score rewrite must refuse");
    assert!(pg_code(&err, "23514"), "expected the drift refusal, got: {err}");

    // ── the wall-clock trap: warp the session clock an hour back ───────
    sqlx::query(
        r#"UPDATE survey.survey_surveys
           SET session_question_start_time = now() - interval '1 hour'
           WHERE id = $1"#,
    )
    .bind(survey_id)
    .execute(&db.pool)
    .await
    .expect("warp clock");

    // A recompute (the next submit folds the input triple) keeps the
    // STORED line score — the fold never re-derives from any clock.
    let q1_stored: f64 = sqlx::query_scalar(
        r#"SELECT answer_score FROM survey.survey_user_input_lines WHERE id = $1"#,
    )
    .bind(line_id)
    .fetch_one(&db.pool)
    .await
    .expect("stored q1 line score");
    let out2 = svc
        .intake
        .submit_answer(&link, q2, AnswerDraft::Number(0.0))
        .await
        .expect("submit q2");
    assert!(
        (out2.totals.total - (q1_stored + 10.0)).abs() < 1e-9,
        "recompute folded the stored q1 score, got {}",
        out2.totals.total
    );

    // The sanctioned regrade reproduces q1's score from the STORED basis:
    // a wall-clock re-derivation would grade elapsed=3600s → the 0.5
    // floor → 5.0. The stored basis keeps it near the original (within
    // the one-second truncation step of the speed factor's slope).
    let touched = svc.scoring.regrade_question(survey_id, q1).await.expect("regrade");
    assert_eq!(touched.len(), 1, "one line regraded");
    let (lid, old, new) = touched[0];
    assert_eq!(lid, line_id);
    let old = old.expect("old score");
    let new = new.expect("new score");
    assert!(
        new > 9.0,
        "regrade reproduced from the stored basis, not the warped wall clock: {new}"
    );
    assert!(
        (old - new).abs() <= 10.0 * 0.5 / 28.0 + 1e-9,
        "old {old} vs stored-basis new {new} within one truncation step"
    );

    // The audit pair landed on the line's metadata.
    let history_len: i64 = sqlx::query_scalar(
        r#"SELECT jsonb_array_length(COALESCE(metadata->'regrade_history', '[]'::jsonb))::int8
           FROM survey.survey_user_input_lines WHERE id = $1"#,
    )
    .bind(line_id)
    .fetch_one(&db.pool)
    .await
    .expect("history length");
    assert_eq!(history_len, 1, "one regrade audit entry");

    db.dispose().await;
}

/// The speed formula as a pure table: full credit under 2 s, the 50 %
/// floor over the limit, linear decay between, degenerate windows.
#[test]
fn p06_speed_formula_table() {
    assert_eq!(SPEED_FULL_CREDIT_SECONDS, 2.0);

    // Full credit: anything under 2 s (and exactly the degenerate limit).
    assert_eq!(speed_factor(0.0, 30.0), 1.0);
    assert_eq!(speed_factor(1.999, 30.0), 1.0);
    assert_eq!(speed_factor(2.0, 30.0), 1.0, "the linear leg starts at 1.0");

    // The linear leg: 0.5 * (1 + (limit - elapsed)/(limit - 2)).
    assert!((speed_factor(16.0, 30.0) - 0.75).abs() < 1e-12, "half-credit point is 75 %");
    assert!((speed_factor(16.0, 30.0) - 0.5 * (1.0 + 14.0 / 28.0)).abs() < 1e-12);
    assert_eq!(speed_factor(30.0, 30.0), 0.5, "at the limit: exactly 50 %");
    assert!((speed_factor(29.0, 30.0) - 0.5 * (1.0 + 1.0 / 28.0)).abs() < 1e-12);

    // The floor: anything over the limit.
    assert_eq!(speed_factor(30.001, 30.0), 0.5);
    assert_eq!(speed_factor(3600.0, 30.0), 0.5);

    // Degenerate windows (limit <= 2 s): step, not line.
    assert_eq!(speed_factor(0.0, 2.0), 1.0);
    assert_eq!(speed_factor(2.0, 2.0), 1.0, "elapsed == degenerate limit is full credit");
    assert_eq!(speed_factor(2.001, 2.0), 0.5);
    assert_eq!(speed_factor(0.0, 1.0), 1.0);
    assert_eq!(speed_factor(1.5, 1.0), 0.5);

    // Monotone non-increasing across the whole window.
    let mut last = 1.0;
    let mut elapsed = 0.0;
    while elapsed <= 30.5 {
        let f = speed_factor(elapsed, 30.0);
        assert!(f <= last + 1e-12, "speed factor must not increase at elapsed={elapsed}");
        last = f;
        elapsed += 0.25;
    }
}

/// 0.0-correct answers ARE scoreable: correctness is explicit Option
/// equality, never a truthiness check. A wrong non-zero answer scores
/// Some(0.0), not None.
#[tokio::test]
async fn p06b_zero_correct_scoring() {
    let db = TestDb::new("scor_zero").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_scored_survey(&db.pool, 50.0).await;
    // Both questions' correct answer is exactly 0.0, weight 10.
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 10.0, true).await;
    let q2 = seed_question(&db.pool, survey_id, 20, "numerical_box", 10.0, true).await;

    let link = start_attempt(&svc, &token).await;
    svc.intake.begin(&link).await.expect("begin");

    // The correct 0.0 answer: full credit, correct=true — the truthiness
    // defect (0.0 treated as wrong/blank) must not port.
    let out = svc
        .intake
        .submit_answer(&link, q1, AnswerDraft::Number(0.0))
        .await
        .expect("submit correct 0.0");
    assert_eq!(out.line.answer_is_correct, Some(true), "0.0 == 0.0 is CORRECT");
    assert_eq!(out.line.answer_score, Some(10.0), "the 0.0-correct answer earns its weight");
    assert_eq!(out.totals.total, 10.0);

    // The wrong answer on q2: Some(0.0) — a scored zero, not an unscored
    // None (the denominator still counts it).
    let out2 = svc
        .intake
        .submit_answer(&link, q2, AnswerDraft::Number(7.5))
        .await
        .expect("submit wrong");
    assert_eq!(out2.line.answer_is_correct, Some(false));
    assert_eq!(out2.line.answer_score, Some(0.0), "wrong = scored zero");
    assert_eq!(out2.totals.total, 10.0);
    assert_eq!(out2.totals.denominator, 20.0);
    assert_eq!(out2.totals.percentage, 50.0);

    db.dispose().await;
}

/// The snapshot-row count of one input (the prune's witness).
async fn snapshot_count(pool: &sqlx::PgPool, input_id: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*) FROM survey.survey_user_input_predefined_questions
           WHERE user_input_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
    )
    .bind(input_id)
    .fetch_one(pool)
    .await
    .expect("snapshot count")
}

/// Probe 19: the finish-time prune — a conditional whose trigger never
/// fired leaves the frozen denominator on the terminal edge, while a
/// triggered conditional and every unanswered NON-conditional stay.
#[tokio::test]
async fn p19_finish_prunes_unfired_conditionals() {
    let db = TestDb::new("scor_prune").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_scored_survey(&db.pool, 50.0).await;

    // q1 numerical (10, scored) + a choice parent (10, scored) whose
    // label A triggers a conditional numerical (10, scored).
    let q1 = seed_question(&db.pool, survey_id, 10, "numerical_box", 10.0, true).await;
    let parent = seed_question(&db.pool, survey_id, 20, "simple_choice", 10.0, true).await;
    let label_a = seed_label(&db.pool, parent, "A", true, 10.0).await;
    let label_b = seed_label(&db.pool, parent, "B", false, 0.0).await;
    let cond = seed_question(&db.pool, survey_id, 30, "numerical_box", 10.0, true).await;
    seed_trigger(&db.pool, cond, label_a).await;

    // ── attempt 1: the trigger never fires (B chosen) ─────────────────
    let link1 = start_attempt(&svc, &token).await;
    let input1 = input_of(&db.pool, &link1).await.id;
    svc.intake.begin(&link1).await.expect("begin");
    svc.intake
        .submit_answer(&link1, parent, AnswerDraft::Choice(vec![label_b]))
        .await
        .expect("choose B");
    svc.intake
        .submit_answer(&link1, q1, AnswerDraft::Number(0.0))
        .await
        .expect("q1 correct");
    let final1 = svc.intake.finish(&link1).await.expect("finish");

    // Without the prune the denominator would be 30 (10/30 = 33% — a
    // FAIL at the 50 threshold, dinged for a question never shown).
    // With it: 10/20 = 50% — exactly at the threshold, a PASS.
    assert_eq!(final1.scoring_percentage, 50.0, "unfired conditional left the denominator");
    assert!(final1.scoring_success, "10/20 clears the 50 threshold");
    assert_eq!(
        snapshot_count(&db.pool, input1).await,
        2,
        "only q1 + the parent remain in the frozen set"
    );

    // ── attempt 2: the trigger fires (A chosen) ───────────────────────
    let link2 = start_attempt(&svc, &token).await;
    let input2 = input_of(&db.pool, &link2).await.id;
    svc.intake.begin(&link2).await.expect("begin");
    svc.intake
        .submit_answer(&link2, parent, AnswerDraft::Choice(vec![label_a]))
        .await
        .expect("choose A");
    svc.intake
        .submit_answer(&link2, cond, AnswerDraft::Number(0.0))
        .await
        .expect("conditional is answerable — its snapshot row stayed");
    svc.intake
        .submit_answer(&link2, q1, AnswerDraft::Number(0.0))
        .await
        .expect("q1 correct");
    let final2 = svc.intake.finish(&link2).await.expect("finish");
    assert_eq!(final2.scoring_percentage, 100.0, "30/30 — the triggered conditional stays");
    assert_eq!(snapshot_count(&db.pool, input2).await, 3, "nothing pruned on the triggered path");

    // ── attempt 3: empty submission — non-conditionals are NEVER pruned
    let link3 = start_attempt(&svc, &token).await;
    let input3 = input_of(&db.pool, &link3).await.id;
    let final3 = svc.intake.finish(&link3).await.expect("empty finish");
    assert_eq!(final3.scoring_percentage, 0.0);
    assert!(!final3.scoring_success);
    assert_eq!(
        snapshot_count(&db.pool, input3).await,
        2,
        "an unanswered NON-conditional stays in the denominator (blank = 0 points, never exempt)"
    );

    db.dispose().await;
}
