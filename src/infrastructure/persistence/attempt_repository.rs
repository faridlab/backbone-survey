//! `AttemptRepository` — the hand-written SQL behind attempt entry, the
//! Tier A capability columns, and the attempt-pool self-join
//! (hand-authored, user-owned; see `metaphor.codegen.yaml`).
//!
//! Module rule: services orchestrate, repositories hold SQL. Runtime
//! queries — no `.sqlx` macros, so no compile-time cache is needed. Every
//! method takes a connection inside the caller's transaction; every read
//! filters on `metadata->>'deleted_at' IS NULL` (the soft-delete
//! convention). The module is unfenced (posture C2): there is no tenant
//! column and the database boundary is the isolation.
//!
//! What lives here, and why:
//!
//! - **Tier A selector + expiry** — `token_nonce` is the 128-bit hex
//!   selector with a partial UNIQUE among live rows; the MAC lives only in
//!   the link the service hands out, so a leaked row alone is inert. The
//!   nonce is NOT unique across soft-deleted rows (rotation history may
//!   reuse the namespace safely).
//! - **Conditional state transitions** — every state write is a
//!   conditional UPDATE keyed on the EXPECTED prior state, returning the
//!   affected row; zero rows is the typed `StateConflict` (the service's
//!   first line). The DB-level monotonic trigger is the backstop for every
//!   other write path (raw SQL included).
//! - **The attempt pool self-join** (the upstream `_count_attempt` port,
//!   verbatim semantics) — same survey, `state = done`, not a test entry,
//!   live row, shared `invite_token` (or both NULL), AND same `partner_id`
//!   OR same `email`. Parameterized with nullable binds so the SQL shape
//!   is fixed and plan-cached.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::entity::{SurveyInputState, UserInput};

/// Hand-written attempt SQL. Services orchestrate; this holds SQL.
pub struct AttemptRepository;

impl AttemptRepository {
    pub fn new() -> Self {
        Self
    }

    // ── reads ─────────────────────────────────────────────────────────────────

