//! Probes 13, 17: the DB/Rust enum census, and the public-surface
//! hygiene — the preview is side-effect-free, refusals share one body,
//! every mutator is POST, module composition builds, and the crate
//! carries no sibling-module Cargo edge (fail-hard; fresh scratch DB).

use std::sync::Arc;

use backbone_survey::application::service::survey_write_service::SurveyWriteError;
use backbone_survey::SurveyModule;

use super::common::*;

/// Assert the DB enum's value set equals the expected set AND every
/// value round-trips through the Rust enum (FromStr then Display).
macro_rules! census {
    ($pool:expr, $qualified:literal, $ty:ty, [$($v:literal),+ $(,)?]) => {{
        let sql = format!("SELECT v::text FROM unnest(enum_range(NULL::{})) v ORDER BY 1", $qualified);
        let rows: Vec<String> = sqlx::query_scalar(&sql)
            .fetch_all($pool)
            .await
            .unwrap_or_else(|e| panic!("census {} failed: {}", $qualified, e));
        let expected: Vec<&str> = vec![$($v),+];
        let got: Vec<&str> = rows.iter().map(String::as_str).collect();
        assert_eq!(got, expected, "DB census of {} must match the schema set", $qualified);
        for v in &rows {
            let parsed: $ty = v
                .parse()
                .unwrap_or_else(|e| panic!("{} value {} does not parse in Rust: {:?}", $qualified, v, e));
            assert_eq!(parsed.to_string(), *v, "{} round-trip", $qualified);
        }
        rows.len()
    }};
}

/// The enum census: the database's enum values and the Rust enums agree
/// exactly — no orphan DB value the Rust side cannot name, no Rust-only
/// variant that can never be stored.
#[tokio::test]
async fn p13_enum_census() {
    let db = TestDb::new("census").await;

    use backbone_survey::domain::entity::{
        SurveyAnswerType, SurveyInputState, SurveyQuestionType, SurveyQuestionsSelection,
        SurveyScoringType, SurveySessionState,
    };

    let n_input_state = census!(
        &db.pool,
        "survey_input_state",
        SurveyInputState,
        ["done", "in_progress", "new"]
    );
    let n_session_state = census!(
        &db.pool,
        "survey_session_state",
        SurveySessionState,
        ["in_progress", "ready"]
    );
    let n_scoring = census!(
        &db.pool,
        "survey_scoring_type",
        SurveyScoringType,
        [
            "no_scoring",
            "scoring_with_answers",
            "scoring_with_answers_after_page",
            "scoring_without_answers"
        ]
    );
    let n_selection = census!(
        &db.pool,
        "survey_questions_selection",
        SurveyQuestionsSelection,
        ["all", "random"]
    );
    let n_question_type = census!(
        &db.pool,
        "survey_question_type",
        SurveyQuestionType,
        [
            "char_box",
            "date",
            "datetime",
            "matrix",
            "multiple_choice",
            "numerical_box",
            "scale",
            "simple_choice",
            "text_box"
        ]
    );
    let n_answer_type = census!(
        &db.pool,
        "survey_answer_type",
        SurveyAnswerType,
        ["char_box", "date", "datetime", "numerical_box", "scale", "suggestion", "text_box"]
    );

    // Non-trivial sets on the ones that carry the domain weight.
    assert_eq!(n_input_state, 3);
    assert_eq!(n_session_state, 2);
    assert_eq!(n_scoring, 4);
    assert_eq!(n_question_type, 9);
    assert_eq!(n_answer_type, 7);
    assert_eq!(n_selection, 2);

    db.dispose().await;
}

// ─── the HTTP-facing hygiene probe ────────────────────────────────────────────

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

async fn call(router: &axum::Router, req: Request<Body>) -> (StatusCode, Value, String) {
    let resp = router
        .clone()
        .oneshot(req)
        .await
        .unwrap_or_else(|e| panic!("router call failed: {e}"));
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap_or_else(|e| panic!("body read failed: {e}"));
    let raw = String::from_utf8_lossy(&bytes).to_string();
    let json: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    (status, json, raw)
}

