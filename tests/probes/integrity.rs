//! The scoring, attempt-pool and public-surface integrity probes: a taker can
//! neither score past a perfect attempt, nor take a limited survey more often
//! than its limit, nor read a verdict before finishing; concurrent submits on
//! one attempt store one answer; a regrade keeps choice scores; the session
//! code lockout counts a caller across codes; database refusals answer typed.

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use backbone_survey::application::service::intake_service::AnswerDraft;
use backbone_survey::application::service::survey_write_service::SurveyWriteError;
use backbone_survey::SurveyModule;
use tower::ServiceExt;
use uuid::Uuid;

use super::common::*;

async fn line_count(pool: &sqlx::PgPool, question: Uuid) -> i64 {
    sqlx::query_scalar(
        r#"SELECT count(*) FROM survey.survey_user_input_lines
           WHERE question_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
    )
    .bind(question)
    .fetch_one(pool)
    .await
    .expect("line count")
}

/// Repeating a correct label, adding a wrong unweighted label, or naming a label of
/// another question cannot carry a score past 100 %; a single choice takes one label;
/// a question with two correct labels counts both in its maximum.
#[tokio::test]
async fn i01_choice_scores_cannot_pass_a_perfect_attempt() {
    let db = TestDb::new("int_inflate").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_survey_opts(&db.pool, "scoring_with_answers", 50.0, None, None, true, None).await;
    let multi = seed_question(&db.pool, survey_id, 10, "multiple_choice", 10.0, true).await;
    let right = seed_label(&db.pool, multi, "right", true, 0.0).await;
    let wrong = seed_label(&db.pool, multi, "wrong", false, 0.0).await;
    let single = seed_question(&db.pool, survey_id, 20, "simple_choice", 10.0, true).await;
    let s_right = seed_label(&db.pool, single, "right", true, 0.0).await;
    let s_wrong = seed_label(&db.pool, single, "wrong", false, 0.0).await;
    let foreign = seed_label(&db.pool, single, "foreign", true, 0.0).await;
    let link = start_attempt(&svc, &token).await;

    // One correct label sent three times is one correct label.
    let out = svc
        .intake
        .submit_answer(&link, multi, AnswerDraft::Choice(vec![right, right, right]))
        .await
        .expect("repeated label");
    assert_eq!(line_count(&db.pool, multi).await, 1, "a label is stored once");
    assert!(out.totals.percentage <= 100.0, "got {}", out.totals.percentage);

    // A wrong label with no weight of its own earns nothing.
    let out = svc
        .intake
        .submit_answer(&link, multi, AnswerDraft::Choice(vec![right, wrong]))
        .await
        .expect("right plus wrong");
    assert_eq!(out.totals.total, 10.0, "the wrong label added nothing");

    // A single choice takes one label; a label of another question is refused.
    match svc.intake.submit_answer(&link, single, AnswerDraft::Choice(vec![s_right, s_wrong])).await {
        Err(SurveyWriteError::ValidationFailed { .. }) => {}
        other => panic!("two labels on a single choice must refuse, got {other:?}"),
    }
    match svc.intake.submit_answer(&link, multi, AnswerDraft::Choice(vec![foreign])).await {
        Err(SurveyWriteError::ValidationFailed { .. }) => {}
        other => panic!("another question's label must refuse, got {other:?}"),
    }
    let out = svc
        .intake
        .submit_answer(&link, single, AnswerDraft::Choice(vec![s_right]))
        .await
        .expect("single right");
    assert_eq!(out.totals.percentage, 100.0, "both questions answered right is exactly 100 %");
    let done = svc.intake.finish(&link).await.expect("finish");
    assert_eq!(done.scoring_percentage, 100.0);

    // Two correct labels: the maximum counts both, so both chosen is 100 %, not 200 %.
    let (survey_b, token_b) = seed_survey_opts(&db.pool, "scoring_with_answers", 50.0, None, None, true, None).await;
    let q = seed_question(&db.pool, survey_b, 10, "multiple_choice", 10.0, true).await;
    let a = seed_label(&db.pool, q, "a", true, 0.0).await;
    let b = seed_label(&db.pool, q, "b", true, 0.0).await;
    let link_b = start_attempt(&svc, &token_b).await;
    let out = svc
        .intake
        .submit_answer(&link_b, q, AnswerDraft::Choice(vec![a, b]))
        .await
        .expect("both correct");
    assert_eq!(out.totals.percentage, 100.0);
    assert_eq!(out.totals.denominator, 20.0);

    db.dispose().await;
}

