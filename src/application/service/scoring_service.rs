//! `ScoringService` — the scoring engine: the speed-rating formulas, the
//! frozen-denominator fold, and the sanctioned regrade
//! (hand-written, user-owned; see `metaphor.codegen.yaml`).
//!
//! Port rulings recorded here (the spec's §5.3/§5.4 carry the mandates;
//! these are the concrete shapes):
//!
//! - **The denominator freezes VALUES, not just ids.** At attempt entry
//!   the snapshot rows carry the question's `answer_score` + scored flag
//!   in their metadata (see `ScoringRepository::snapshot_denominator`);
//!   every later fold reads ONLY those frozen values. A mid-attempt
//!   weight edit, question soft-delete, or new-question add cannot move
//!   an in-flight attempt's score — the recomputation is byte-identical.
//! - **`0.0` correct answers ARE scoreable.** Numerical/date correctness
//!   is an explicit `Option<f64>`/value equality, never a truthiness
//!   check (the upstream defect does not port).
//! - **The speed engine is pure** (`speed_factor`) so the formula table
//!   is assertable without a database: `< 2 s` full credit, over-limit or
//!   a superseded question exactly 50 %, between them the linear decay
//!   `points/2 * (1 + (limit - secs)/(limit - 2))` — a 50 % floor for
//!   correct answers by construction.
//! - **`speed_seconds` is the immutable elapsed basis** captured at
//!   submit against the STORED session clock (`now + 1 s`); every later
//!   recompute (regrade included) derives from the STORED basis, never a
//!   wall clock — the recompute-drift trap.
//! - **Answer-level weights are authoritative for choice scoring** (the
//!   label row's `answer_score`, which may be negative — penalty
//!   scoring); the question-level `answer_score` is the scalar-type
//!   weight. Correctness for multiple choice is set equality (exactly
//!   the marked-correct labels chosen); for simple choice it is the
//!   chosen label's mark.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::entity::{
    Question, QuestionAnswer, Survey, SurveyAnswerType, SurveyQuestionType, UserInput, UserInputLine,
};
use crate::infrastructure::persistence::scoring_repository::{
    FrozenQuestion, ScoringRepository,
};
use crate::application::service::survey_write_service::SurveyWriteError;

/// Elapsed seconds under which the line earns 100 % of its points.
pub const SPEED_FULL_CREDIT_SECONDS: f64 = 2.0;

/// The pure speed-rating engine (SVM-11, ported verbatim):
///
/// - `elapsed < 2 s` → `1.0` (full credit);
/// - `elapsed > limit` (or the question was superseded — the caller
///   passes the 0.5 floor directly for that case) → `0.5`;
/// - between → `0.5 * (1 + (limit - elapsed) / (limit - 2))` — linear
///   from 1.0 at `elapsed = 2` down to 0.5 at `elapsed = limit`.
///
/// A degenerate limit (`<= 2 s`) cannot express the window: full credit
/// for anything under it, 50 % over it.
pub fn speed_factor(elapsed_seconds: f64, limit_seconds: f64) -> f64 {
    if limit_seconds <= SPEED_FULL_CREDIT_SECONDS {
        return if elapsed_seconds <= limit_seconds { 1.0 } else { 0.5 };
    }
    if elapsed_seconds < SPEED_FULL_CREDIT_SECONDS {
        1.0
    } else if elapsed_seconds > limit_seconds {
        0.5
    } else {
        0.5 * (1.0 + (limit_seconds - elapsed_seconds) / (limit_seconds - SPEED_FULL_CREDIT_SECONDS))
    }
}

/// The typed answer value an intake draft resolves to — shared by the
/// intake path (storage dispatch) and the grading fold (correctness).
#[derive(Debug, Clone, PartialEq)]
pub enum AnswerValue {
    Skipped,
    Char(String),
    Text(String),
    Number(f64),
    Scale(i32),
    Date(NaiveDate),
    Datetime(DateTime<Utc>),
    Choice(Vec<Uuid>),
    /// (matrix row label id, chosen column label id) pairs.
    Matrix(Vec<(Uuid, Uuid)>),
    Comment(String),
}