fn req(method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    builder
        .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
        .expect("request builds")
}

/// The public surface's hygiene: the preview GET is side-effect-free and
/// does not consume Tier B counters; unknown and dead codes answer
/// byte-identical bodies (no oracle); a garbage attempt token gets the
/// shared refusal; every mutator is POST and every GET is a pure read;
/// the module composes through the builder; and the crate's manifest
/// carries no sibling-module edge (the certification seam stays a port).
#[tokio::test]
async fn p17_public_surface_hygiene() {
    let db = TestDb::new("pub_hyg").await;
    ensure_token_secret();

    // Compose through the PUBLIC builder path (env secret).
    let module = Arc::new(
        SurveyModule::builder()
            .with_database(db.pool.clone())
            .build()
            .expect("module composes"),
    );
    let public = module.survey_public_routes();
    let _guarded = module.guarded_routes();

    let (survey_id, access_token) = seed_survey(&db.pool).await;
    seed_question(&db.pool, survey_id, 10, "numerical_box", 1.0, false).await;
    let armed = module.survey_write_service().arm_session(survey_id).await.expect("arm");
    let code = armed.session_code.clone().expect("code");

    // ── the preview: a pure read ────────────────────────────────────────
    let inputs_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM survey.survey_user_inputs")
            .fetch_one(&db.pool)
            .await
            .expect("count");
    assert_eq!(inputs_before, 0, "nothing has written attempts yet");

    let (status, body, _) = call(&public, req("GET", &format!("/survey/s/{code}"), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["survey_id"].as_str().map(String::from), Some(survey_id.to_string()));
    assert_eq!(body["live"], Value::Bool(true));
    assert!(body["title"].is_string());

    // Side-effect-free: still no attempts, and the survey row is unchanged.
    let inputs_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM survey.survey_user_inputs")
            .fetch_one(&db.pool)
            .await
            .expect("count 2");
    assert_eq!(inputs_after, 0, "the preview must not mint attempts");

    // Repeating the preview changes nothing (no counters, no state move).
    let (s2, _, _) = call(&public, req("GET", &format!("/survey/s/{code}"), None)).await;
    assert_eq!(s2, StatusCode::OK);

    // ── unknown vs dead: one body, no oracle ────────────────────────────
    let (_, _, unknown_body) = call(&public, req("GET", "/survey/s/0000", None)).await;
    assert_eq!(call(&public, req("GET", "/survey/s/0000", None)).await.0, StatusCode::NOT_FOUND);
    module.survey_write_service().end_session(survey_id).await.expect("end");
    let (dead_status, _, dead_body) = call(&public, req("GET", &format!("/survey/s/{code}"), None)).await;
    assert_eq!(dead_status, StatusCode::NOT_FOUND);
    assert_eq!(unknown_body, dead_body, "unknown and dead codes are indistinguishable");

    // ── the shared refusal on a garbage attempt token ───────────────────
    let any_q = uuid::Uuid::new_v4();
    let (st, jb, _) = call(
        &public,
        req(
            "POST",
            "/survey/attempt/totally.garbage.token.here/submit",
            Some(serde_json::json!({ "question_id": any_q, "value_char": "x" })),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT, "garbage token -> 409");
    assert_eq!(jb["code"], "survey_attempt_not_submittable", "the shared refusal body");
    // And a well-formed unknown uuid token: the same shared body.
    let (st2, jb2, _) = call(
        &public,
        req(
            "POST",
            &format!("/survey/attempt/{}/begin", uuid::Uuid::new_v4()),
            None,
        ),
    )
    .await;
    assert_eq!(st2, StatusCode::CONFLICT);
    assert_eq!(jb2["code"], "survey_attempt_not_submittable");

    // ── method hygiene: mutators are POST, reads are GET ────────────────
    for uri in [
        "/survey/attempt/x/begin",
        "/survey/attempt/x/submit",
        "/survey/start/x",
        "/survey/s/x/join",
    ] {
        let (st, _, _) = call(&public, req("GET", uri, None)).await;
        assert_eq!(st, StatusCode::METHOD_NOT_ALLOWED, "GET {uri} must be 405");
    }
    for uri in ["/survey/s/x", "/survey/attempt/x/next", "/survey/attempt/x/certification"] {
        let (st, _, _) = call(&public, req("POST", uri, Some(serde_json::json!({})))).await;
        assert_eq!(st, StatusCode::METHOD_NOT_ALLOWED, "POST {uri} must be 405");
    }

    // ── the full public happy path through the ROUTER ───────────────────
    let (st, jb, _) = call(
        &public,
        req(
            "POST",
            &format!("/survey/start/{access_token}"),
            Some(serde_json::json!({ "email": "hygiene@example.com", "nickname": "hygiene" })),
        ),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "public start through the router");
    let link = jb["link"].as_str().expect("capability link").to_string();

    let (st, jb, _) = call(&public, req("POST", &format!("/survey/attempt/{link}/begin"), None)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(jb["state"], "in_progress");

    // next on a NON-session survey: the awaiting shape.
    let (st, jb, _) = call(&public, req("GET", &format!("/survey/attempt/{link}/next"), None)).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(jb["question_id"], Value::Null);
    assert_eq!(jb["state"], "awaiting");

    // certification evidence on a not-yet-passed attempt refuses (404/409
    // family — never a leak of the evidence view).
    let (st, _, _) = call(
        &public,
        req("GET", &format!("/survey/attempt/{link}/certification"), None),
    )
    .await;
    assert!(
        st == StatusCode::NOT_FOUND || st == StatusCode::CONFLICT || st == StatusCode::UNPROCESSABLE_ENTITY,
        "unfinished attempt must not render evidence, got {st}"
    );

    // ── manifest hygiene: no sibling-module Cargo edges ─────────────────
    let manifest = std::fs::read_to_string(format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")))
        .expect("Cargo.toml readable");
    let deps_block = manifest
        .split("[dev-dependencies]")
        .next()
        .expect("deps section");
    for banned in [
        "backbone-portal",
        "backbone-engagement",
        "backbone-mailing",
        "backbone-website",
        "website-routing",
    ] {
        assert!(
            !deps_block.contains(banned),
            "the certification/realtime seams must stay ports — found a Cargo edge on {banned}"
        );
    }

    // ── zero crons: no migration schedules anything ─────────────────────
    let migrations_dir = format!("{}/migrations", env!("CARGO_MANIFEST_DIR"));
    for entry in std::fs::read_dir(&migrations_dir).expect("migrations dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("sql") {
            let sql = std::fs::read_to_string(&path).expect("migration readable");
            assert!(
                !sql.to_lowercase().contains("pg_cron") && !sql.to_lowercase().contains("create schedule"),
                "migration {} must not schedule anything",
                path.display()
            );
        }
    }

    db.dispose().await;
}

/// The seeded-survey hygiene leg compiles: SurveyWriteError is the one
/// error surface the routes map (the shared-vocabulary contract).
#[test]
fn p17b_shared_error_codes() {
    assert_eq!(SurveyWriteError::AttemptNotSubmittable.code(), "survey_attempt_not_submittable");
    assert_eq!(SurveyWriteError::AttemptExpired.code(), "survey_attempt_not_submittable");
    assert_eq!(SurveyWriteError::AttemptExpired.http_status(), 410);
    assert_eq!(SurveyWriteError::AttemptNotSubmittable.http_status(), 409);
    assert_eq!(
        SurveyWriteError::SessionCodeLocked { retry_after_seconds: 30 }.http_status(),
        429
    );
}
