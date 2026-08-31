//! `IntakeService` — the answer-intake runtime: begin, per-question
//! submit, and the terminal `done` edge with its post-commit publishes
//! (hand-written, user-owned; see `metaphor.codegen.yaml`).
//!
//! The submit pipeline per question (spec §5.2, kept verbatim):
//!
//! 1. **Capability + state gates** — the Tier A link verifies; a `done`
//!    attempt refuses (re-entry guard); a `new` attempt lazily begins
//!    (the clock starts at the FIRST interaction, not at mint).
//! 2. **Anti-cheat windows** — attempt pre-creation already happened at
//!    entry (the row exists before any submit — no pre-verify submit);
//!    the survey-wide limit enforces `deadline + 10 s`; the per-question
//!    limit enforces `session clock + limit + 3 s` for the live cursor
//!    question. Within the grace but beyond the limit, validation is
//!    SUPPRESSED (a late answer is saved unvalidated rather than lost).
//! 3. **Frozen-set membership** — answers only land on questions in the
//!    attempt's frozen snapshot (a mid-attempt added question cannot be
//!    answered; a removed one is not).
//! 4. **Overwrite gate** — overwriting an existing answer requires the
//!    survey's go-back flag; scalar overwrites touch VALUE columns only
//!    (the score triple is write-once), choice/matrix overwrite is
//!    delete-and-recreate (fresh INSERTs, fresh scores).
//! 5. **Validation** — ranges, length, email, mandatory — only for the
//!    free-input families and only when `validation_required`.
//! 6. **Conditional clearing** — a choice answer that leaves a
//!    conditional question's trigger unchosen deletes that question's
//!    dependent lines (scoring correctness over UX, kept).
//! 7. **Recompute** — the stored input triple folds against the frozen
//!    denominator on EVERY submit (success flips mid-attempt the moment
//!    the threshold clears).
//!
//! The terminal edge publishes post-commit with per-input error
//! isolation: the `AnswerCompleted` fact crosses the event sink, the
//! certification fact crosses the fail-closed port — a refused publish
//! is an audited critical event, NEVER a rollback of the completion.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::service::attempt_service::AttemptService;
use crate::application::service::certification_port::{
    CertificationFact, CertificationGrantPort, CertificationGrantSlot,
};
use crate::application::service::event_sink::{EventSinkSlot, Fact};
use crate::application::service::scoring_service::{
    GradeContext, LineGrade, ScoringService, ScoreTotals, AnswerValue,
};
use crate::application::service::survey_write_service::SurveyWriteError;
use crate::domain::entity::{
    Question, QuestionAnswer, Survey, SurveyAnswerType, UserInput,
};
use crate::infrastructure::persistence::attempt_repository::AttemptRepository;
use crate::infrastructure::persistence::scoring_repository::ScoringRepository;
use crate::infrastructure::persistence::survey_session_repository::SurveySessionRepository;

/// The survey-wide grace beyond the stored deadline (10 s).
pub const SURVEY_GRACE_SECONDS: i64 = 10;
/// The per-question grace beyond the question time limit (3 s).
pub const QUESTION_GRACE_SECONDS: i64 = 3;

/// What a submit carries over the wire. The typed value families mirror
/// the question types; `Comment` rides the char column (comments are
/// never scored); `Skipped` is an explicit non-answer.
#[derive(Debug, Clone, PartialEq)]
pub enum AnswerDraft {
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

impl AnswerDraft {
    /// The storage answer-type of this draft (`None` = skipped row).
    pub fn answer_type(&self) -> Option<SurveyAnswerType> {
        match self {
            Self::Skipped => None,
            Self::Char(_) | Self::Comment(_) => Some(SurveyAnswerType::CharBox),
            Self::Text(_) => Some(SurveyAnswerType::TextBox),
            Self::Number(_) => Some(SurveyAnswerType::NumericalBox),
            Self::Scale(_) => Some(SurveyAnswerType::Scale),
            Self::Date(_) => Some(SurveyAnswerType::Date),
            Self::Datetime(_) => Some(SurveyAnswerType::Datetime),
            Self::Choice(_) | Self::Matrix(_) => Some(SurveyAnswerType::Suggestion),
        }
    }

