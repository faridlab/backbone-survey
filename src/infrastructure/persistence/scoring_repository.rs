//! `ScoringRepository` — the hand-written SQL behind the answer lines, the
//! frozen scoring denominator, and the sanctioned regrade
//! (hand-authored, user-owned; see `metaphor.codegen.yaml`).
//!
//! What lives here, and why:
//!
//! - **The frozen denominator snapshot** — at attempt creation the
//!   input's `predefined_question_ids` rows are inserted carrying the
//!   question's CURRENT `answer_score` / scored flag INSIDE the join row's
//!   metadata (`frozen_answer_score`, `frozen_scored`). Freezing VALUES,
//!   not just ids, is what makes the denominator actually immutable:
//!   a mid-attempt officer edit of a question's weight, a soft-delete of
//!   a question, or a brand-new question added to the survey cannot move
//!   an in-flight attempt's computed score (later survey edits NEVER
//!   change existing scores — TR-SV-5). The scoring service reads ONLY
//!   these frozen values; the live question rows are never a scoring
//!   input outside the sanctioned `regrade` verb.
//! - **The write-once score payload** — lines are INSERTed with their
//!   `(answer_score, answer_is_correct, speed_seconds)` triple; scalar
//!   re-submits UPDATE only the value columns (the triple is immutable
//!   outside `regrade`, enforced by the `survey_score_drift_refused`
//!   trigger). Choice/matrix re-submits DELETE-AND-RECREATE their lines
//!   (fresh rows carry fresh scores — an INSERT, never an UPDATE).
//! - **The regrade marker** — `set_config('survey.allow_regrade', 'on',
//!   true)` is the transaction-local flag the drift trigger consults; it
//!   is set ONLY inside the guarded `regrade_question` verb.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::entity::{SurveyAnswerType, UserInputLine};

/// The frozen per-question scoring weight captured at attempt creation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FrozenQuestion {
    pub question_id: Uuid,
    /// The question's `answer_score` at snapshot time.
    pub frozen_answer_score: f64,
    /// The question's `is_scored_question` at snapshot time.
    pub frozen_scored: bool,
    /// The question's sequence at snapshot time (page/section grouping
    /// for statistics stays stable against later reordering).
    pub frozen_sequence: i32,
    /// The question's `page_id` at snapshot time (statistics grouping).
    pub frozen_page_id: Option<Uuid>,
}

/// Hand-written scoring SQL. Services orchestrate; this holds SQL.
pub struct ScoringRepository;

impl ScoringRepository {
    pub fn new() -> Self {
        Self
    }

    // ── the frozen denominator ────────────────────────────────────────────────

