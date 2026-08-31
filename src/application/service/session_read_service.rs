//! `SessionReadService` — the live-session read surface: the realtime
//! record-channel grammar, the push payload shape, and the data the
//! host's ThreadAccessResolver needs (hand-written, user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! Composition contract (spec §8): this module owns NO SSE route —
//! attendees stream through the mail module's existing
//! `GET /mail/realtime/stream` and see only records they can resolve.
//! What survey owes that stream:
//!
//! - the channel grammar `survey.survey_{survey_id}` (record-shaped;
//!   [`record_channel`] renders it, [`parse_record_channel`] is the
//!   strict parser the resolver + probes share);
//! - the `next_question` payload `{question_start_ms, question_id,
//!   sequence}` where `question_start_ms` is the PRE-write clock — the
//!   stored `session_question_start_time` is `now + 1 s` (the
//!   server-delay grace), so the read side subtracts the grace back out;
//! - the resolver query: an identity may read a survey's channel iff a
//!   live, non-terminal session-answer row carries its
//!   `wire_identity_key` (the opaque host-minted handle stamped at
//!   join/begin). Foreign handles and finished attempts deny.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::service::survey_write_service::SurveyWriteError;
use crate::domain::entity::Survey;

/// The realtime channel prefix (record-shaped: `{kind}.{record}`).
pub const RECORD_CHANNEL_PREFIX: &str = "survey.survey_";
/// The server-delay grace the advance verb writes INTO the stored clock
/// (`now + 1 s`); the push payload carries the pre-write value.
pub const STORED_CLOCK_GRACE_MS: i64 = 1_000;

/// Render the realtime record channel of a survey.
pub fn record_channel(survey_id: Uuid) -> String {
    format!("{RECORD_CHANNEL_PREFIX}{survey_id}")
}

/// The strict inverse: `survey.survey_{uuid}` → the uuid. Anything else
/// (wrong kind, wrong prefix, malformed uuid, trailing junk) is `None`
/// — the resolver must never half-match a channel name.
pub fn parse_record_channel(channel: &str) -> Option<Uuid> {
    let rest = channel.strip_prefix(RECORD_CHANNEL_PREFIX)?;
    if rest.contains(RECORD_CHANNEL_PREFIX) || rest.contains('.') {
        return None;
    }
    Uuid::parse_str(rest).ok()
}

/// The push payload of one cursor move (the staged `next_question`
/// fact's body).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextQuestionPayload {
    /// The PRE-write clock in epoch millis (stored clock minus the
    /// server-delay grace) — the value attendee screens render from.
    pub question_start_ms: i64,
    pub question_id: Uuid,
    pub sequence: i32,
}

/// The session's read-side snapshot (the leaderboard window's bounds).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSnapshot {
    pub survey_id: Uuid,
    pub session_state: Option<crate::domain::entity::SurveySessionState>,
    pub session_code: Option<String>,
    pub session_start_time: Option<DateTime<Utc>>,
    pub current_question_id: Option<Uuid>,
    pub current_question_sequence: Option<i32>,
    pub question_clock: Option<DateTime<Utc>>,
}

impl SessionSnapshot {
    /// The `next_question` payload for the CURRENT cursor. `None` when
    /// no question is live. `question_start_ms` is the pre-write clock.
    pub fn next_question_payload(&self) -> Option<NextQuestionPayload> {
        let question_id = self.current_question_id?;
        let clock = self.question_clock?;
        Some(NextQuestionPayload {
            question_start_ms: clock.timestamp_millis() - STORED_CLOCK_GRACE_MS,
            question_id,
            sequence: self.current_question_sequence.unwrap_or(0),
        })
    }
}

/// The live-session read service.
pub struct SessionReadService {
    pool: PgPool,
}

impl SessionReadService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// The session snapshot of a survey (any state; `session_state`
    /// `None` = never armed or ended).
    pub async fn snapshot(&self, survey_id: Uuid) -> Result<SessionSnapshot, SurveyWriteError> {
        let row = sqlx::query_as::<_, Survey>(
            r#"SELECT * FROM survey.survey_surveys
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(survey_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(SurveyWriteError::SurveyNotFound(survey_id))?;
        let current_sequence: Option<i32> = match row.session_question_id {
            Some(qid) => {
                sqlx::query_scalar::<_, i32>(
                    r#"SELECT sequence FROM survey.survey_questions WHERE id = $1"#,
                )
                .bind(qid)
                .fetch_optional(&self.pool)
                .await?
            }
            None => None,
        };
        Ok(SessionSnapshot {
            survey_id: row.id,
            session_state: row.session_state,
            session_code: row.session_code,
            session_start_time: row.session_start_time,
            current_question_id: row.session_question_id,
            current_question_sequence: current_sequence,
            question_clock: row.session_question_start_time,
        })
    }

    /// The resolver's allow-rule: a wire identity may read a survey's
    /// channel iff it carries a live, non-terminal session-answer row
    /// for that survey. Foreign handles, anonymous handles, and done
    /// attempts all deny.
    pub async fn resolver_allows(
        &self,
        survey_id: Uuid,
        wire_identity_key: &str,
    ) -> Result<bool, SurveyWriteError> {
        let live = sqlx::query_scalar::<_, i64>(
            r#"SELECT count(*) FROM survey.survey_user_inputs
               WHERE survey_id = $1
                 AND wire_identity_key = $2
                 AND is_session_answer
                 AND state <> 'done'
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(survey_id)
        .bind(wire_identity_key)
        .fetch_one(&self.pool)
        .await?;
        Ok(live > 0)
    }

    /// The session standings (the leaderboard window): done session
    /// answers ranked by `scoring_total` DESC inside the session's
    /// start-time window, newest last. Speed-rating sessions rank by
    /// raw total (negatives included) — the fold's clamped percentage
    /// is a display concern.
    pub async fn leaderboard(
        &self,
        survey_id: Uuid,
        limit: i64,
    ) -> Result<Vec<(Uuid, Option<String>, f64, bool)>, SurveyWriteError> {
        let rows = sqlx::query_as::<_, (Uuid, Option<String>, f64, bool)>(
            r#"SELECT id, nickname, scoring_total, scoring_success
               FROM survey.survey_user_inputs
               WHERE survey_id = $1
                 AND is_session_answer
                 AND state = 'done'
                 AND test_entry IS NOT TRUE
                 AND (metadata->>'deleted_at') IS NULL
               ORDER BY scoring_total DESC, end_datetime ASC
               LIMIT $2"#,
        )
        .bind(survey_id)
        .bind(limit.clamp(1, 1000))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
