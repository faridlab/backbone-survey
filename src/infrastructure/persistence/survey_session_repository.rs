//! `SurveySessionRepository` — the hand-written SQL behind the live-session
//! runtime (hand-authored, user-owned; see `metaphor.codegen.yaml`).
//!
//! The session runtime is survey-carried (there is no separate session
//! entity): `session_state`, `session_code`, `session_question_id`, and
//! the two clocks live on the survey row, and every mutation here is a
//! row-locked conditional write.
//!
//! What lives here, and why:
//!
//! - **`FOR UPDATE` on the survey row for the advance verb** — the
//!   read-modify-write race (two hosts advancing concurrently) is closed
//!   by the lock, not by hoping; the winner moves the cursor once, stamps
//!   the clock once, and pushes once. Probe 9 hammers exactly this.
//! - **The code mint** — 4..9 digits, uniqueness decided by the DB's
//!   partial UNIQUE on `session_code` (SV-S2); the length ladder climbs on
//!   collision and exhaustion at 9 digits is the caller's loud typed
//!   error (SV-B12 — the upstream False-emitting generator does not port).
//! - **The stored clock is written `now() + 1 s`** (the server-delay
//!   grace, kept from upstream): the pushed payload carries the PRE-write
//!   instant the attendee screens should render from.
//! - **Bulk-done at session end** is a forward-only conditional UPDATE —
//!   it passes the monotonic guard by construction and deliberately does
//!   NOT run the certification funnel (the two regimes are disjoint by
//!   config; a certification survey cannot run a live session).

use chrono::{DateTime, Utc};
use rand::Rng;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::entity::{Question, Survey};

/// Hand-written session SQL. Services orchestrate; this holds SQL.
pub struct SurveySessionRepository;

impl SurveySessionRepository {
    pub fn new() -> Self {
        Self
    }

    // ── reads ─────────────────────────────────────────────────────────────────

