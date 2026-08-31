//! Probes 3: the attempt-state monotonic guard — the DB trigger, the
//! state machine's illegal edges, the conditional-transition refusals,
//! and the concurrent-duplicate attempt guard (fail-hard; fresh scratch
//! DB per test).

use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use backbone_survey::application::service::survey_write_service::SurveyWriteError;
use backbone_survey::domain::entity::UserInput;
use backbone_survey::domain::state_machine::survey_input_stateState;

use super::common::*;

/// Is this sqlx error the given Postgres error code (SQLSTATE class)?
fn pg_code(e: &sqlx::Error, code: &str) -> bool {
    match e {
        sqlx::Error::Database(db) => db.code().as_deref() == Some(code),
        _ => false,
    }
}

/// The DB-level monotonic guard: rank-ordered state writes refuse every
/// BACKWARD edge with 23514 — raw SQL included, the service layer
/// included — while forward edges pass. Double-done and done-then-begin
/// refuse through the conditional transitions (zero rows, typed error).
#[tokio::test]
async fn p03_attempt_monotonic_guard() {
    let db = TestDb::new("monotonic").await;
    let svc = Svc::new(db.pool.clone());
    let (survey_id, access_token) = seed_survey(&db.pool).await;
    seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;

    // ── the trigger over raw SQL ─────────────────────────────────────────
    let id = seed_input(&db.pool, survey_id, "monotonic-nonce-1").await;
    assert_eq!(state_of(&db.pool, id).await, "new");

    // Forward: new -> in_progress -> done, both legal.
    sqlx::query(r#"UPDATE survey.survey_user_inputs SET state = 'in_progress' WHERE id = $1"#)
        .bind(id)
        .execute(&db.pool)
        .await
        .expect("new -> in_progress is forward");
    sqlx::query(r#"UPDATE survey.survey_user_inputs SET state = 'done' WHERE id = $1"#)
        .bind(id)
        .execute(&db.pool)
        .await
        .expect("in_progress -> done is forward");

    // Backward from terminal: refused by the trigger (23514).
    let err = sqlx::query(
        r#"UPDATE survey.survey_user_inputs SET state = 'in_progress' WHERE id = $1"#,
    )
    .bind(id)
    .execute(&db.pool)
    .await
    .expect_err("done -> in_progress must refuse");
    assert!(pg_code(&err, "23514"), "expected check_violation, got: {err}");

    // Backward one rung: in_progress -> new, same refusal.
    let id2 = seed_input(&db.pool, survey_id, "monotonic-nonce-2").await;
    sqlx::query(r#"UPDATE survey.survey_user_inputs SET state = 'in_progress' WHERE id = $1"#)
        .bind(id2)
        .execute(&db.pool)
        .await
        .expect("forward");
    let err = sqlx::query(r#"UPDATE survey.survey_user_inputs SET state = 'new' WHERE id = $1"#)
        .bind(id2)
        .execute(&db.pool)
        .await
        .expect_err("in_progress -> new must refuse");
    assert!(pg_code(&err, "23514"), "expected check_violation, got: {err}");
    assert_eq!(state_of(&db.pool, id2).await, "in_progress", "the refused write left no trace");

    // ── the service path: conditional transitions ────────────────────────
    let link = start_attempt(&svc, &access_token).await;
    let sid = input_of(&db.pool, &link).await.id;
    svc.intake.begin(&link).await.expect("begin");
    svc.intake.finish(&link).await.expect("finish");
    // Double-done: zero rows -> the shared refusal, never a second end_datetime.
    let before_end: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar(r#"SELECT end_datetime FROM survey.survey_user_inputs WHERE id = $1"#)
            .bind(sid)
            .fetch_one(&db.pool)
            .await
            .expect("end stamp");
    assert!(before_end.is_some());
    match svc.intake.finish(&link).await {
        Err(SurveyWriteError::AttemptNotSubmittable) => {}
        other => panic!("double finish must refuse, got {other:?}"),
    }
    let after_end: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar(r#"SELECT end_datetime FROM survey.survey_user_inputs WHERE id = $1"#)
            .bind(sid)
            .fetch_one(&db.pool)
            .await
            .expect("end stamp 2");
    assert_eq!(before_end, after_end, "the refused finish must not rewrite the stamp");
    // Begin after done refuses (the terminal edge has no begin).
    match svc.intake.begin(&link).await {
        Err(SurveyWriteError::AttemptNotSubmittable) => {}
        other => panic!("begin after done must refuse, got {other:?}"),
    }

    db.dispose().await;
}

/// The entity state machine (the in-memory first line): the illegal
/// edges are refused BEFORE any SQL runs — new -> done skips a rung,
/// done -> anything is terminal, in_progress -> new is backward.
#[test]
fn p03a_state_machine_illegal_edges() {
    let mut input = UserInput::new(
        Uuid::new_v4(),
        "nonce".into(),
        Utc::now() + chrono::Duration::days(1),
        false,
        backbone_survey::domain::entity::SurveyInputState::New,
        false,
        0.0,
        0.0,
        false,
    );
    // new -> done: no such edge.
    assert!(input.transition_to(survey_input_stateState::Done).is_err(), "new -> done must refuse");
    // new -> in_progress: the begin edge.
    input
        .transition_to(survey_input_stateState::InProgress)
        .expect("new -> in_progress is legal");
    // in_progress -> new: backward.
    assert!(
        input.transition_to(survey_input_stateState::New).is_err(),
        "in_progress -> new must refuse"
    );
    // in_progress -> done: the finish edge.
    input
        .transition_to(survey_input_stateState::Done)
        .expect("in_progress -> done is legal");
    // done -> anything: terminal.
    assert!(input.transition_to(survey_input_stateState::New).is_err(), "done -> new must refuse");
    assert!(
        input.transition_to(survey_input_stateState::InProgress).is_err(),
        "done -> in_progress must refuse"
    );
    assert!(input.transition_to(survey_input_stateState::Done).is_err(), "done -> done must refuse");
}

/// The concurrent-duplicate attempt guard: two SIMULTANEOUS connections
/// inserting attempts under the SAME live token_nonce — the partial
/// unique index admits exactly one; the loser answers 23505 (not a
/// sequential retry, a genuinely racy pair gated by the DB).
#[tokio::test]
async fn p03b_concurrent_duplicate_attempt_refused() {
    let db = TestDb::new("dup_attempt").await;
    let (survey_id, _token) = seed_survey(&db.pool).await;
    let shared_nonce = format!("shared{}", Uuid::new_v4().simple());

    // One dedicated connection per contender, released together by the
    // barrier so the two INSERTs race rather than queue.
    let conn_a = db.pool.acquire().await.expect("conn a");
    let conn_b = db.pool.acquire().await.expect("conn b");
    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let sql = r#"INSERT INTO survey.survey_user_inputs
                     (id, survey_id, token_nonce, token_expires_at)
                 VALUES ($1, $2, $3, now() + interval '30 days')"#;
    let survey_a = survey_id;
    let nonce_a = shared_nonce.clone();
    let gate_a = barrier.clone();
    let task_a = tokio::spawn(async move {
        let mut conn = conn_a;
        gate_a.wait().await;
        sqlx::query(sql)
            .bind(Uuid::new_v4())
            .bind(survey_a)
            .bind(&nonce_a)
            .execute(&mut *conn)
            .await
    });
    let survey_b = survey_id;
    let nonce_b = shared_nonce.clone();
    let gate_b = barrier;
    let task_b = tokio::spawn(async move {
        let mut conn = conn_b;
        gate_b.wait().await;
        sqlx::query(sql)
            .bind(Uuid::new_v4())
            .bind(survey_b)
            .bind(&nonce_b)
            .execute(&mut *conn)
            .await
    });

    let res_a = task_a.await.expect("task a join");
    let res_b = task_b.await.expect("task b join");

    // Exactly one winner; the loser is the unique-violation 23505.
    let wins = [&res_a, &res_b].iter().filter(|r| r.is_ok()).count();
    let uniq = [&res_a, &res_b].iter().filter(|r| matches!(r, Err(e) if pg_code(e, "23505"))).count();
    assert_eq!(wins, 1, "exactly one insert may win — got a:{res_a:?} b:{res_b:?}");
    assert_eq!(uniq, 1, "exactly one unique_violation — got a:{res_a:?} b:{res_b:?}");

    // And exactly ONE attempt row carries the nonce.
    let rows: i64 =
        sqlx::query_scalar(r#"SELECT count(*) FROM survey.survey_user_inputs WHERE token_nonce = $1"#)
            .bind(&shared_nonce)
            .fetch_one(&db.pool)
            .await
            .expect("count nonce rows");
    assert_eq!(rows, 1, "the DB held exactly one live attempt under the nonce");

    db.dispose().await;
}