/// The write-once score payload of one line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineGrade {
    pub answer_score: Option<f64>,
    pub answer_is_correct: Option<bool>,
    pub speed_seconds: Option<i32>,
}

impl LineGrade {
    fn unscored() -> Self {
        Self { answer_score: None, answer_is_correct: None, speed_seconds: None }
    }
}

/// The recomputed attempt totals (the stored triple on the input row).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoreTotals {
    /// Raw sum of line scores — CAN go negative (the leaderboard ranks by
    /// this; statistics read the clamped percentage).
    pub total: f64,
    /// The frozen denominator (sum of frozen weights over scored snapshot
    /// questions).
    pub denominator: f64,
    /// 0-floored percentage against the frozen denominator.
    pub percentage: f64,
    /// `percentage >= survey.scoring_success_min`, evaluated live.
    pub success: bool,
}

/// Everything one grading needs besides the value itself.
pub struct GradeContext<'a> {
    pub survey: &'a Survey,
    pub input: &'a UserInput,
    pub question: &'a Question,
    /// The label rows of the question (choice/matrix grading data).
    pub answers: &'a [QuestionAnswer],
    pub now: DateTime<Utc>,
}

/// The scoring engine. Stateless over the pool — every verb opens its own
/// transaction; correctness lives in the SQL + the fold, not in cached
/// state.
pub struct ScoringService {
    pool: PgPool,
}