/// Identical scalar submits racing on one attempt store one line.
#[tokio::test]
async fn i02_concurrent_submits_store_one_answer() {
    let db = TestDb::new("int_race").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_survey_opts(&db.pool, "scoring_with_answers", 50.0, None, None, true, None).await;
    for round in 0..5 {
        let q = seed_question(&db.pool, survey_id, 10 + round, "numerical_box", 10.0, true).await;
        let link = start_attempt(&svc, &token).await;
        svc.intake.begin(&link).await.expect("begin");
        let submit = || svc.intake.submit_answer(&link, q, AnswerDraft::Number(0.0));
        let (a, b, c, d, e, f) = tokio::join!(submit(), submit(), submit(), submit(), submit(), submit());
        let results = [a, b, c, d, e, f];
        assert!(results.iter().all(|r| r.is_ok()), "every submit lands: {results:?}");
        let lines: i64 = sqlx::query_scalar(
            r#"SELECT count(*) FROM survey.survey_user_input_lines
               WHERE question_id = $1 AND user_input_id = $2"#,
        )
        .bind(q)
        .bind(input_of(&db.pool, &link).await.id)
        .fetch_one(&db.pool)
        .await
        .expect("count");
        assert_eq!(lines, 1, "round {round}: one answer, however many submits raced");
    }
    db.dispose().await;
}

/// A limited survey counts a signed-in taker, never an email the caller types: an
/// anonymous start is refused, a repeated start resumes the open attempt, and the
/// limit binds once the finished attempts reach it.
#[tokio::test]
async fn i03_attempt_limit_counts_the_taker() {
    let db = TestDb::new("int_limit").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_survey_opts(&db.pool, "no_scoring", 50.0, None, Some(1), false, None).await;
    seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;

    match svc.attempts.public_start(&token, None, None, None).await {
        Err(SurveyWriteError::LoginRequired) => {}
        other => panic!("an anonymous start of a limited survey must refuse, got {other:?}"),
    }
    match svc.attempts.public_start(&token, Some("fresh@example.com"), None, None).await {
        Err(SurveyWriteError::LoginRequired) => {}
        other => panic!("a typed email is not an identity, got {other:?}"),
    }

    let taker = Uuid::new_v4();
    let first = svc.attempts.public_start(&token, None, None, Some(taker)).await.expect("start");
    let again = svc.attempts.public_start(&token, None, None, Some(taker)).await.expect("resume");
    assert_eq!(first.input.id, again.input.id, "an open attempt is resumed, not doubled");
    svc.intake.finish(&first.link).await.expect("finish");
    match svc.attempts.public_start(&token, Some("other@example.com"), None, Some(taker)).await {
        Err(SurveyWriteError::AttemptsExhausted { limit: 1 }) => {}
        other => panic!("the limit binds on the taker, got {other:?}"),
    }
    db.dispose().await;
}

