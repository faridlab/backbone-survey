//! Survey public + guarded route composers (hand-written; user-owned;
//! see `metaphor.codegen.yaml`).
//!
//! Two mounts, one principle: **the capability link is the auth** (the
//! rating precedent, ADR-0019 class 2 — bare capability mounts,
//! throttled, shared refusals).
//!
//! - **Public** — the taker-facing tree (`/survey/...`), BARE mount
//!   throttled 120/min per client. The `{id}.{nonce}.{exp}.{mac}` token
//!   in the path is the only credential; the taker is anonymous by
//!   design; every refusal after MAC verification answers the shared
//!   `survey_attempt_not_submittable` shape — unknown, forged,
//!   malformed, and finished are indistinguishable (no oracle). The
//!   session-code routes add the Tier B counters (lockout + spacing).
//!   All mutators are POST; the GETs are side-effect-free reads.
//! - **Guarded** — the officer verbs (invite batch, test-start, session
//!   arm/advance/end, rotate, resend, regrade, statistics, leaderboard)
//!   for the host's authenticated tree, composed over the hand write
//!   services. No generic mutation reaches the attempt tables: rows are
//!   ONLY written through the validated verbs (the module's readonly
//!   base stays available separately).
//!
//! Route map (relative to each mount point):
//!
//! | Mount | Method | Path | Handler |
//! |---|---|---|---|
//! | public | GET | /survey/s/:code | session preview (no writes) |
//! | public | POST | /survey/s/:code/join | Tier B verify + attempt mint |
//! | public | POST | /survey/start/:survey_token | public entry mint |
//! | public | POST | /survey/attempt/:token/begin | open the attempt |
//! | public | POST | /survey/attempt/:token/submit | the intake |
//! | public | GET | /survey/attempt/:token/next | live-session cursor read |
//! | public | GET | /survey/attempt/:token/certification | scoring evidence |
//! | guarded | POST | /surveys/:id/invite | invitation batch |
//! | guarded | POST | /surveys/:id/test-start | test-mode entry |
//! | guarded | POST | /surveys/:id/session/start | arm the session |
//! | guarded | POST | /surveys/:id/session/next | advance the cursor |
//! | guarded | POST | /surveys/:id/session/end | bulk-done + close |
//! | guarded | GET | /surveys/:id/session/leaderboard | top-15 standings |
//! | guarded | GET | /surveys/:id/statistics | per-section funnel counts |
//! | guarded | POST | /user-inputs/:id/rotate-token | Tier A rotation |
//! | guarded | POST | /user-inputs/:id/resend | pool re-entry |
//! | guarded | POST | /surveys/:id/questions/:qid/regrade | sanctioned regrade |

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::application::service::attempt_service::{
    AttemptService, InviteRecipient,
};
use crate::application::service::intake_service::{AnswerDraft, IntakeService};
use crate::application::service::survey_write_service::{
    SurveyWriteError, SurveyWriteService,
};
use crate::domain::entity::SurveyInviteExistingMode;
use crate::SurveyModule;

/// The shared state of both composers: the whole module behind an Arc.
pub type ApiState = Arc<SurveyModule>;

// ─── the shared refusal mapping ───────────────────────────────────────────────

fn survey_err(e: SurveyWriteError) -> Response {
    let status =
        StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = match &e {
        SurveyWriteError::Db(_) | SurveyWriteError::Internal(_) => {
            json!({ "error": "internal error", "code": "internal_error" })
        }
        // The shared-refusal family: one body, one code, no oracle.
        SurveyWriteError::AttemptNotSubmittable | SurveyWriteError::AttemptExpired => {
            json!({ "error": "not submittable", "code": "survey_attempt_not_submittable" })
        }
        SurveyWriteError::SecretNotConfigured => {
            json!({ "error": "internal error", "code": "internal_error" })
        }
        other => json!({ "error": other.to_string(), "code": other.code() }),
    };
    (status, Json(body)).into_response()
}