    /// The live input row by id (soft-deleted rows are invisible).
    pub async fn find_live_by_id(
        conn: &mut PgConnection,
        input_id: Uuid,
    ) -> Result<Option<UserInput>, sqlx::Error> {
        sqlx::query_as::<_, UserInput>(
            r#"SELECT * FROM survey.survey_user_inputs
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(input_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The live input row by token nonce (the Tier A selector).
    pub async fn find_live_by_nonce(
        conn: &mut PgConnection,
        nonce: &str,
    ) -> Result<Option<UserInput>, sqlx::Error> {
        sqlx::query_as::<_, UserInput>(
            r#"SELECT * FROM survey.survey_user_inputs
               WHERE token_nonce = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(nonce)
        .fetch_optional(&mut *conn)
        .await
    }

    // ── entry ─────────────────────────────────────────────────────────────────

    /// Insert a fresh attempt row. The state is the caller's call: the
    /// normal entry paths mint `new`; the session-attendee direct-entry
    /// ruling allows `in_progress` (the monotonic trigger only forbids
    /// BACKWARD edges, and `new -> in_progress` on insert-time is a
    /// forward start).
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_input(
        conn: &mut PgConnection,
        id: Uuid,
        survey_id: Uuid,
        token_nonce: &str,
        token_expires_at: DateTime<Utc>,
        invite_token: Option<&str>,
        partner_id: Option<Uuid>,
        email: Option<&str>,
        nickname: Option<&str>,
        user_id: Option<Uuid>,
        wire_identity_key: Option<&str>,
        test_entry: bool,
        state: SurveyInputState,
        deadline: Option<DateTime<Utc>>,
        is_session_answer: bool,
    ) -> Result<UserInput, sqlx::Error> {
        sqlx::query_as::<_, UserInput>(
            r#"INSERT INTO survey.survey_user_inputs
                   (id, survey_id, token_nonce, token_expires_at, invite_token,
                    partner_id, email, nickname, user_id, wire_identity_key,
                    test_entry, state, deadline, is_session_answer)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
               RETURNING *"#,
        )
        .bind(id)
        .bind(survey_id)
        .bind(token_nonce)
        .bind(token_expires_at)
        .bind(invite_token)
        .bind(partner_id)
        .bind(email)
        .bind(nickname)
        .bind(user_id)
        .bind(wire_identity_key)
        .bind(test_entry)
        .bind(state)
        .bind(deadline)
        .bind(is_session_answer)
        .fetch_one(&mut *conn)
        .await
    }

    // ── Tier A rotation ───────────────────────────────────────────────────────

    /// Atomically replace the nonce + expiry. Conditional on the row being
    /// live and not terminal (`done` attempts keep their audit trail; the
    /// link of a finished attempt must die, not rotate). Zero rows → the
    /// typed refusal at the service.
    pub async fn rotate_nonce(
        conn: &mut PgConnection,
        input_id: Uuid,
        new_nonce: &str,
        new_expires_at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE survey.survey_user_inputs
               SET token_nonce = $2, token_expires_at = $3
               WHERE id = $1
                 AND state <> 'done'
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(input_id)
        .bind(new_nonce)
        .bind(new_expires_at)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    // ── conditional state transitions (the monotonic-safe first line) ─────────

    /// `new -> in_progress` with `start_datetime` stamped on the same write.
    /// Zero rows → `StateConflict` (already begun or already done).
    pub async fn transition_begin(
        conn: &mut PgConnection,
        input_id: Uuid,
        now: DateTime<Utc>,
        deadline: Option<DateTime<Utc>>,
    ) -> Result<Option<UserInput>, sqlx::Error> {
        sqlx::query_as::<_, UserInput>(
            r#"UPDATE survey.survey_user_inputs
               SET state = 'in_progress', start_datetime = $2, deadline = COALESCE(deadline, $3)
               WHERE id = $1 AND state = 'new' AND (metadata->>'deleted_at') IS NULL
               RETURNING *"#,
        )
        .bind(input_id)
        .bind(now)
        .bind(deadline)
        .fetch_optional(&mut *conn)
        .await
    }

    /// `in_progress -> done` with `end_datetime` stamped on the same write.
    /// A double `mark_done` is a zero-row no-op refusal, never a timestamp
    /// rewrite. Raw-SQL backward edges are refused by the DB trigger.
    pub async fn transition_done(
        conn: &mut PgConnection,
        input_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE survey.survey_user_inputs
               SET state = 'done', end_datetime = $2
               WHERE id = $1 AND state = 'in_progress' AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(input_id)
        .bind(now)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    // ── side-writes ───────────────────────────────────────────────────────────

    /// Stamp the opaque host-minted realtime handle (join/begin; the
    /// module never interprets it — the host resolver resolves it).
    pub async fn set_wire_identity(
        conn: &mut PgConnection,
        input_id: Uuid,
        wire_identity_key: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE survey.survey_user_inputs
               SET wire_identity_key = $2
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(input_id)
        .bind(wire_identity_key)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// The `save_as_email` / `save_as_nickname` side-writes land on the
    /// INPUT row (upstream `_save_lines` semantics).
    pub async fn set_identity_side_writes(
        conn: &mut PgConnection,
        input_id: Uuid,
        email: Option<&str>,
        nickname: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE survey.survey_user_inputs
               SET email = COALESCE($2, email),
                   nickname = COALESCE($3, nickname)
               WHERE id = $1"#,
        )
        .bind(input_id)
        .bind(email)
        .bind(nickname)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Persist the recomputed scoring triple (evaluated live, mid-attempt
    /// included — `scoring_success` flips the moment the threshold clears).
    pub async fn set_scoring(
        conn: &mut PgConnection,
        input_id: Uuid,
        percentage: f64,
        total: f64,
        success: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE survey.survey_user_inputs
               SET scoring_percentage = $2, scoring_total = $3, scoring_success = $4
               WHERE id = $1"#,
        )
        .bind(input_id)
        .bind(percentage)
        .bind(total)
        .bind(success)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    // ── the attempt pool (upstream `_count_attempt`, verbatim semantics) ───────

    /// Count the DONE attempts in this identity's pool: same survey, done,
    /// not a test entry, live row, shared invite_token (or both NULL), and
    /// same partner OR same email. `self` (by id) is excluded so the
    /// first-completion gate can ask "any PRIOR success in the pool".
    #[allow(clippy::too_many_arguments)]
    pub async fn pool_count(
        conn: &mut PgConnection,
        survey_id: Uuid,
        invite_token: Option<&str>,
        partner_id: Option<Uuid>,
        email: Option<&str>,
        exclude_input_id: Option<Uuid>,
        prior_success_only: bool,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            r#"SELECT count(*) FROM survey.survey_user_inputs pool
               WHERE pool.survey_id = $1
                 AND pool.state = 'done'
                 AND pool.test_entry IS NOT TRUE
                 AND (pool.metadata->>'deleted_at') IS NULL
                 AND ($2::text IS NULL AND pool.invite_token IS NULL OR pool.invite_token = $2::text)
                 AND (pool.partner_id IS NOT NULL AND pool.partner_id = $3
                      OR pool.email IS NOT NULL AND pool.email = $4)
                 AND ($5::uuid IS NULL OR pool.id <> $5)
                 AND (NOT $6 OR pool.scoring_success)"#,
        )
        .bind(survey_id)
        .bind(invite_token)
        .bind(partner_id)
        .bind(email)
        .bind(exclude_input_id)
        .bind(prior_success_only)
        .fetch_one(&mut *conn)
        .await
    }

    /// Ordinal of `input_id` inside its pool (1-based) — the human-facing
    /// "attempt #N" of the drift refusal. Any-state members count: an
    /// attempt is an attempt from the moment its row exists.
    pub async fn pool_ordinal(
        conn: &mut PgConnection,
        survey_id: Uuid,
        invite_token: Option<&str>,
        partner_id: Option<Uuid>,
        email: Option<&str>,
        input_id: Uuid,
    ) -> Result<i32, sqlx::Error> {
        sqlx::query_scalar::<_, i32>(
            r#"SELECT count(*)::int + 1 FROM survey.survey_user_inputs pool
               WHERE pool.survey_id = $1
                 AND (pool.metadata->>'deleted_at') IS NULL
                 AND ($2::text IS NULL AND pool.invite_token IS NULL OR pool.invite_token = $2::text)
                 AND (pool.partner_id IS NOT NULL AND pool.partner_id = $3
                      OR pool.email IS NOT NULL AND pool.email = $4)
                 AND pool.id <> $5"#,
        )
        .bind(survey_id)
        .bind(invite_token)
        .bind(partner_id)
        .bind(email)
        .bind(input_id)
        .fetch_one(&mut *conn)
        .await
    }
}

impl Default for AttemptRepository {
    fn default() -> Self {
        Self::new()
    }
}