    /// The live survey row by id.
    pub async fn find_survey_by_id(
        conn: &mut PgConnection,
        survey_id: Uuid,
    ) -> Result<Option<Survey>, sqlx::Error> {
        sqlx::query_as::<_, Survey>(
            r#"SELECT * FROM survey.survey_surveys
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(survey_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The live survey row by its public URL key (`access_token` — public
    /// infrastructure, NOT a credential; participants are authorized by
    /// the per-attempt Tier A capability).
    pub async fn find_survey_by_access_token(
        conn: &mut PgConnection,
        access_token: &str,
    ) -> Result<Option<Survey>, sqlx::Error> {
        sqlx::query_as::<_, Survey>(
            r#"SELECT * FROM survey.survey_surveys
               WHERE access_token = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(access_token)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The live survey row currently holding `session_code` (the Tier B
    /// join key is unique among live rows).
    pub async fn find_survey_by_session_code(
        conn: &mut PgConnection,
        code: &str,
    ) -> Result<Option<Survey>, sqlx::Error> {
        sqlx::query_as::<_, Survey>(
            r#"SELECT * FROM survey.survey_surveys
               WHERE session_code = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(code)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The live survey row LOCKED FOR UPDATE — the advance/end verbs'
    /// race-closing read.
    pub async fn lock_survey_for_update(
        conn: &mut PgConnection,
        survey_id: Uuid,
    ) -> Result<Option<Survey>, sqlx::Error> {
        sqlx::query_as::<_, Survey>(
            r#"SELECT * FROM survey.survey_surveys
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL
               FOR UPDATE"#,
        )
        .bind(survey_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The survey's live non-page questions in sequence order — the cursor
    /// walk and the session page payloads read this.
    pub async fn questions_in_sequence(
        conn: &mut PgConnection,
        survey_id: Uuid,
    ) -> Result<Vec<Question>, sqlx::Error> {
        sqlx::query_as::<_, Question>(
            r#"SELECT * FROM survey.survey_questions
               WHERE survey_id = $1 AND is_page = FALSE
                 AND (metadata->>'deleted_at') IS NULL
               ORDER BY sequence ASC, id ASC"#,
        )
        .bind(survey_id)
        .fetch_all(&mut *conn)
        .await
    }

    // ── the session verbs ─────────────────────────────────────────────────────

    /// Arm the session: force `page_per_question`, stamp the code, set
    /// `ready`. Conditional on no session running (`session_state IS
    /// NULL`) so a double arm is a zero-row refusal, not a re-arm.
    /// The arm instant is recorded in the row's metadata
    /// (`session_armed_at`, an operational timestamp beside the audit
    /// keys) — the Tier B hard-TTL clock.
    pub async fn arm_session(
        conn: &mut PgConnection,
        survey_id: Uuid,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<Survey>, sqlx::Error> {
        sqlx::query_as::<_, Survey>(
            r#"UPDATE survey.survey_surveys
               SET questions_layout = 'page_per_question',
                   session_state = 'ready',
                   session_code = $2,
                   session_question_id = NULL,
                   session_question_start_time = NULL,
                   metadata = jsonb_set(metadata, '{session_armed_at}', to_jsonb($3::timestamptz))
               WHERE id = $1
                 AND session_state IS NULL
                 AND (metadata->>'deleted_at') IS NULL
               RETURNING *"#,
        )
        .bind(survey_id)
        .bind(code)
        .bind(now)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The lazy open on first advance: `ready -> in_progress`, stamping
    /// the leaderboard window's lower bound.
    pub async fn open_session(
        conn: &mut PgConnection,
        survey_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE survey.survey_surveys
               SET session_state = 'in_progress',
                   session_start_time = COALESCE(session_start_time, $2)
               WHERE id = $1 AND session_state = 'ready'"#,
        )
        .bind(survey_id)
        .bind(now)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Move the cursor + stamp the stored clock `now() + 1 s` (the
    /// server-delay grace; the pushed payload carries the PRE-write
    /// value). The caller holds the row lock and supplies the target
    /// question; this write is the single cursor mutation.
    pub async fn advance_cursor(
        conn: &mut PgConnection,
        survey_id: Uuid,
        question_id: Uuid,
        clock: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE survey.survey_surveys
               SET session_question_id = $2, session_question_start_time = $3
               WHERE id = $1 AND session_state IN ('ready', 'in_progress')"#,
        )
        .bind(survey_id)
        .bind(question_id)
        .bind(clock)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// End the session: NULL the state (the code's validity is
    /// state-based — a NULL `session_state` invalidates it), leave the
    /// code + window stamps as history.
    pub async fn end_session(
        conn: &mut PgConnection,
        survey_id: Uuid,
    ) -> Result<Option<Survey>, sqlx::Error> {
        sqlx::query_as::<_, Survey>(
            r#"UPDATE survey.survey_surveys
               SET session_state = NULL, session_question_start_time = NULL
               WHERE id = $1 AND session_state IS NOT NULL
               RETURNING *"#,
        )
        .bind(survey_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The session-end bulk-done: every live session attendee still
    /// `in_progress` moves forward to `done` with `end_datetime` stamped.
    /// Forward-only by construction (the monotonic trigger passes it);
    /// `new` attendees are left alone (never started, never finished).
    pub async fn bulk_done_attendees(
        conn: &mut PgConnection,
        survey_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE survey.survey_user_inputs
               SET state = 'done', end_datetime = $2
               WHERE survey_id = $1
                 AND is_session_answer
                 AND state = 'in_progress'
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(survey_id)
        .bind(now)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected())
    }

    // ── the Tier B code mint ──────────────────────────────────────────────────

    /// A random digit string of `digits` length (leading zeros allowed —
    /// the human types what the screen shows).
    pub fn candidate_code(digits: usize) -> String {
        let mut rng = rand::thread_rng();
        (0..digits).map(|_| char::from(b'0' + rng.gen_range(0..10))).collect()
    }

    /// Claim `code` for `survey_id` atomically. The partial UNIQUE on
    /// `session_code` among live rows decides: a collision surfaces as
    /// the unique-violation error code (23505) and the caller climbs the
    /// length ladder. Ok(true) = claimed; Ok(false) = the survey is not
    /// armable (already running / gone).
    pub async fn claim_session_code(
        conn: &mut PgConnection,
        survey_id: Uuid,
        code: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE survey.survey_surveys
               SET session_code = $2
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(survey_id)
        .bind(code)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }
}

impl Default for SurveySessionRepository {
    fn default() -> Self {
        Self::new()
    }
}
