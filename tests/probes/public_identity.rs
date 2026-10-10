//! The public start carries no session, so it cannot know who is taking a survey: the
//! taker named in the request body is ignored. A survey that requires a login is refused
//! there, and an open survey's attempt is a guest's, never attributed to a user the body
//! names (fail-hard; fresh scratch DB per test).

use tower::ServiceExt;
use uuid::Uuid;

use super::common::*;

async fn start(router: axum::Router, token: &str, body: serde_json::Value) -> (u16, serde_json::Value) {
    let mut req = axum::http::Request::post(format!("/survey/start/{token}"))
        .header("content-type", "application/json")
        .body(axum::body::Body::from(body.to_string()))
        .unwrap();
    req.extensions_mut()
        .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 9))));
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn p_public_start_ignores_the_taker_the_body_names() {
    ensure_token_secret();
    let db = TestDb::new("pub_ident").await;
    let module = std::sync::Arc::new(
        backbone_survey::SurveyModule::builder().with_database(db.pool.clone()).build().expect("module"),
    );
    let router = module.survey_public_routes();
    let forged = Uuid::new_v4();

    // A login-required survey: a made-up user id no longer passes for a login.
    let (_, gated) = seed_cert_survey(&db.pool, 50.0).await;
    let (status, body) =
        start(router.clone(), &gated, serde_json::json!({ "email": "taker@example.com", "user_id": forged })).await;
    assert_ne!(status, 201, "a body user id must not pass the login gate: {body}");
    assert!(body.to_string().contains("login_required"), "refused as login required: {body}");

    // An open survey: the attempt starts, as a guest.
    let (_, open) = seed_survey(&db.pool).await;
    let (status, body) =
        start(router, &open, serde_json::json!({ "email": "taker@example.com", "user_id": forged })).await;
    assert_eq!(status, 201, "{body}");
    let input: Uuid = body["input_id"].as_str().unwrap().parse().unwrap();
    let user: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM survey.survey_user_inputs WHERE id = $1")
        .bind(input)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(user, None, "the attempt is not attributed to the user the body names");
    db.dispose().await;
}

#[tokio::test]
async fn p_a_login_required_survey_starts_in_the_session_and_belongs_to_the_principal() {
    ensure_token_secret();
    let db = TestDb::new("sess_start").await;
    let module = std::sync::Arc::new(
        backbone_survey::SurveyModule::builder().with_database(db.pool.clone()).build().expect("module"),
    );
    let router = module.survey_session_routes();
    let (_, gated) = seed_cert_survey(&db.pool, 50.0).await;
    let taker = Uuid::new_v4();
    let post = |user: Option<Uuid>| {
        let mut req = axum::http::Request::post(format!("/survey/me/start/{gated}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(serde_json::json!({ "email": "taker@example.com" }).to_string()))
            .unwrap();
        if let Some(user) = user {
            req.extensions_mut().insert(backbone_auth::org::OrgContext {
                acting_unit_id: Uuid::new_v4(),
                entitled_units: vec![],
                legacy_company_id: None,
                user_id: user.to_string(),
            });
        }
        router.clone().oneshot(req)
    };
    let anonymous = post(None).await.unwrap();
    assert_eq!(anonymous.status().as_u16(), 401, "the session route needs a principal");

    let resp = post(Some(taker)).await.unwrap();
    assert_eq!(resp.status().as_u16(), 201);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let input: Uuid = body["input_id"].as_str().unwrap().parse().unwrap();
    let user: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM survey.survey_user_inputs WHERE id = $1")
        .bind(input)
        .fetch_one(&db.pool)
        .await
        .unwrap();
    assert_eq!(user, Some(taker), "the attempt is the principal's");
    db.dispose().await;
}