    pub fn is_skipped(&self) -> bool {
        matches!(self, Self::Skipped)
    }

    /// The grading view of this draft.
    pub fn value(&self) -> AnswerValue {
        match self.clone() {
            Self::Skipped => AnswerValue::Skipped,
            Self::Char(v) | Self::Comment(v) => AnswerValue::Char(v),
            Self::Text(v) => AnswerValue::Text(v),
            Self::Number(v) => AnswerValue::Number(v),
            Self::Scale(v) => AnswerValue::Scale(v),
            Self::Date(v) => AnswerValue::Date(v),
            Self::Datetime(v) => AnswerValue::Datetime(v),
            Self::Choice(ids) => AnswerValue::Choice(ids),
            Self::Matrix(cells) => AnswerValue::Matrix(cells),
        }
    }
}

/// What one submit returns: the (re)stamped input, the graded line
/// payload, and the recomputed totals.
#[derive(Debug, Clone, Copy)]
pub struct SubmitOutcome {
    pub totals: ScoreTotals,
    pub line: LineGrade,
}

/// The answer-intake runtime.
pub struct IntakeService {
    pool: PgPool,
    scoring: ScoringService,
    attempts: std::sync::Arc<AttemptService>,
    sink: EventSinkSlot,
    certification: CertificationGrantSlot,
}

impl IntakeService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: PgPool,
        attempts: std::sync::Arc<AttemptService>,
        sink: EventSinkSlot,
        certification: CertificationGrantSlot,
    ) -> Self {
        let scoring = ScoringService::new(pool.clone());
        Self { pool, scoring, attempts, sink, certification }
    }

    // ── begin ─────────────────────────────────────────────────────────────────

    /// Open the attempt (`new -> in_progress`), stamping the start and
    /// deriving the deadline from the survey-wide limit when there is
    /// none yet. Idempotent: an already-running attempt returns its row;
    /// a finished one refuses.
    pub async fn begin(&self, link: &str) -> Result<UserInput, SurveyWriteError> {
        let head = self.attempts.verify_capability(link).await?;
        let mut tx = self.pool.begin().await?;
        let survey = SurveySessionRepository::find_survey_by_id(&mut tx, head.survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(head.survey_id))?;
        let deadline = if survey.is_time_limited {
            Some(Utc::now() + Duration::seconds((survey.time_limit * 60.0) as i64))
        } else {
            None
        };
        match AttemptRepository::transition_begin(&mut tx, head.id, Utc::now(), deadline).await? {
            Some(row) => {
                tx.commit().await?;
                Ok(row)
            }
            None => {
                // Zero rows: not `new` anymore. Idempotent on
                // in_progress; refusal on terminal.
                let current = AttemptRepository::find_live_by_id(&mut tx, head.id)
                    .await?
                    .ok_or(SurveyWriteError::InputNotFound(head.id))?;
                tx.commit().await?;
                if current.state == crate::domain::entity::SurveyInputState::Done {
                    return Err(SurveyWriteError::AttemptNotSubmittable);
                }
                Ok(current)
            }
        }
    }

    // ── submit ────────────────────────────────────────────────────────────────

    /// Submit one question's answer. See the module docs for the
    /// pipeline; every refusal is typed.
    pub async fn submit_answer(
        &self,
        link: &str,
        question_id: Uuid,
        draft: AnswerDraft,
    ) -> Result<SubmitOutcome, SurveyWriteError> {
        let head = self.attempts.verify_capability(link).await?;
        let now = Utc::now();
        let mut tx = self.pool.begin().await?;

        let input = AttemptRepository::find_live_by_id(&mut tx, head.id)
            .await?
            .ok_or(SurveyWriteError::InputNotFound(head.id))?;
        let survey = SurveySessionRepository::find_survey_by_id(&mut tx, input.survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(input.survey_id))?;
        let mut input = input;

        // Lazy begin: the clock starts at the first interaction.
        if input.state == crate::domain::entity::SurveyInputState::New {
            let deadline = if survey.is_time_limited {
                Some(now + Duration::seconds((survey.time_limit * 60.0) as i64))
            } else {
                None
            };
            input = AttemptRepository::transition_begin(&mut tx, input.id, now, deadline)
                .await?
                .ok_or(SurveyWriteError::StateConflict {
                    input_id: input.id,
                    expected: "new",
                })?;
        }

        // (2) Anti-cheat: survey-wide deadline + 10 s grace.
        if survey.is_time_limited {
            if let Some(deadline) = input.deadline {
                if now > deadline + Duration::seconds(SURVEY_GRACE_SECONDS) {
                    tx.commit().await?;
                    return Err(SurveyWriteError::SurveyTimeLimitExceeded);
                }
            }
        }

        let question = sqlx::query_as::<_, Question>(
            r#"SELECT * FROM survey.survey_questions
               WHERE id = $1 AND survey_id = $2 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(question_id)
        .bind(survey.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(SurveyWriteError::QuestionNotFound(question_id))?;
        if question.is_page {
            return Err(SurveyWriteError::ValidationFailed {
                question_id,
                reason: "pages are not answerable".into(),
            });
        }

        // (2) Anti-cheat: question limit + 3 s grace on the live cursor.
        let mut late_suppresses_validation = false;
        if question.is_time_limited
            && input.is_session_answer
            && survey.session_question_id == Some(question.id)
        {
            if let Some(clock) = survey.session_question_start_time {
                let limit = question.time_limit.unwrap_or(0).max(0) as i64;
                if now > clock + Duration::seconds(limit + QUESTION_GRACE_SECONDS) {
                    tx.commit().await?;
                    return Err(SurveyWriteError::QuestionTimeLimitExceeded);
                }
                // Beyond the limit but inside the grace: save unvalidated.
                late_suppresses_validation = now > clock + Duration::seconds(limit);
            }
        }

        // (3) Frozen-set membership.
        let frozen = ScoringRepository::frozen_snapshot(&mut tx, input.id).await?;
        if !frozen.iter().any(|f| f.question_id == question.id) {
            return Err(SurveyWriteError::ValidationFailed {
                question_id,
                reason: "question is not part of this attempt's frozen set".into(),
            });
        }

        // (5) Validation (suppressed when late-but-accepted).
        if !draft.is_skipped() && question.validation_required && !late_suppresses_validation {
            if let Err(reason) = validate(&question, &draft) {
                return Err(SurveyWriteError::ValidationFailed { question_id, reason });
            }
        }
        if question.constr_mandatory && draft.is_skipped() && !late_suppresses_validation {
            return Err(SurveyWriteError::ValidationFailed {
                question_id,
                reason: "this question is mandatory".into(),
            });
        }

        // (4) Overwrite gates + (storage dispatch).
        let is_set_answer =
            matches!(draft, AnswerDraft::Choice(_) | AnswerDraft::Matrix(_));
        let existing_scalar = if is_set_answer {
            None
        } else {
            ScoringRepository::find_scalar_line(&mut tx, input.id, question.id).await?
        };
        if existing_scalar.is_some() || is_set_answer {
            let has_existing = existing_scalar.is_some()
                || sqlx::query_scalar::<_, i64>(
                    r#"SELECT count(*) FROM survey.survey_user_input_lines
                       WHERE user_input_id = $1 AND question_id = $2
                         AND (metadata->>'deleted_at') IS NULL"#,
                )
                .bind(input.id)
                .bind(question.id)
                .fetch_one(&mut *tx)
                .await?
                > 0;
            if has_existing && !survey.users_can_go_back {
                tx.commit().await?;
                return Err(SurveyWriteError::OverwriteRefused { question_id });
            }
        }

        // An empty set draft is an explicit skip (nothing chosen).
        let draft = match draft {
            AnswerDraft::Choice(v) if v.is_empty() => AnswerDraft::Skipped,
            AnswerDraft::Matrix(v) if v.is_empty() => AnswerDraft::Skipped,
            other => other,
        };

        let answers = ScoringRepository::answers_for_question(&mut tx, question.id).await?;
        let ctx = GradeContext { survey: &survey, input: &input, question: &question, answers: &answers, now };
        let value = draft.value();
        let raw = ScoringService::raw_grade(&ctx, &value);
        let graded = ScoringService::grade(&ctx, &value);

        match &draft {
            AnswerDraft::Choice(chosen) if !chosen.is_empty() => {
                ScoringRepository::delete_question_lines(&mut tx, input.id, question.id).await?;
                let ratio = line_ratio(raw.answer_score, graded.answer_score);
                for label_id in chosen {
                    let label = answers.iter().find(|a: &&QuestionAnswer| a.id == *label_id);
                    let (w, correct) = match label {
                        Some(a) => (ScoringService::answer_weight(a, &question), a.is_correct),
                        None => (0.0, false),
                    };
                    ScoringRepository::insert_line(
                        &mut tx, Uuid::new_v4(), input.id, survey.id, question.id,
                        Some(*label_id), None, false, Some(SurveyAnswerType::Suggestion),
                        None, None, None, None, None, None,
                        Some(w * ratio), Some(correct), graded.speed_seconds, now,
                    )
                    .await?;
                }
            }
            AnswerDraft::Matrix(cells) if !cells.is_empty() => {
                ScoringRepository::delete_question_lines(&mut tx, input.id, question.id).await?;
                let ratio = line_ratio(raw.answer_score, graded.answer_score);
                for (row_id, col_id) in cells {
                    let col = answers.iter().find(|a| a.id == *col_id);
                    let (w, correct) = match col {
                        Some(a) => (ScoringService::answer_weight(a, &question), a.is_correct),
                        None => (0.0, false),
                    };
                    ScoringRepository::insert_line(
                        &mut tx, Uuid::new_v4(), input.id, survey.id, question.id,
                        Some(*col_id), Some(*row_id), false, Some(SurveyAnswerType::Suggestion),
                        None, None, None, None, None, None,
                        Some(w * ratio), Some(correct), graded.speed_seconds, now,
                    )
                    .await?;
                }
            }
            scalar_or_skip => {
                // The scalar families + explicit skips: ONE line.
                let (ch, tx_, num, scale, date, dt) = scalar_parts(scalar_or_skip);
                if let Some(line) = existing_scalar {
                    // Go-back overwrite: values move, the score triple is
                    // write-once.
                    ScoringRepository::update_scalar_value(
                        &mut tx, line.id,
                        draft.answer_type().unwrap_or(SurveyAnswerType::CharBox),
                        ch, tx_, num, scale, date, dt, draft.is_skipped(),
                    )
                    .await?;
                } else {
                    ScoringRepository::insert_line(
                        &mut tx, Uuid::new_v4(), input.id, survey.id, question.id,
                        None, None, draft.is_skipped(), draft.answer_type(),
                        ch, tx_, num, scale, date, dt,
                        graded.answer_score, graded.answer_is_correct, graded.speed_seconds, now,
                    )
                    .await?;
                }
            }
        }

        // (6) Conditional clearing: a choice answer that left a
        // conditional's trigger unchosen deletes its dependent lines.
        if let AnswerDraft::Choice(chosen) = &draft {
            clear_inactive_conditionals(&mut tx, input.id, question.id, chosen).await?;
        }

        // (7) Fold the stored triple against the frozen denominator.
        let totals = self.scoring.recompute_input_scores(&mut tx, &input, &survey).await?;
        tx.commit().await?;

        Ok(SubmitOutcome { totals, line: graded })
    }

    // ── the terminal edge ─────────────────────────────────────────────────────

    /// Finish the attempt (`in_progress -> done`), recompute the final
    /// totals, then publish post-commit with per-input isolation: the
    /// completion fact on the sink, the certification fact on the port
    /// (gated to the FIRST success in the attempt pool — the producer
    /// half of exactly-once; the consumer's idempotency key is the
    /// other).
    pub async fn finish(&self, link: &str) -> Result<UserInput, SurveyWriteError> {
        let head = self.attempts.verify_capability(link).await?;
        let mut tx = self.pool.begin().await?;
        let mut input = AttemptRepository::find_live_by_id(&mut tx, head.id)
            .await?
            .ok_or(SurveyWriteError::InputNotFound(head.id))?;
        let survey = SurveySessionRepository::find_survey_by_id(&mut tx, input.survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(input.survey_id))?;

        // A never-begun attempt still finishes (an empty submission).
        if input.state == crate::domain::entity::SurveyInputState::New {
            input = AttemptRepository::transition_begin(&mut tx, input.id, Utc::now(), None)
                .await?
                .ok_or(SurveyWriteError::StateConflict { input_id: input.id, expected: "new" })?;
        }
        if !AttemptRepository::transition_done(&mut tx, input.id, Utc::now()).await? {
            tx.commit().await?;
            return Err(SurveyWriteError::AttemptNotSubmittable);
        }
        // The terminal-edge prune: conditional questions whose trigger
        // never fired on this attempt's live lines leave the frozen set
        // so the final denominator counts only the taker's actual path.
        // Safe ONLY here — past this edge no submit can reference the
        // snapshot again (the state machine refuses re-entry).
        let keep = ScoringRepository::active_question_ids(&mut tx, input.id).await?;
        ScoringRepository::prune_inactive_questions(&mut tx, input.id, &keep).await?;
        let totals = self.scoring.recompute_input_scores(&mut tx, &input, &survey).await?;
        let final_row = AttemptRepository::find_live_by_id(&mut tx, input.id)
            .await?
            .ok_or(SurveyWriteError::InputNotFound(input.id))?;
        tx.commit().await?;

        // Post-commit publishes — isolated per input; a refusal NEVER
        // rolls the completion back.
        self.sink.record(&Fact::AnswerCompleted {
            survey_id: survey.id,
            input_id: final_row.id,
            scoring_percentage: totals.percentage,
            scoring_success: totals.success,
            test_entry: final_row.test_entry,
        });

        if survey.certification
            && survey.certification_give_badge
            && totals.success
            && !final_row.test_entry
        {
            if let Some(user_id) = final_row.user_id {
                let mut gate_conn = self.pool.acquire().await?;
                match AttemptRepository::pool_count(
                    &mut gate_conn,
                    survey.id,
                    final_row.invite_token.as_deref(),
                    final_row.partner_id,
                    final_row.email.as_deref(),
                    Some(final_row.id),
                    true,
                )
                .await
                {
                    Ok(prior) if prior == 0 => {
                        let fact = CertificationFact::new(
                            survey.id,
                            final_row.id,
                            user_id,
                            survey.certification_badge_key.clone().unwrap_or_default(),
                        );
                        if let Err(cause) =
                            self.certification.certification_passed(&fact).await
                        {
                            // Audited critical event; the completion stands.
                            tracing::error!(
                                event = "survey_certification_grant_refused",
                                survey_id = %survey.id,
                                input_id = %final_row.id,
                                error = %cause,
                                "certification publish refused"
                            );
                        }
                    }
                    Ok(_) => {} // a prior success exists: first-completion only
                    Err(e) => {
                        tracing::error!(
                            event = "survey_certification_grant_refused",
                            survey_id = %survey.id,
                            input_id = %final_row.id,
                            error = %e,
                            "certification gate could not read the attempt pool"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    event = "survey_certification_grant_refused",
                    survey_id = %survey.id,
                    input_id = %final_row.id,
                    "anonymous attempt cannot earn a badge"
                );
            }
        }

        Ok(final_row)
    }

    /// The record channel this attempt's pushes ride (the resolver's
    /// read surface).
    pub fn record_channel(&self, survey_id: Uuid) -> String {
        crate::application::service::session_read_service::record_channel(survey_id)
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────────

/// The speed-scale ratio between the graded and raw sums for per-line
/// distribution (1.0 when speed did not apply; 0.0-sum stays 0).
fn line_ratio(raw: Option<f64>, graded: Option<f64>) -> f64 {
    match (raw, graded) {
        (Some(r), Some(g)) if r != 0.0 => g / r,
        (Some(_), Some(_)) => 0.0,
        _ => 1.0,
    }
}

/// Destructure the scalar drafts into storage columns.
fn scalar_parts(
    draft: &AnswerDraft,
) -> (Option<&str>, Option<&str>, Option<f64>, Option<i32>, Option<NaiveDate>, Option<DateTime<Utc>>) {
    match draft {
        AnswerDraft::Char(v) | AnswerDraft::Comment(v) => (Some(v), None, None, None, None, None),
        AnswerDraft::Text(v) => (None, Some(v), None, None, None, None),
        AnswerDraft::Number(v) => (None, None, Some(*v), None, None, None),
        AnswerDraft::Scale(v) => (None, None, None, Some(*v), None, None),
        AnswerDraft::Date(v) => (None, None, None, None, Some(*v), None),
        AnswerDraft::Datetime(v) => (None, None, None, None, None, Some(*v)),
        _ => (None, None, None, None, None, None),
    }
}

/// The `validation_required` dispatch: range / length / email / date
/// windows. Returns the human reason on refusal.
fn validate(question: &Question, draft: &AnswerDraft) -> Result<(), String> {
    match draft {
        AnswerDraft::Number(v) => {
            if let Some(min) = question.validation_min_float_value {
                if *v < min {
                    return Err(format!("value {v} below the minimum {min}"));
                }
            }
            if let Some(max) = question.validation_max_float_value {
                if *v > max {
                    return Err(format!("value {v} above the maximum {max}"));
                }
            }
        }
        AnswerDraft::Char(v) | AnswerDraft::Comment(v) => {
            let len = v.chars().count() as i32;
            if question.validation_length_min > 0 && len < question.validation_length_min {
                return Err(format!("answer too short (min {})", question.validation_length_min));
            }
            if question.validation_length_max > 0 && len > question.validation_length_max {
                return Err(format!("answer too long (max {})", question.validation_length_max));
            }
            if question.validation_email && !v.contains('@') {
                return Err("answer must be an email address".into());
            }
        }
        AnswerDraft::Text(v) => {
            let len = v.chars().count() as i32;
            if question.validation_length_min > 0 && len < question.validation_length_min {
                return Err(format!("answer too short (min {})", question.validation_length_min));
            }
            if question.validation_length_max > 0 && len > question.validation_length_max {
                return Err(format!("answer too long (max {})", question.validation_length_max));
            }
        }
        AnswerDraft::Date(v) => {
            if let Some(min) = question.validation_min_date {
                if *v < min {
                    return Err(format!("date {v} before the minimum {min}"));
                }
            }
            if let Some(max) = question.validation_max_date {
                if *v > max {
                    return Err(format!("date {v} after the maximum {max}"));
                }
            }
        }
        AnswerDraft::Datetime(v) => {
            if let Some(min) = question.validation_min_datetime {
                if *v < min {
                    return Err(format!("datetime {v} before the minimum {min}"));
                }
            }
            if let Some(max) = question.validation_max_datetime {
                if *v > max {
                    return Err(format!("datetime {v} after the maximum {max}"));
                }
            }
        }
        AnswerDraft::Scale(v) => {
            if *v < question.scale_min || *v > question.scale_max {
                return Err(format!(
                    "scale value {v} outside {}..={}",
                    question.scale_min, question.scale_max
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Delete the lines of conditional questions whose triggering label (on
/// `parent_question_id`) was NOT among the just-chosen labels — the
/// inactive-conditional clear.
async fn clear_inactive_conditionals(
    conn: &mut sqlx::PgConnection,
    input_id: Uuid,
    parent_question_id: Uuid,
    chosen: &[Uuid],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"DELETE FROM survey.survey_user_input_lines line
           USING survey.survey_question_triggering_answers trig,
                survey.survey_question_answers label
           WHERE line.user_input_id = $1
             AND line.question_id = trig.question_id
             AND trig.suggested_answer_id = label.id
             AND label.question_id = $2
             AND trig.suggested_answer_id <> ALL($3::uuid[])"#,
    )
    .bind(input_id)
    .bind(parent_question_id)
    .bind(chosen)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