impl ScoringService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    // ── the grading fold (pure once the rows are loaded) ──────────────────────

    /// Grade one submitted answer. Raw correctness/weight first (type
    /// dispatch), then the speed factor — applied ONLY when the raw score
    /// is positive, the attempt is a session answer, the survey runs
    /// speed rating, and the question is time-limited; a question that is
    /// not the live session cursor grades at the 50 % floor (superseded).
    pub fn grade(ctx: &GradeContext<'_>, value: &AnswerValue) -> LineGrade {
        let mut grade = Self::raw_grade(ctx, value);
        if let Some(raw) = grade.answer_score {
            if raw > 0.0 && Self::speed_eligible(ctx) {
                let factor = if ctx.survey.session_question_id == Some(ctx.question.id) {
                    let elapsed = Self::elapsed_seconds(ctx);
                    let limit = ctx.question.time_limit.unwrap_or(0).max(0) as f64;
                    speed_factor(elapsed, limit)
                } else {
                    // Superseded question: exactly 50 %.
                    0.5
                };
                grade.answer_score = Some(raw * factor);
                grade.speed_seconds = Self::captured_speed_seconds(ctx);
            }
        }
        grade
    }

    /// Raw (pre-speed) weight + correctness by question type.
    pub(crate) fn raw_grade(ctx: &GradeContext<'_>, value: &AnswerValue) -> LineGrade {
        let q = ctx.question;
        let Some(qtype) = q.question_type else {
            return LineGrade::unscored();
        };
        let survey_scores = !matches!(
            ctx.survey.scoring_type,
            crate::domain::entity::SurveyScoringType::NoScoring
        );
        match (qtype, value) {
            (
                SurveyQuestionType::NumericalBox,
                AnswerValue::Number(v),
            ) => {
                if !survey_scores || !q.is_scored_question {
                    return LineGrade::unscored();
                }
                // Explicit Option equality: Some(0.0) == Some(0.0) is
                // CORRECT — the truthiness defect does not port.
                let correct = q.answer_numerical_box == Some(*v);
                LineGrade {
                    answer_score: Some(if correct { q.answer_score } else { 0.0 }),
                    answer_is_correct: Some(correct),
                    speed_seconds: None,
                }
            }
            (SurveyQuestionType::Date, AnswerValue::Date(v)) => {
                if !survey_scores || !q.is_scored_question {
                    return LineGrade::unscored();
                }
                let correct = q.answer_date == Some(*v);
                LineGrade {
                    answer_score: Some(if correct { q.answer_score } else { 0.0 }),
                    answer_is_correct: Some(correct),
                    speed_seconds: None,
                }
            }
            (SurveyQuestionType::Datetime, AnswerValue::Datetime(v)) => {
                if !survey_scores || !q.is_scored_question {
                    return LineGrade::unscored();
                }
                let correct = q.answer_datetime == Some(*v);
                LineGrade {
                    answer_score: Some(if correct { q.answer_score } else { 0.0 }),
                    answer_is_correct: Some(correct),
                    speed_seconds: None,
                }
            }
            (
                SurveyQuestionType::SimpleChoice,
                AnswerValue::Choice(chosen),
            ) => {
                if !survey_scores || !q.is_scored_question {
                    return LineGrade::unscored();
                }
                let chosen_rows: Vec<&QuestionAnswer> = ctx
                    .answers
                    .iter()
                    .filter(|a| chosen.contains(&a.id))
                    .collect();
                if chosen_rows.is_empty() {
                    return LineGrade::unscored();
                }
                // Simple choice: the single chosen label's own mark.
                let row = chosen_rows[0];
                LineGrade {
                    answer_score: Some(if row.is_correct { Self::answer_weight(row, q) } else { 0.0 }),
                    answer_is_correct: Some(row.is_correct),
                    speed_seconds: None,
                }
            }
            (
                SurveyQuestionType::MultipleChoice,
                AnswerValue::Choice(chosen),
            ) => {
                if !survey_scores || !q.is_scored_question {
                    return LineGrade::unscored();
                }
                let correct_ids: Vec<Uuid> =
                    ctx.answers.iter().filter(|a| a.is_correct).map(|a| a.id).collect();
                let mut chosen_sorted = chosen.clone();
                chosen_sorted.sort();
                let mut correct_sorted = correct_ids.clone();
                correct_sorted.sort();
                let exactly_correct = chosen_sorted == correct_sorted && !chosen.is_empty();
                // Answer-level weights accumulate (penalties may drive the
                // sum negative); set equality decides the boolean.
                let sum: f64 = ctx
                    .answers
                    .iter()
                    .filter(|a| chosen.contains(&a.id))
                    .map(|a| Self::answer_weight(a, q))
                    .sum();
                LineGrade {
                    answer_score: Some(sum),
                    answer_is_correct: Some(exactly_correct),
                    speed_seconds: None,
                }
            }
            (
                SurveyQuestionType::Matrix,
                AnswerValue::Matrix(cells),
            ) => {
                if !survey_scores || !q.is_scored_question || cells.is_empty() {
                    return LineGrade::unscored();
                }
                // Matrix: each cell is a mini choice against the column
                // labels; correct cells sum, wrong cells zero.
                let mut sum = 0.0;
                let mut all_correct = true;
                for (_row, col) in cells {
                    match ctx.answers.iter().find(|a| a.id == *col) {
                        Some(col_row) if col_row.is_correct => sum += Self::answer_weight(col_row, q),
                        Some(_) => all_correct = false,
                        None => all_correct = false,
                    }
                }
                LineGrade {
                    answer_score: Some(sum),
                    answer_is_correct: Some(all_correct),
                    speed_seconds: None,
                }
            }
            _ => LineGrade::unscored(),
        }
    }

    /// The label row's own weight when it carries one, else the question's
    /// (seeds mirror the upstream: labels inherit the question weight).
    pub(crate) fn answer_weight(answer: &QuestionAnswer, question: &Question) -> f64 {
        if answer.answer_score != 0.0 {
            answer.answer_score
        } else {
            question.answer_score
        }
    }

    fn speed_eligible(ctx: &GradeContext<'_>) -> bool {
        ctx.input.is_session_answer
            && ctx.survey.session_speed_rating
            && ctx.question.is_time_limited
            && ctx.question.time_limit.map_or(false, |t| t > 0)
    }

    /// The stored-clock elapsed basis, clamped at zero (clock skew or a
    /// pre-clock line must never produce a negative elapsed).
    fn elapsed_seconds(ctx: &GradeContext<'_>) -> f64 {
        ctx.survey
            .session_question_start_time
            .map(|clock| (ctx.now - clock).num_milliseconds().max(0) as f64 / 1000.0)
            .unwrap_or(0.0)
    }

    fn captured_speed_seconds(ctx: &GradeContext<'_>) -> Option<i32> {
        if !ctx.input.is_session_answer {
            return None;
        }
        ctx.survey
            .session_question_start_time
            .map(|clock| ((ctx.now - clock).num_seconds()).max(0) as i32)
    }

    // ── the fold over stored lines (the recomputation) ────────────────────────

    /// Fold the stored lines against the frozen denominator into the
    /// stored input triple. Pure — the caller persists it.
    pub fn fold_totals(
        survey: &Survey,
        lines: &[UserInputLine],
        snapshot: &[FrozenQuestion],
    ) -> ScoreTotals {
        let total: f64 = lines.iter().map(|l| l.answer_score.unwrap_or(0.0)).sum();
        let denominator: f64 = snapshot
            .iter()
            .filter(|f| f.frozen_scored)
            .map(|f| f.frozen_answer_score)
            .sum();
        let percentage =
            if denominator > 0.0 { (100.0 * total / denominator).max(0.0) } else { 0.0 };
        ScoreTotals {
            total,
            denominator,
            percentage,
            success: percentage >= survey.scoring_success_min,
        }
    }

    /// Recompute + persist the input's scoring triple from its stored
    /// lines + frozen snapshot (mid-attempt included — success flips the
    /// moment the threshold clears).
    pub async fn recompute_input_scores(
        &self,
        conn: &mut PgConnection,
        input: &UserInput,
        survey: &Survey,
    ) -> Result<ScoreTotals, SurveyWriteError> {
        let lines = ScoringRepository::lines_for_input(conn, input.id).await?;
        let snapshot = ScoringRepository::frozen_snapshot(conn, input.id).await?;
        let totals = Self::fold_totals(survey, &lines, &snapshot);
        crate::infrastructure::persistence::attempt_repository::AttemptRepository::set_scoring(
            conn,
            input.id,
            totals.percentage,
            totals.total,
            totals.success,
        )
        .await?;
        Ok(totals)
    }

    // ── the sanctioned regrade ────────────────────────────────────────────────

    /// The guarded `regrade_question` verb: recompute every stored line of
    /// ONE question from the LIVE correct-answer data and the STORED value
    /// columns + STORED `speed_seconds` (never a wall clock), under the
    /// transaction-local regrade marker the drift trigger consults. The
    /// old/new pair is appended to the line's metadata (`regrade_history`)
    /// for audit. Returns (lines touched, old->new score pairs).
    pub async fn regrade_question(
        &self,
        survey_id: Uuid,
        question_id: Uuid,
    ) -> Result<Vec<(Uuid, Option<f64>, Option<f64>)>, SurveyWriteError> {
        let mut tx = self.pool.begin().await?;
        let survey = crate::infrastructure::persistence::survey_session_repository::SurveySessionRepository::find_survey_by_id(&mut tx, survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(survey_id))?;
        let question = sqlx::query_as::<_, Question>(
            r#"SELECT * FROM survey.survey_questions
               WHERE id = $1 AND survey_id = $2 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(question_id)
        .bind(survey_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(SurveyWriteError::QuestionNotFound(question_id))?;
        let answers = ScoringRepository::answers_for_question(&mut tx, question_id).await?;

        // Load the takers of every stored line of this question.
        let line_rows = sqlx::query_as::<_, UserInputLine>(
            r#"SELECT * FROM survey.survey_user_input_lines
               WHERE question_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(question_id)
        .fetch_all(&mut *tx)
        .await?;

        // The marker is armed for THIS transaction only.
        ScoringRepository::arm_regrade_marker(&mut tx).await?;

        let mut touched = Vec::new();
        for line in &line_rows {
            let input = crate::infrastructure::persistence::attempt_repository::AttemptRepository::find_live_by_id(&mut tx, line.user_input_id)
                .await?
                .ok_or(SurveyWriteError::InputNotFound(line.user_input_id))?;
            let value = Self::stored_value_of(line);
            let ctx = GradeContext {
                survey: &survey,
                input: &input,
                question: &question,
                answers: &answers,
                now: Utc::now(),
            };
            let mut grade = Self::raw_grade(&ctx, &value);
            // Re-apply the speed factor from the STORED basis (the whole
            // point: the recompute must reproduce, not re-derive from a
            // fresh wall clock).
            if let Some(raw) = grade.answer_score {
                if raw > 0.0 && Self::speed_eligible(&ctx) {
                    let factor = if ctx.survey.session_question_id == Some(ctx.question.id) {
                        let stored = line.speed_seconds.unwrap_or(0) as f64;
                        let limit = ctx.question.time_limit.unwrap_or(0).max(0) as f64;
                        speed_factor(stored, limit)
                    } else {
                        0.5
                    };
                    grade.answer_score = Some(raw * factor);
                }
            }

            ScoringRepository::regrade_score(&mut tx, line.id, grade.answer_score, grade.answer_is_correct)
                .await?;
            // The audit pair on the line's metadata.
            sqlx::query(
                r#"UPDATE survey.survey_user_input_lines
                   SET metadata = jsonb_set(
                        metadata,
                        '{regrade_history}',
                        COALESCE(metadata->'regrade_history', '[]'::jsonb)
                          || jsonb_build_array(jsonb_build_object(
                               'at', to_jsonb(NOW()),
                               'old_score', to_jsonb($2),
                               'new_score', to_jsonb($3))))
                   WHERE id = $1"#,
            )
            .bind(line.id)
            .bind(line.answer_score)
            .bind(grade.answer_score)
            .execute(&mut *tx)
            .await?;

            touched.push((line.id, line.answer_score, grade.answer_score));
        }

        // Recompute the affected inputs' stored triples from the frozen
        // denominators (unchanged by construction — only weights moved).
        let mut seen = std::collections::HashSet::new();
        for line in &line_rows {
            if seen.insert(line.user_input_id) {
                if let Some(input) = crate::infrastructure::persistence::attempt_repository::AttemptRepository::find_live_by_id(&mut tx, line.user_input_id).await? {
                    self.recompute_input_scores(&mut tx, &input, &survey).await?;
                }
            }
        }

        tx.commit().await?;
        Ok(touched)
    }

    /// Rebuild the typed value from a stored line (the regrade input).
    pub fn stored_value_of(line: &UserInputLine) -> AnswerValue {
        if line.skipped {
            return AnswerValue::Skipped;
        }
        if let Some(t) = line.answer_type {
            match t {
                SurveyAnswerType::CharBox => {
                    return line.value_char_box.clone().map_or(AnswerValue::Skipped, AnswerValue::Char)
                }
                SurveyAnswerType::TextBox => {
                    return line.value_text_box.clone().map_or(AnswerValue::Skipped, AnswerValue::Text)
                }
                SurveyAnswerType::NumericalBox => {
                    return line.value_numerical_box.map_or(AnswerValue::Skipped, AnswerValue::Number)
                }
                SurveyAnswerType::Scale => {
                    return line.value_scale.map_or(AnswerValue::Skipped, AnswerValue::Scale)
                }
                SurveyAnswerType::Date => {
                    return line.value_date.map_or(AnswerValue::Skipped, AnswerValue::Date)
                }
                SurveyAnswerType::Datetime => {
                    return line.value_datetime.map_or(AnswerValue::Skipped, AnswerValue::Datetime)
                }
                SurveyAnswerType::Suggestion => {
                    // The caller reassembles choice/matrix lines from the
                    // full line set (multiplicity lives ACROSS rows); a
                    // single suggestion row is meaningless alone.
                    return AnswerValue::Skipped;
                }
            }
        }
        AnswerValue::Skipped
    }
}