/// The visitor IP as the host saw it (first `X-Forwarded-For` hop) — the
/// Tier B per-IP lockout grain.
fn visitor_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

// ─── the public tree ──────────────────────────────────────────────────────────

/// GET /survey/s/:code — the session preview: metadata a join screen
/// renders (title, live state). NO writes, no code verification
/// bookkeeping — a preview must not consume Tier B counters.
async fn session_preview(State(app): State<ApiState>, Path(code): Path<String>) -> Response {
    let writes = app.survey_write_service();
    match writes.survey_by_session_code(&code).await {
        Ok(Some(view)) if view["live"] == json!(true) => (StatusCode::OK, Json(view)).into_response(),
        // Unknown AND dead (ended / expired) codes answer the same 404 —
        // a known-but-dead code is not distinguishable from an unknown one.
        Ok(Some(_)) | Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "unknown session code", "code": "survey_session_code_not_valid" })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

#[derive(Deserialize)]
pub struct JoinBody {
    /// The attendee's self-declared identity (the Tier B per-identity
    /// lockout grain).
    pub identity: String,
    /// The host-minted realtime handle stamped on the attempt (the
    /// resolver's key).
    pub wire_identity_key: String,
    pub nickname: Option<String>,
}

/// POST /survey/s/:code/join — Tier B verify (counters, lockout,
/// spacing) → attempt pre-creation + Tier A mint + guest-handle stamp.
/// The row exists BEFORE any submit — THE anti-cheat.
async fn join_session(
    State(app): State<ApiState>,
    headers: HeaderMap,
    Path(code): Path<String>,
    Json(body): Json<JoinBody>,
) -> Response {
    let attempts = app.attempt_service();
    let ip = visitor_ip(&headers);
    match attempts
        .join_by_code(
            &code,
            &body.identity,
            &ip,
            &body.wire_identity_key,
            body.nickname.as_deref(),
        )
        .await
    {
        Ok(ticket) => (
            StatusCode::CREATED,
            Json(json!({
                "input_id": ticket.input.id,
                "link": ticket.link,
                "state": ticket.input.state.to_string(),
            })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

#[derive(Deserialize)]
pub struct PublicStartBody {
    pub email: Option<String>,
    pub nickname: Option<String>,
    pub user_id: Option<Uuid>,
}

/// POST /survey/start/:survey_token — public (non-session) entry: the
/// intake gates then the attempt pre-creation + Tier A mint. The
/// survey's URL key is NOT an authorizer — the minted capability is.
async fn public_start(
    State(app): State<ApiState>,
    Path(survey_token): Path<String>,
    Json(body): Json<PublicStartBody>,
) -> Response {
    match app
        .attempt_service()
        .public_start(
            &survey_token,
            body.email.as_deref(),
            body.nickname.as_deref(),
            body.user_id,
        )
        .await
    {
        Ok(ticket) => (
            StatusCode::CREATED,
            Json(json!({ "input_id": ticket.input.id, "link": ticket.link })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

/// POST /survey/attempt/:token/begin — open the attempt
/// (`new -> in_progress`), stamping the start and the deadline.
async fn begin_attempt(State(app): State<ApiState>, Path(token): Path<String>) -> Response {
    match app.intake_service().begin(&token).await {
        Ok(input) => (
            StatusCode::OK,
            Json(json!({
                "input_id": input.id,
                "state": input.state.to_string(),
                "deadline": input.deadline,
            })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

/// The wire shape of one answer: `skipped`, or exactly one typed value
/// field. `question_id` is the target.
#[derive(Deserialize)]
pub struct SubmitBody {
    pub question_id: Uuid,
    #[serde(default)]
    pub skipped: bool,
    pub value_char: Option<String>,
    pub value_text: Option<String>,
    pub value_number: Option<f64>,
    pub value_scale: Option<i32>,
    pub value_date: Option<chrono::NaiveDate>,
    pub value_datetime: Option<chrono::DateTime<chrono::Utc>>,
    pub value_choice: Option<Vec<Uuid>>,
    /// Flat matrix encoding: `row_id:col_id` pairs.
    pub value_matrix: Option<Vec<(Uuid, Uuid)>>,
    pub value_comment: Option<String>,
}

impl SubmitBody {
    fn draft(&self) -> Result<AnswerDraft, SurveyWriteError> {
        if self.skipped {
            return Ok(AnswerDraft::Skipped);
        }
        let provided: usize = [
            self.value_char.is_some(),
            self.value_text.is_some(),
            self.value_number.is_some(),
            self.value_scale.is_some(),
            self.value_date.is_some(),
            self.value_datetime.is_some(),
            self.value_choice.is_some(),
            self.value_matrix.is_some(),
            self.value_comment.is_some(),
        ]
        .into_iter()
        .filter(|b| *b)
        .count();
        if provided != 1 {
            return Err(SurveyWriteError::ValidationFailed {
                question_id: self.question_id,
                reason: "exactly one typed value (or skipped) must be provided".into(),
            });
        }
        if let Some(v) = &self.value_char {
            return Ok(AnswerDraft::Char(v.clone()));
        }
        if let Some(v) = &self.value_text {
            return Ok(AnswerDraft::Text(v.clone()));
        }
        if let Some(v) = self.value_number {
            return Ok(AnswerDraft::Number(v));
        }
        if let Some(v) = self.value_scale {
            return Ok(AnswerDraft::Scale(v));
        }
        if let Some(v) = self.value_date {
            return Ok(AnswerDraft::Date(v));
        }
        if let Some(v) = self.value_datetime {
            return Ok(AnswerDraft::Datetime(v));
        }
        if let Some(v) = &self.value_choice {
            return Ok(AnswerDraft::Choice(v.clone()));
        }
        if let Some(v) = &self.value_matrix {
            return Ok(AnswerDraft::Matrix(v.clone()));
        }
        if let Some(v) = &self.value_comment {
            return Ok(AnswerDraft::Comment(v.clone()));
        }
        Err(SurveyWriteError::ValidationFailed {
            question_id: self.question_id,
            reason: "no value provided".into(),
        })
    }
}

/// POST /survey/attempt/:token/submit — THE intake: gates, validation,
/// write-once scoring, conditional clearing, live recompute.
async fn submit_answer(
    State(app): State<ApiState>,
    Path(token): Path<String>,
    Json(body): Json<SubmitBody>,
) -> Response {
    // A malformed body answers 422 without touching the capability —
    // shape errors reveal nothing about the token.
    let draft = match body.draft() {
        Ok(d) => d,
        Err(e) => return survey_err(e),
    };
    match app.intake_service().submit_answer(&token, body.question_id, draft).await {
        Ok(outcome) => (
            StatusCode::OK,
            Json(json!({
                "scoring_percentage": outcome.totals.percentage,
                "scoring_total": outcome.totals.total,
                "scoring_success": outcome.totals.success,
            })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

/// GET /survey/attempt/:token/next — the live-session attendee cursor
/// read (safe: no writes; polling).
async fn next_question(State(app): State<ApiState>, Path(token): Path<String>) -> Response {
    match app.attempt_service().verify_capability(&token).await {
        Ok(input) => match app.session_read_service().snapshot(input.survey_id).await {
            Ok(snapshot) => match snapshot.next_question_payload() {
                Some(p) => (
                    StatusCode::OK,
                    Json(json!({
                        "question_start_ms": p.question_start_ms,
                        "question_id": p.question_id,
                        "sequence": p.sequence,
                    })),
                )
                    .into_response(),
                None => (
                    StatusCode::OK,
                    Json(json!({ "question_id": null, "state": "awaiting" })),
                )
                    .into_response(),
            },
            Err(e) => survey_err(e),
        },
        Err(e) => survey_err(e),
    }
}

/// GET /survey/attempt/:token/certification — the scoring evidence the
/// webapp certification page renders. Requires a `scoring_success`
/// input; the reveal obeys `scoring_type` (without-answers shows the
/// verdicts but never the stored correct answers).
async fn certification_evidence(State(app): State<ApiState>, Path(token): Path<String>) -> Response {
    let module = app.clone();
    match module.attempt_service().verify_capability(&token).await {
        Ok(input) => match module.certification_evidence(input.id).await {
            Ok(view) => (StatusCode::OK, Json(view)).into_response(),
            Err(e) => survey_err(e),
        },
        Err(e) => survey_err(e),
    }
}

/// The PUBLIC survey group — BARE mount (the capability token is the
/// auth), throttled 120/min per client.
pub fn public_composer() -> Router<ApiState> {
    use axum::middleware as axum_mw;

    Router::<ApiState>::new()
        .route("/survey/s/:code", get(session_preview))
        .route("/survey/s/:code/join", post(join_session))
        .route("/survey/start/:survey_token", post(public_start))
        .route("/survey/attempt/:token/begin", post(begin_attempt))
        .route("/survey/attempt/:token/submit", post(submit_answer))
        .route("/survey/attempt/:token/next", get(next_question))
        .route("/survey/attempt/:token/certification", get(certification_evidence))
        .route_layer(axum_mw::from_fn_with_state(
            backbone_rate_limit::middleware(120, 60),
            backbone_rate_limit::rate_limit_middleware,
        ))
}

// ─── the guarded officer verbs ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct InviteBody {
    pub recipients: Vec<InviteRecipientBody>,
    #[serde(default)]
    pub free_form_emails: String,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub existing_mode: Option<String>,
}

#[derive(Deserialize)]
pub struct InviteRecipientBody {
    pub partner_id: Option<Uuid>,
    pub email: Option<String>,
    pub nickname: Option<String>,
    pub user_id: Option<Uuid>,
}

async fn invite(State(app): State<ApiState>, Path(survey_id): Path<Uuid>, Json(body): Json<InviteBody>) -> Response {
    // new (default) vs resend — anything else refuses 422.
    let mode = match body.existing_mode.as_deref() {
        None | Some("new") => SurveyInviteExistingMode::New,
        Some("resend") => SurveyInviteExistingMode::Resend,
        Some(other) => {
            return survey_err(SurveyWriteError::ValidationFailed {
                question_id: survey_id, // not a question — the field is the nearest typed refusal
                reason: format!("unknown existing_mode {other:?}"),
            })
        }
    };
    let recipients: Vec<InviteRecipient> = body
        .recipients
        .into_iter()
        .map(|r| InviteRecipient {
            partner_id: r.partner_id,
            email: r.email,
            nickname: r.nickname,
            user_id: r.user_id,
        })
        .collect();
    match app
        .attempt_service()
        .invite(survey_id, recipients, &body.free_form_emails, body.deadline, mode)
        .await
    {
        Ok(results) => {
            let items: Vec<serde_json::Value> = results
                .into_iter()
                .map(|r| match r {
                    Ok(t) => json!({ "ok": true, "input_id": t.input.id, "link": t.link }),
                    Err(e) => json!({ "ok": false, "error": e.to_string(), "code": e.code() }),
                })
                .collect();
            (StatusCode::CREATED, Json(json!({ "results": items }))).into_response()
        }
        Err(e) => survey_err(e),
    }
}

async fn test_start(State(app): State<ApiState>, Path(survey_id): Path<Uuid>) -> Response {
    match app.attempt_service().test_start(survey_id).await {
        Ok(t) => (
            StatusCode::CREATED,
            Json(json!({ "input_id": t.input.id, "link": t.link })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

async fn session_start(State(app): State<ApiState>, Path(survey_id): Path<Uuid>) -> Response {
    match app.survey_write_service().arm_session(survey_id).await {
        Ok(survey) => (
            StatusCode::OK,
            Json(json!({
                "session_code": survey.session_code,
                "session_state": survey.session_state.map(|s| s.to_string()),
            })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

async fn session_next(State(app): State<ApiState>, Path(survey_id): Path<Uuid>) -> Response {
    match app.survey_write_service().advance_session(survey_id).await {
        Ok(survey) => (
            StatusCode::OK,
            Json(json!({
                "session_question_id": survey.session_question_id,
                "session_question_start_time": survey.session_question_start_time,
            })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

async fn session_end(State(app): State<ApiState>, Path(survey_id): Path<Uuid>) -> Response {
    match app.survey_write_service().end_session(survey_id).await {
        Ok(done) => (
            StatusCode::OK,
            Json(json!({ "attendees_done": done })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

async fn leaderboard(State(app): State<ApiState>, Path(survey_id): Path<Uuid>) -> Response {
    match app.leaderboard_view(survey_id).await {
        Ok(rows) => (StatusCode::OK, Json(json!({ "leaderboard": rows }))).into_response(),
        Err(e) => survey_err(e),
    }
}

async fn statistics(State(app): State<ApiState>, Path(survey_id): Path<Uuid>) -> Response {
    match app.statistics_view(survey_id).await {
        Ok(sections) => (StatusCode::OK, Json(json!({ "sections": sections }))).into_response(),
        Err(e) => survey_err(e),
    }
}

#[derive(Deserialize)]
pub struct TtlBody {
    pub ttl_days: Option<i64>,
}

async fn rotate_token(
    State(app): State<ApiState>,
    Path(input_id): Path<Uuid>,
    body: Option<Json<TtlBody>>,
) -> Response {
    let ttl = body.and_then(|Json(b)| b.ttl_days);
    match app.attempt_service().rotate_token(input_id, ttl).await {
        Ok(t) => (
            StatusCode::OK,
            Json(json!({ "input_id": t.input.id, "link": t.link })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

async fn resend(State(app): State<ApiState>, Path(input_id): Path<Uuid>) -> Response {
    match app.attempt_service().resend(input_id).await {
        Ok(t) => (
            StatusCode::OK,
            Json(json!({ "input_id": t.input.id, "link": t.link })),
        )
            .into_response(),
        Err(e) => survey_err(e),
    }
}

async fn regrade(
    State(app): State<ApiState>,
    Path((survey_id, question_id)): Path<(Uuid, Uuid)>,
) -> Response {
    match app.scoring_service().regrade_question(survey_id, question_id).await {
        Ok(touched) => {
            let items: Vec<serde_json::Value> = touched
                .into_iter()
                .map(|(line, old, new)| json!({ "line_id": line, "old": old, "new": new }))
                .collect();
            (StatusCode::OK, Json(json!({ "touched": items }))).into_response()
        }
        Err(e) => survey_err(e),
    }
}

/// The GUARDED survey group — the officer verbs over the hand write
/// services, for the host's authenticated tree (the host nests this at
/// its `/api/v1/survey` behind identity + its module-write gate).
pub fn guarded_composer() -> Router<ApiState> {
    Router::<ApiState>::new()
        .route("/surveys/:id/invite", post(invite))
        .route("/surveys/:id/test-start", post(test_start))
        .route("/surveys/:id/session/start", post(session_start))
        .route("/surveys/:id/session/next", post(session_next))
        .route("/surveys/:id/session/end", post(session_end))
        .route("/surveys/:id/session/leaderboard", get(leaderboard))
        .route("/surveys/:id/statistics", get(statistics))
        .route("/user-inputs/:id/rotate-token", post(rotate_token))
        .route("/user-inputs/:id/resend", post(resend))
        .route("/surveys/:id/questions/:qid/regrade", post(regrade))
}