    /// Snapshot the survey's live questions into the input's
    /// `predefined_question_ids`, freezing weight + scored flag + grouping
    /// into the join row's metadata. `random` selection passes a sampled
    /// question-id list (the service owns the sampling); `all` passes
    /// None and every live non-page question is snapshotted.
    pub async fn snapshot_denominator(
        conn: &mut PgConnection,
        input_id: Uuid,
        survey_id: Uuid,
        only_question_ids: Option<&[Uuid]>,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            r#"INSERT INTO survey.survey_user_input_predefined_questions
                   (user_input_id, question_id, metadata)
               SELECT $1, q.id,
                      jsonb_build_object(
                          'frozen_answer_score', q.answer_score,
                          'frozen_scored', q.is_scored_question,
                          'frozen_sequence', q.sequence,
                          'frozen_page_id', q.page_id::text
                      )
               FROM survey.survey_questions q
               WHERE q.survey_id = $2
                 AND q.is_page = FALSE
                 AND (q.metadata->>'deleted_at') IS NULL
                 AND ($3::uuid[] IS NULL OR q.id = ANY($3::uuid[]))"#,
        )
        .bind(input_id)
        .bind(survey_id)
        .bind(only_question_ids.map(|v| v.to_vec()))
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected())
    }

    /// The frozen snapshot rows for an input (frozen values only — the
    /// live question rows are deliberately NOT joined in).
    pub async fn frozen_snapshot(
        conn: &mut PgConnection,
        input_id: Uuid,
    ) -> Result<Vec<FrozenQuestion>, sqlx::Error> {
        let rows: Vec<(Uuid, f64, bool, i32, Option<String>)> = sqlx::query_as(
            r#"SELECT question_id,
                      COALESCE((metadata->>'frozen_answer_score')::float8, 0),
                      COALESCE((metadata->>'frozen_scored')::boolean, false),
                      COALESCE((metadata->>'frozen_sequence')::int, 0),
                      NULLIF(metadata->>'frozen_page_id', '')
               FROM survey.survey_user_input_predefined_questions
               WHERE user_input_id = $1
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(input_id)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(question_id, score, scored, seq, page)| FrozenQuestion {
                question_id,
                frozen_answer_score: score,
                frozen_scored: scored,
                frozen_sequence: seq,
                frozen_page_id: page.and_then(|p| Uuid::parse_str(&p).ok()),
            })
            .collect())
    }

    /// The keep set for the finish-time prune. A snapshot question stays
    /// when ANY of:
    ///
    /// - it is not a conditional question (no triggering rows) — an
    ///   unanswered scored question BELONGS in the denominator: the score
    ///   of a blank is zero, never exempt;
    /// - it has a live answer line anyway (the go-back edge: answered
    ///   while active, the parent's answer later moved);
    /// - one of its triggering labels is among this input's live chosen
    ///   labels — the trigger fired, so the question was on the taker's
    ///   path whether or not they answered it.
    ///
    /// A conditional whose trigger never fired on the live lines is the
    /// only thing this set leaves out.
    pub async fn active_question_ids(
        conn: &mut PgConnection,
        input_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        let rows: Vec<Uuid> = sqlx::query_scalar(
            r#"SELECT snap.question_id
               FROM survey.survey_user_input_predefined_questions snap
               WHERE snap.user_input_id = $1
                 AND (snap.metadata->>'deleted_at') IS NULL
                 AND (
                     NOT EXISTS (SELECT 1 FROM survey.survey_question_triggering_answers trig
                                 WHERE trig.question_id = snap.question_id)
                     OR EXISTS (SELECT 1 FROM survey.survey_user_input_lines line
                                WHERE line.user_input_id = $1
                                  AND line.question_id = snap.question_id
                                  AND (line.metadata->>'deleted_at') IS NULL)
                     OR EXISTS (SELECT 1
                                FROM survey.survey_question_triggering_answers trig2
                                JOIN survey.survey_user_input_lines tline
                                  ON tline.user_input_id = $1
                                 AND tline.suggested_answer_id = trig2.suggested_answer_id
                                 AND (tline.metadata->>'deleted_at') IS NULL
                                WHERE trig2.question_id = snap.question_id)
                 )"#,
        )
        .bind(input_id)
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows)
    }

    /// The finish-funnel prune: drop snapshot rows for conditional
    /// questions whose trigger never fired (inactive questions never
    /// join the denominator of a finished attempt). Runs ONLY on the
    /// terminal edge — a pruned row is gone, so pruning mid-attempt
    /// would break the frozen-set membership check for a later submit
    /// after the trigger re-fires. Returns the pruned count.
    pub async fn prune_inactive_questions(
        conn: &mut PgConnection,
        input_id: Uuid,
        active_question_ids: &[Uuid],
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            r#"DELETE FROM survey.survey_user_input_predefined_questions
               WHERE user_input_id = $1
                 AND NOT (question_id = ANY($2::uuid[]))"#,
        )
        .bind(input_id)
        .bind(active_question_ids.to_vec())
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected())
    }

    // ── the answer lines ──────────────────────────────────────────────────────

    /// The live label rows of one question (choice / matrix columns; the
    /// correctness data behind choice grading and regrade).
    pub async fn answers_for_question(
        conn: &mut PgConnection,
        question_id: Uuid,
    ) -> Result<Vec<crate::domain::entity::QuestionAnswer>, sqlx::Error> {
        sqlx::query_as::<_, crate::domain::entity::QuestionAnswer>(
            r#"SELECT * FROM survey.survey_question_answers
               WHERE question_id = $1 AND (metadata->>'deleted_at') IS NULL
               ORDER BY sequence ASC, id ASC"#,
        )
        .bind(question_id)
        .fetch_all(&mut *conn)
        .await
    }

    /// All lines of one input, question-ordered (the scoring fold and the
    /// intake's clear-inactive pass read this shape).
    pub async fn lines_for_input(
        conn: &mut PgConnection,
        input_id: Uuid,
    ) -> Result<Vec<UserInputLine>, sqlx::Error> {
        sqlx::query_as::<_, UserInputLine>(
            r#"SELECT * FROM survey.survey_user_input_lines
               WHERE user_input_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(input_id)
        .fetch_all(&mut *conn)
        .await
    }

    /// The scalar line of (input, question) — the one row with no matrix
    /// row discriminator and no suggested answer (char/text/number/scale/
    /// date/datetime storage).
    pub async fn find_scalar_line(
        conn: &mut PgConnection,
        input_id: Uuid,
        question_id: Uuid,
    ) -> Result<Option<UserInputLine>, sqlx::Error> {
        sqlx::query_as::<_, UserInputLine>(
            r#"SELECT * FROM survey.survey_user_input_lines
               WHERE user_input_id = $1 AND question_id = $2
                 AND matrix_row_id IS NULL AND suggested_answer_id IS NULL
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(input_id)
        .bind(question_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Update ONLY the value columns of an existing scalar line. The
    /// score triple `(answer_score, answer_is_correct, speed_seconds)` is
    /// deliberately absent from the SET list — written once at first
    /// submit; the drift trigger refuses any other write path.
    #[allow(clippy::too_many_arguments)]
    pub async fn update_scalar_value(
        conn: &mut PgConnection,
        line_id: Uuid,
        answer_type: SurveyAnswerType,
        value_char_box: Option<&str>,
        value_text_box: Option<&str>,
        value_numerical_box: Option<f64>,
        value_scale: Option<i32>,
        value_date: Option<NaiveDate>,
        value_datetime: Option<DateTime<Utc>>,
        skipped: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE survey.survey_user_input_lines
               SET answer_type = $2,
                   value_char_box = $3,
                   value_text_box = $4,
                   value_numerical_box = $5,
                   value_scale = $6,
                   value_date = $7,
                   value_datetime = $8,
                   skipped = $9
               WHERE id = $1"#,
        )
        .bind(line_id)
        .bind(answer_type)
        .bind(value_char_box)
        .bind(value_text_box)
        .bind(value_numerical_box)
        .bind(value_scale)
        .bind(value_date)
        .bind(value_datetime)
        .bind(skipped)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Insert one answer line with its write-once score payload.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_line(
        conn: &mut PgConnection,
        id: Uuid,
        user_input_id: Uuid,
        survey_id: Uuid,
        question_id: Uuid,
        suggested_answer_id: Option<Uuid>,
        matrix_row_id: Option<Uuid>,
        skipped: bool,
        answer_type: Option<SurveyAnswerType>,
        value_char_box: Option<&str>,
        value_text_box: Option<&str>,
        value_numerical_box: Option<f64>,
        value_scale: Option<i32>,
        value_date: Option<NaiveDate>,
        value_datetime: Option<DateTime<Utc>>,
        answer_score: Option<f64>,
        answer_is_correct: Option<bool>,
        speed_seconds: Option<i32>,
        answered_at: DateTime<Utc>,
    ) -> Result<UserInputLine, sqlx::Error> {
        sqlx::query_as::<_, UserInputLine>(
            r#"INSERT INTO survey.survey_user_input_lines
                   (id, user_input_id, survey_id, question_id, suggested_answer_id,
                    matrix_row_id, skipped, answer_type, value_char_box, value_text_box,
                    value_numerical_box, value_scale, value_date, value_datetime,
                    answer_score, answer_is_correct, speed_seconds, answered_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18)
               RETURNING *"#,
        )
        .bind(id)
        .bind(user_input_id)
        .bind(survey_id)
        .bind(question_id)
        .bind(suggested_answer_id)
        .bind(matrix_row_id)
        .bind(skipped)
        .bind(answer_type)
        .bind(value_char_box)
        .bind(value_text_box)
        .bind(value_numerical_box)
        .bind(value_scale)
        .bind(value_date)
        .bind(value_datetime)
        .bind(answer_score)
        .bind(answer_is_correct)
        .bind(speed_seconds)
        .bind(answered_at)
        .fetch_one(&mut *conn)
        .await
    }

    /// Hard-delete the choice/matrix lines of one question (the
    /// DELETE-AND-RECREATE storage semantics — the recreated rows are
    /// fresh INSERTs carrying fresh scores).
    pub async fn delete_question_lines(
        conn: &mut PgConnection,
        input_id: Uuid,
        question_id: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let res = sqlx::query(
            r#"DELETE FROM survey.survey_user_input_lines
               WHERE user_input_id = $1 AND question_id = $2"#,
        )
        .bind(input_id)
        .bind(question_id)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected())
    }

    /// Overwrite the score triple under the regrade marker — the caller
    /// MUST hold `set_config('survey.allow_regrade', 'on', true)` for the
    /// transaction or the drift trigger refuses.
    pub async fn regrade_score(
        conn: &mut PgConnection,
        line_id: Uuid,
        answer_score: Option<f64>,
        answer_is_correct: Option<bool>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE survey.survey_user_input_lines
               SET answer_score = $2, answer_is_correct = $3
               WHERE id = $1"#,
        )
        .bind(line_id)
        .bind(answer_score)
        .bind(answer_is_correct)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Arm the transaction-local regrade marker (the drift trigger's only
    /// sanctioned escape hatch).
    pub async fn arm_regrade_marker(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT set_config('survey.allow_regrade', 'on', true)")
            .execute(&mut *conn)
            .await?;
        Ok(())
    }
}

impl Default for ScoringRepository {
    fn default() -> Self {
        Self::new()
    }
}