/// A regrade recomputes choice and matrix lines from their rows instead of wiping them.
#[tokio::test]
async fn i04_regrade_keeps_choice_scores() {
    let db = TestDb::new("int_regrade").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_survey_opts(&db.pool, "scoring_with_answers", 50.0, None, None, true, None).await;
    let q = seed_question(&db.pool, survey_id, 10, "multiple_choice", 10.0, true).await;
    let right = seed_label(&db.pool, q, "right", true, 0.0).await;
    seed_label(&db.pool, q, "wrong", false, 0.0).await;
    let link = start_attempt(&svc, &token).await;
    svc.intake.submit_answer(&link, q, AnswerDraft::Choice(vec![right])).await.expect("submit");
    svc.intake.finish(&link).await.expect("finish");

    let touched = svc.scoring.regrade_question(survey_id, q).await.expect("regrade");
    assert_eq!(touched.len(), 1);
    assert_eq!(touched[0].2, Some(10.0), "the correct choice keeps its score");
    let row = input_of(&db.pool, &link).await;
    assert_eq!(row.scoring_percentage, 100.0, "the attempt keeps its total");
    db.dispose().await;
}

async fn call(router: &axum::Router, req: Request<Body>) -> StatusCode {
    let resp = router.clone().oneshot(req).await.expect("router call");
    let status = resp.status();
    let _ = to_bytes(resp.into_body(), usize::MAX).await;
    status
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().method("GET").uri(uri).body(Body::empty()).expect("request builds")
}

/// The certification page reads the verdict of a finished attempt only: a link to a
/// running attempt is refused, the same link after finishing is answered.
#[tokio::test]
async fn i05_certification_page_waits_for_the_finish() {
    let db = TestDb::new("int_certpage").await;
    ensure_token_secret();
    let module = Arc::new(
        SurveyModule::builder().with_database(db.pool.clone()).build().expect("module composes"),
    );
    let public = module.survey_public_routes();
    let (survey_id, token) = seed_survey_opts(&db.pool, "scoring_with_answers", 50.0, None, None, true, None).await;
    let q = seed_question(&db.pool, survey_id, 10, "numerical_box", 10.0, true).await;
    let ticket = module.attempt_service().public_start(&token, None, None, None).await.expect("start");
    module
        .intake_service()
        .submit_answer(&ticket.link, q, AnswerDraft::Number(0.0))
        .await
        .expect("correct answer");
    let uri = format!("/survey/attempt/{}/certification", ticket.link);
    assert_eq!(call(&public, get(&uri)).await, StatusCode::CONFLICT, "no verdict mid-attempt");
    module.intake_service().finish(&ticket.link).await.expect("finish");
    assert_eq!(call(&public, get(&uri)).await, StatusCode::OK, "the verdict after the finish");
    db.dispose().await;
}

/// One caller guessing a different code each time is locked out like one guessing the
/// same code: the book counts the caller, not the code.
#[tokio::test]
async fn i06_code_lockout_counts_the_caller_across_codes() {
    let db = TestDb::new("int_walk").await;
    let svc = Svc::new(db.pool.clone());
    for (i, code) in ["1111", "2222", "3333"].iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(1050)).await;
        }
        let _ = svc.attempts.join_by_code(code, &format!("walker-{i}"), "10.9.9.9", None).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(1050)).await;
    match svc.attempts.join_by_code("4444", "walker-4", "10.9.9.9", None).await {
        Err(SurveyWriteError::SessionCodeLocked { .. }) => {}
        other => panic!("a caller walking codes must be locked out, got {other:?}"),
    }
    db.dispose().await;
}

/// A unique violation answers as a conflict, not an internal error.
#[tokio::test]
async fn i07_database_refusals_answer_typed() {
    let db = TestDb::new("int_typed").await;
    let (survey_id, _) = seed_survey(&db.pool).await;
    let e = sqlx::query("INSERT INTO survey.survey_surveys (id, title, access_token) VALUES ($1, 'dup', 'dup')")
        .bind(survey_id)
        .execute(&db.pool)
        .await
        .expect_err("duplicate id");
    let typed = SurveyWriteError::from(e);
    assert!(matches!(typed, SurveyWriteError::Conflict(_)), "got {typed:?}");
    assert_eq!(typed.http_status(), 409);
    db.dispose().await;
}

/// A finished attempt's lines are fixed: deleting one or adding one outside the regrade
/// refuses with the drift rule's code, so a delete-and-reinsert cannot rewrite a score.
#[tokio::test]
async fn i10_a_done_attempt_lines_cannot_be_deleted_or_added() {
    let db = TestDb::new("int_churn").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_survey_opts(&db.pool, "scoring_with_answers", 50.0, None, None, true, None).await;
    let q = seed_question(&db.pool, survey_id, 10, "simple_choice", 10.0, true).await;
    let _right = seed_label(&db.pool, q, "right", true, 0.0).await;
    let wrong = seed_label(&db.pool, q, "wrong", false, 0.0).await;
    let link = start_attempt(&svc, &token).await;
    svc.intake.submit_answer(&link, q, AnswerDraft::Choice(vec![wrong])).await.expect("answer");
    let done = svc.intake.finish(&link).await.expect("finish");

    let refused = sqlx::query("DELETE FROM survey.survey_user_input_lines WHERE user_input_id = $1")
        .bind(done.id)
        .execute(&db.pool)
        .await
        .expect_err("a done attempt's line cannot be deleted");
    let e = refused.as_database_error().expect("a database refusal");
    assert_eq!(e.constraint(), Some("ck_survey_user_input_lines_score_drift_refused__survey_score_drift_refused"), "{e:?}");

    let refused = sqlx::query(
        r#"INSERT INTO survey.survey_user_input_lines (id, user_input_id, survey_id, question_id, skipped, answer_score)
           VALUES ($1, $2, $3, $4, false, 10)"#,
    )
    .bind(Uuid::new_v4())
    .bind(done.id)
    .bind(survey_id)
    .bind(q)
    .execute(&db.pool)
    .await
    .expect_err("a done attempt takes no new line");
    let e = refused.as_database_error().expect("a database refusal");
    assert_eq!(e.constraint(), Some("ck_survey_user_input_lines_score_drift_refused__survey_score_drift_refused"), "{e:?}");

    // Deleting the attempt itself takes its lines with it.
    sqlx::query("DELETE FROM survey.survey_user_inputs WHERE id = $1").bind(done.id).execute(&db.pool).await.expect("the attempt deletes");
    assert_eq!(line_count(&db.pool, q).await, 0);
    db.dispose().await;
}

/// Answering a choice with nothing replaces what it answered before and clears the
/// conditional questions its earlier answer had opened.
#[tokio::test]
async fn i11_an_empty_choice_clears_earlier_lines_and_conditionals() {
    let db = TestDb::new("int_empty").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, token) = seed_survey_opts(&db.pool, "scoring_with_answers", 50.0, None, None, true, None).await;
    let q = seed_question(&db.pool, survey_id, 10, "simple_choice", 10.0, true).await;
    let opens = seed_label(&db.pool, q, "opens", true, 0.0).await;
    let follow = seed_question(&db.pool, survey_id, 20, "numerical_box", 10.0, true).await;
    seed_trigger(&db.pool, follow, opens).await;
    let link = start_attempt(&svc, &token).await;
    svc.intake.submit_answer(&link, q, AnswerDraft::Choice(vec![opens])).await.expect("answer");
    svc.intake.submit_answer(&link, follow, AnswerDraft::Number(3.0)).await.expect("conditional");

    let out = svc.intake.submit_answer(&link, q, AnswerDraft::Choice(vec![])).await.expect("empty choice");
    let chosen: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM survey.survey_user_input_lines WHERE question_id = $1 AND NOT skipped",
    )
    .bind(q)
    .fetch_one(&db.pool)
    .await
    .unwrap();
    assert_eq!(chosen, 0, "the earlier choice is gone");
    assert_eq!(line_count(&db.pool, q).await, 1, "one skipped line stands for the question");
    assert_eq!(line_count(&db.pool, follow).await, 0, "the conditional it opened is cleared");
    assert_eq!(out.totals.total, 0.0, "nothing scores");
    db.dispose().await;
}
