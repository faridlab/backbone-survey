//! `SurveyWriteService` — the validated survey/question write path and the
//! live-session runtime (hand-written, user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! This file also owns `SurveyWriteError`, the error surface shared by
//! every hand-written survey service (one enum so the route layer maps ONE
//! refusal vocabulary; the public family's shared-refusal rule needs a
//! single place to decide what is indistinguishable from what).
//!
//! What lives here, and why:
//!
//! - **Clamp guards (SVM-2)** — the editable-compute guard cluster ports
//!   as WRITE-PATH CLAMPS, never silent derivations: `certification`
//!   forces `scoring_without_answers`; `no_scoring` clears certification;
//!   `certification_give_badge = users_login_required AND certification`;
//!   `is_attempts_limited` is forced on when conditional questions exist
//!   or token-gated login applies; `question_type` clamps to NULL on
//!   pages; `validation_required` clamps off outside the validated
//!   families. The user's choice persists until a guard clamps it — a
//!   clamp is a guard, not a derivation.
//! - **The SV-B3 fix** — there is no early-return write path: one update
//!   touching certification flags AND speed-rating settings applies ALL
//!   clamps and persists ALL propagated fields
//!   (`session_speed_rating_time_limit` onto the questions) in ONE
//!   transaction.
//! - **The session verbs** — arm (clamp layout + mint code + `ready`),
//!   advance (row-locked cursor move + `now()+1s` clock + push fact),
//!   end (forward-only bulk-done + channel-closing fact). The advance
//!   race is closed by `SELECT ... FOR UPDATE` (SV-B4); the +50 %
//!   compensation for late answers stays a per-formula concern.
//! - **Zero crons** — nothing here schedules; expiry/lazy completion
//!   stays read-path, badge granting is the post-commit event publish.

use chrono::{DateTime, Duration, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::application::service::event_sink::{EventSinkSlot, Fact};
use crate::domain::entity::Survey;
use crate::infrastructure::persistence::survey_session_repository::SurveySessionRepository;

// ─── the shared error surface ─────────────────────────────────────────────────

/// The typed error vocabulary of the hand-written survey services.
/// `code()` is the stable machine string; `http_status()` maps the route
/// layer. Refusals that must stay indistinguishable to callers share one
/// code (`survey_attempt_not_submittable`) — the no-oracle rule.
#[derive(Debug, thiserror::Error)]
pub enum SurveyWriteError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("internal error: {0}")]
    Internal(String),
    #[error("no token secret is configured (set SURVEY_TOKEN_SECRET or pass one at composition)")]
    SecretNotConfigured,

    #[error("survey {0} not found")]
    SurveyNotFound(Uuid),
    #[error("question {0} not found")]
    QuestionNotFound(Uuid),
    #[error("attempt {0} not found")]
    InputNotFound(Uuid),

    // ── session verbs ──
    #[error("a session is already armed or running for this survey")]
    SessionAlreadyArmed,
    /// Certification and the armed-session flow are disjoint shapes: a
    /// session is live facilitation (shared code, leaderboard, speed
    /// rating); certification is an individual credential. Refused at
    /// arm time rather than silently mis-scored at finish.
    #[error("a certification survey cannot run as an armed session")]
    SessionCertificationConflict,
    #[error("no session is running for this survey")]
    SessionNotRunning,
    #[error("no next question to advance to")]
    NoNextQuestion,
    #[error("the session-code namespace is exhausted (no unique code up to the length ceiling)")]
    SessionCodeExhausted,
    #[error("the session code is unknown or no longer valid")]
    SessionCodeNotValid,
    #[error("the session code is locked; retry after the lockout window")]
    SessionCodeLocked { retry_after_seconds: i64 },
    #[error("attempts are too closely spaced; wait before retrying")]
    SessionCodeSpacing,

    // ── attempt state ──
    #[error("attempt {input_id} is not in the expected state ({expected})")]
    StateConflict { input_id: Uuid, expected: &'static str },
    /// The DB-level monotonic guard fired (raw SQL or an unexpected path
    /// tried a backward edge). Surfaced loudly, never swallowed.
    #[error("attempt state is not monotonic: {detail}")]
    StateNotMonotonic { detail: String },
    /// The write-once score payload rule: a later write would have moved
    /// `answer_score`/`answer_is_correct`/`speed_seconds` outside the
    /// sanctioned regrade. NEVER a silent overwrite.
    #[error(
        "score drift refused (line {line_id}, attempt #{attempt_no}): the score payload is written once at submit"
    )]
    ScoreDriftRefused { line_id: Uuid, attempt_no: i32 },

    // ── token / entry gates (the shared-refusal family) ──
    #[error("the attempt is not submittable (unknown, used, malformed, or finished)")]
    AttemptNotSubmittable,
    #[error("the attempt is not submittable (unknown, used, malformed, or finished)")]
    AttemptExpired,

    #[error("the survey is closed (archived)")]
    SurveyClosed,
    #[error("this survey requires an invitation (token access)")]
    SurveyNotPublicAccess,
    #[error("this survey requires an authenticated session")]
    LoginRequired,
    #[error("no attempts left for this identity")]
    AttemptsExhausted { limit: i32 },
    #[error("the attempt deadline has passed")]
    DeadlineExceeded,
    #[error("the survey time limit was exceeded beyond the grace window")]
    SurveyTimeLimitExceeded,
    #[error("the question time limit was exceeded beyond the grace window")]
    QuestionTimeLimitExceeded,

    // ── intake validation ──
    #[error("validation failed for question {question_id}: {reason}")]
    ValidationFailed { question_id: Uuid, reason: String },
    #[error("overwriting an existing answer requires the survey to allow going back")]
    OverwriteRefused { question_id: Uuid },
}

impl SurveyWriteError {
    /// Stable machine code. `AttemptNotSubmittable` and `AttemptExpired`
    /// deliberately share one code — the body must not carry an oracle.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Db(_) => "database_error",
            Self::Internal(_) => "internal_error",
            Self::SecretNotConfigured => "survey_secret_not_configured",
            Self::SurveyNotFound(_) => "survey_not_found",
            Self::QuestionNotFound(_) => "question_not_found",
            Self::InputNotFound(_) => "survey_input_not_found",
            Self::SessionAlreadyArmed => "survey_session_already_armed",
            Self::SessionCertificationConflict => "survey_session_certification_conflict",
            Self::SessionNotRunning => "survey_session_not_running",
            Self::NoNextQuestion => "survey_session_no_next_question",
            Self::SessionCodeExhausted => "survey_session_code_exhausted",
            Self::SessionCodeNotValid => "survey_session_code_not_valid",
            Self::SessionCodeLocked { .. } => "survey_session_code_locked",
            Self::SessionCodeSpacing => "survey_session_code_spacing",
            Self::StateConflict { .. } => "survey_input_state_conflict",
            Self::StateNotMonotonic { .. } => "survey_input_state_not_monotonic",
            Self::ScoreDriftRefused { .. } => "survey_score_drift_refused",
            Self::AttemptNotSubmittable | Self::AttemptExpired => "survey_attempt_not_submittable",
            Self::SurveyClosed => "survey_closed",
            Self::SurveyNotPublicAccess => "survey_not_public_access",
            Self::LoginRequired => "survey_login_required",
            Self::AttemptsExhausted { .. } => "survey_attempts_exhausted",
            Self::DeadlineExceeded => "survey_deadline_exceeded",
            Self::SurveyTimeLimitExceeded => "survey_time_limit_exceeded",
            Self::QuestionTimeLimitExceeded => "survey_question_time_limit_exceeded",
            Self::ValidationFailed { .. } => "survey_validation_failed",
            Self::OverwriteRefused { .. } => "survey_overwrite_refused",
        }
    }

    pub fn http_status(&self) -> u16 {
        match self {
            Self::Db(_) | Self::Internal(_) | Self::SecretNotConfigured => 500,
            Self::SurveyNotFound(_) | Self::QuestionNotFound(_) | Self::InputNotFound(_) => 404,
            Self::AttemptExpired => 410,
            Self::SessionCodeLocked { .. } | Self::SessionCodeSpacing => 429,
            Self::StateConflict { .. }
            | Self::StateNotMonotonic { .. }
            | Self::ScoreDriftRefused { .. }
            | Self::AttemptNotSubmittable
            | Self::SessionAlreadyArmed
            | Self::SessionCertificationConflict
            | Self::SessionNotRunning
            | Self::SurveyClosed
            | Self::SurveyNotPublicAccess
            | Self::LoginRequired
            | Self::AttemptsExhausted { .. }
            | Self::DeadlineExceeded
            | Self::SurveyTimeLimitExceeded
            | Self::QuestionTimeLimitExceeded
            | Self::OverwriteRefused { .. } => 409,
            Self::ValidationFailed { .. } => 422,
            Self::NoNextQuestion | Self::SessionCodeExhausted | Self::SessionCodeNotValid => 409,
        }
    }
}

// ─── clamp guards (SVM-2) ─────────────────────────────────────────────────────

/// The survey-level clamp cluster. Applied on create + every update, in
/// ONE place, BEFORE persisting — there is no early-return path around
/// it (SV-B3).
pub fn apply_survey_clamps(survey: &mut Survey, has_conditional_questions: bool) {
    use crate::domain::entity::SurveyScoringType;
    // certification ⇒ real scoring; no_scoring ⇒ no certification.
    if survey.certification && survey.scoring_type == SurveyScoringType::NoScoring {
        survey.scoring_type = SurveyScoringType::ScoringWithoutAnswers;
    }
    if survey.scoring_type == SurveyScoringType::NoScoring {
        survey.certification = false;
    }
    // The badge interlock: only logged-in certification surveys grant.
    survey.certification_give_badge = survey.users_login_required && survey.certification;
    // Conditional questions and login-gated token access need attempt
    // limiting (a resubmittable conditional survey is a leak).
    if has_conditional_questions
        || (survey.users_login_required
            && survey.access_mode == crate::domain::entity::SurveyAccessMode::Token)
    {
        survey.is_attempts_limited = true;
    }
    // Speed rating needs a positive full-credit window.
    if survey.session_speed_rating && survey.session_speed_rating_time_limit.map_or(true, |t| t <= 0) {
        // Refuse at clamp time is silent-fix territory — the CHECK
        // constraint is the loud backstop; the clamp clears the flag
        // rather than inventing a limit the officer never chose.
        survey.session_speed_rating = false;
    }
}

/// The question-level clamp cluster (dual-nature + validation families).
pub fn apply_question_clamps(question: &mut crate::domain::entity::Question) {
    // Pages carry no type; typed rows are not pages.
    if question.is_page {
        question.question_type = None;
    }
    // Comments and placeholders are meaningless on choice/matrix.
    if matches!(
        question.question_type,
        Some(crate::domain::entity::SurveyQuestionType::SimpleChoice)
            | Some(crate::domain::entity::SurveyQuestionType::MultipleChoice)
            | Some(crate::domain::entity::SurveyQuestionType::Matrix)
    ) {
        question.question_placeholder = None;
    }
    // Page backgrounds are page-only.
    if !question.is_page {
        question.background_image = None;
    }
    // Validation only applies to the free-input families.
    let validatable = matches!(
        question.question_type,
        Some(crate::domain::entity::SurveyQuestionType::CharBox)
            | Some(crate::domain::entity::SurveyQuestionType::TextBox)
            | Some(crate::domain::entity::SurveyQuestionType::NumericalBox)
            | Some(crate::domain::entity::SurveyQuestionType::Date)
            | Some(crate::domain::entity::SurveyQuestionType::Datetime)
    );
    if !validatable {
        question.validation_required = false;
    }
    // A time limit must be positive when limited.
    if question.is_time_limited && question.time_limit.map_or(true, |t| t <= 0) {
        question.is_time_limited = false;
        question.time_limit = None;
    }
}

/// Recompute `page_id` — "last page before this question in sequence
/// order" (SVM-8: write-maintained derivation, no ORM recompute to
/// drift). Called by the question upsert/reorder verbs over the full
/// ordered set.
pub fn recompute_page_ids(
    questions: &mut [crate::domain::entity::Question],
) -> Vec<(Uuid, Option<Uuid>)> {
    let mut last_page: Option<Uuid> = None;
    let mut out = Vec::with_capacity(questions.len());
    for q in questions.iter_mut() {
        if q.is_page {
            last_page = Some(q.id);
            q.page_id = None;
        } else {
            q.page_id = last_page;
        }
        out.push((q.id, q.page_id));
    }
    out
}

// ─── the service ──────────────────────────────────────────────────────────────

/// Session-code minting knobs (SV-B12): the length ladder and the retry
/// budget per rung before declaring exhaustion.
pub const SESSION_CODE_MIN_DIGITS: usize = 4;
pub const SESSION_CODE_MAX_DIGITS: usize = 9;
const SESSION_CODE_ATTEMPTS_PER_RUNG: usize = 8;

/// Tier B hard TTL: a code is dead this long after arm, whatever the
/// session state says.
pub const SESSION_CODE_TTL: Duration = Duration::hours(24);

/// Is a session code still live? Validity is state-based (`session_state`
/// non-NULL) AND inside the hard TTL from the recorded arm instant
/// (`armed_at`, read via SQL — the operational stamp rides the row's
/// metadata jsonb beside the audit keys and is not part of the typed
/// struct). A NULL `armed_at` (pre-stamp rows) leaves the state leg as
/// the sole decider.
pub fn session_code_live(survey: &Survey, armed_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    if survey.session_state.is_none() {
        return false;
    }
    match armed_at {
        Some(armed) => now.signed_duration_since(armed) < SESSION_CODE_TTL,
        None => true,
    }
}

/// The validated survey/question/session write path.
pub struct SurveyWriteService {
    pool: sqlx::PgPool,
    sink: EventSinkSlot,
}

impl SurveyWriteService {
    pub fn new(pool: sqlx::PgPool, sink: EventSinkSlot) -> Self {
        Self { pool, sink }
    }

    pub fn sink(&self) -> &EventSinkSlot {
        &self.sink
    }

    // ── survey upsert with the full clamp cluster (SV-B3: one tx) ─────────────

    /// Apply a patch to a survey with ALL clamps and ALL propagated
    /// fields in one transaction. `patch` mutates the loaded row; the
    /// clamps then run; the persist writes every column the cluster
    /// touches. Returns the persisted row.
    pub async fn update_survey<F>(
        &self,
        survey_id: Uuid,
        patch: F,
    ) -> Result<Survey, SurveyWriteError>
    where
        F: FnOnce(&mut Survey),
    {
        let mut tx = self.pool.begin().await?;
        let mut survey = SurveySessionRepository::find_survey_by_id(&mut tx, survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(survey_id))?;

        patch(&mut survey);

        let has_conditional = sqlx::query_scalar::<_, i64>(
            r#"SELECT count(*) FROM survey.survey_question_triggering_answers t
               JOIN survey.survey_questions q ON q.id = t.question_id
               WHERE q.survey_id = $1 AND (q.metadata->>'deleted_at') IS NULL"#,
        )
        .bind(survey_id)
        .fetch_one(&mut *tx)
        .await?
            > 0;

        apply_survey_clamps(&mut survey, has_conditional);

        // ONE persist covering every clamped/propagated column.
        let survey = persist_survey(&mut tx, &survey).await?;

        // The propagation leg: speed-rating window onto the questions
        // (deliberately NOT derived live — circular with the questions'
        // own time limits).
        if survey.session_speed_rating {
            if let Some(limit) = survey.session_speed_rating_time_limit {
                sqlx::query(
                    r#"UPDATE survey.survey_questions
                       SET is_time_limited = TRUE, time_limit = $2,
                           is_time_customized = FALSE
                       WHERE survey_id = $1 AND is_page = FALSE
                         AND (metadata->>'deleted_at') IS NULL"#,
                )
                .bind(survey_id)
                .bind(limit)
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        Ok(survey)
    }

    /// Create a survey with the clamp cluster applied (the composed
    /// entry-point for hosts and probes; generated CRUD remains available
    /// for the officer tree).
    pub async fn create_survey(
        &self,
        mut survey: Survey,
    ) -> Result<Survey, SurveyWriteError> {
        if survey.access_token.is_empty() {
            survey.access_token = Uuid::new_v4().to_string();
        }
        apply_survey_clamps(&mut survey, false);
        let mut tx = self.pool.begin().await?;
        let created = persist_survey(&mut tx, &survey).await?;
        tx.commit().await?;
        Ok(created)
    }

    // ── the session verbs ─────────────────────────────────────────────────────

    /// Arm: clamp the layout, mint a unique code, set `ready`. The lazy
    /// `in_progress` open happens on the first advance.
    pub async fn arm_session(&self, survey_id: Uuid) -> Result<Survey, SurveyWriteError> {
        // Certification-bearing surveys never arm: the typed refusal
        // fires BEFORE any minting (a code minted for a survey that can
        // never run would burn a namespace slot for nothing).
        {
            let mut conn = self.pool.acquire().await?;
            let survey = SurveySessionRepository::find_survey_by_id(&mut conn, survey_id)
                .await?
                .ok_or(SurveyWriteError::SurveyNotFound(survey_id))?;
            if survey.certification {
                return Err(SurveyWriteError::SessionCertificationConflict);
            }
        }
        // The mint ladder climbs 4→9 digits on collision; exhaustion at 9
        // is the loud typed error (SV-B12).
        for digits in SESSION_CODE_MIN_DIGITS..=SESSION_CODE_MAX_DIGITS {
            for _ in 0..SESSION_CODE_ATTEMPTS_PER_RUNG {
                let candidate = SurveySessionRepository::candidate_code(digits);
                let mut tx = self.pool.begin().await?;
                // Availability pre-check (the partial UNIQUE is the
                // decider; this only avoids most retry aborts).
                let taken: bool = sqlx::query_scalar::<_, bool>(
                    r#"SELECT EXISTS(SELECT 1 FROM survey.survey_surveys WHERE session_code = $1)"#,
                )
                .bind(&candidate)
                .fetch_one(&mut *tx)
                .await?;
                if taken {
                    continue;
                }
                match SurveySessionRepository::arm_session(&mut tx, survey_id, &candidate, Utc::now())
                    .await?
                {
                    Some(survey) => {
                        tx.commit().await?;
                        return Ok(survey);
                    }
                    None => {
                        // Already armed/running — not a mint problem.
                        return Err(SurveyWriteError::SessionAlreadyArmed);
                    }
                }
            }
        }
        Err(SurveyWriteError::SessionCodeExhausted)
    }

    /// Advance the cursor one question. Row-locked (SV-B4): the winner of
    /// two concurrent advances moves the cursor exactly once, stamps the
    /// clock exactly once, pushes exactly once. The stored clock is
    /// `now + 1 s` (server-delay grace); the pushed payload carries the
    /// PRE-write instant.
    pub async fn advance_session(&self, survey_id: Uuid) -> Result<Survey, SurveyWriteError> {
        let mut tx = self.pool.begin().await?;
        let survey = SurveySessionRepository::lock_survey_for_update(&mut tx, survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(survey_id))?;
        if survey.session_state.is_none() {
            return Err(SurveyWriteError::SessionNotRunning);
        }

        let questions = SurveySessionRepository::questions_in_sequence(&mut tx, survey_id).await?;
        let next = match survey.session_question_id {
            None => questions.first().map(|q| q.id),
            Some(cursor) => questions
                .iter()
                .skip_while(|q| q.id != cursor)
                .skip(1)
                .next()
                .map(|q| q.id),
        };
        let Some(next_id) = next else {
            return Err(SurveyWriteError::NoNextQuestion);
        };

        let pre_write = Utc::now();
        // The lazy open rides the first advance.
        if survey.session_state == Some(crate::domain::entity::SurveySessionState::Ready) {
            SurveySessionRepository::open_session(&mut tx, survey_id, pre_write).await?;
            self.sink.record(&Fact::SessionStarted {
                survey_id,
                session_code: survey
                    .session_code
                    .clone()
                    .unwrap_or_default(),
            });
        }
        // Stored clock: now + 1 s (attendee-favoring skew, kept).
        let clock = pre_write + Duration::seconds(1);
        SurveySessionRepository::advance_cursor(&mut tx, survey_id, next_id, clock).await?;
        let updated = SurveySessionRepository::find_survey_by_id(&mut tx, survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(survey_id))?;
        tx.commit().await?;

        self.sink.record(&Fact::SessionAdvanced {
            survey_id,
            session_code: updated.session_code.clone().unwrap_or_default(),
            question_id: next_id,
            payload_millis: pre_write.timestamp_millis(),
        });
        Ok(updated)
    }

    /// End: forward-only bulk-done of the live attendees, NULL the state
    /// (the code dies with it), close the realtime channel. Deliberately
    /// does NOT run the certification funnel — the two regimes are kept
    /// disjoint at arm (the typed `SessionCertificationConflict` above)
    /// and by the `survey_session_certification_disjoint` CHECK, so the
    /// bypass can never skip a certification.
    pub async fn end_session(&self, survey_id: Uuid) -> Result<u64, SurveyWriteError> {
        let mut tx = self.pool.begin().await?;
        let survey = SurveySessionRepository::lock_survey_for_update(&mut tx, survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(survey_id))?;
        if survey.session_state.is_none() {
            return Err(SurveyWriteError::SessionNotRunning);
        }
        let done = SurveySessionRepository::bulk_done_attendees(&mut tx, survey_id, Utc::now()).await?;
        let ended = SurveySessionRepository::end_session(&mut tx, survey_id).await?;
        tx.commit().await?;
        if let Some(s) = ended {
            self.sink.record(&Fact::SessionEnded {
                survey_id,
                session_code: s.session_code.unwrap_or_default(),
            });
        }
        Ok(done)
    }

    // ── question upsert with the page derivation ─────────────────────────────

    /// Upsert one question then recompute `page_id` across the survey's
    /// ordered set (SVM-8) — one transaction, no drift window.
    pub async fn upsert_question(
        &self,
        mut question: crate::domain::entity::Question,
    ) -> Result<crate::domain::entity::Question, SurveyWriteError> {
        apply_question_clamps(&mut question);
        let mut tx = self.pool.begin().await?;
        SurveySessionRepository::find_survey_by_id(&mut tx, question.survey_id)
            .await?
            .ok_or(SurveyWriteError::SurveyNotFound(question.survey_id))?;
        let q = persist_question(&mut tx, &question).await?;
        let mut questions = SurveySessionRepository::questions_in_sequence(&mut tx, question.survey_id)
            .await?;
        // Pages are excluded from `questions_in_sequence` (non-page
        // filter); fetch pages too for the derivation.
        let pages = sqlx::query_as::<_, crate::domain::entity::Question>(
            r#"SELECT * FROM survey.survey_questions
               WHERE survey_id = $1 AND is_page = TRUE
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(question.survey_id)
        .fetch_all(&mut *tx)
        .await?;
        questions.extend(pages);
        questions.sort_by_key(|k| (k.sequence, k.id));
        let assignments = recompute_page_ids(&mut questions);
        for (qid, page_id) in assignments {
            sqlx::query(
                r#"UPDATE survey.survey_questions SET page_id = $2 WHERE id = $1"#,
            )
            .bind(qid)
            .bind(page_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(q)
    }
}

// ─── persist helpers (full-column writes — the SV-B3 no-partial-write rule) ───

/// Persist EVERY survey column (a clamp/propagation write must never be
/// narrowed to the patch's own columns).
async fn persist_survey(conn: &mut PgConnection, s: &Survey) -> Result<Survey, SurveyWriteError> {
    let out = sqlx::query_as::<_, Survey>(
        r#"INSERT INTO survey.survey_surveys AS t
               (id, survey_type, title, description, description_done, background_image,
                active, user_id, access_token, questions_layout, questions_selection,
                progression_mode, access_mode, users_login_required, users_can_go_back,
                is_attempts_limited, attempts_limit, is_time_limited, time_limit,
                scoring_type, scoring_success_min, certification, certification_mail_template_id,
                certification_report_layout, certification_give_badge, certification_badge_key,
                session_state, session_code, session_question_id, session_start_time,
                session_question_start_time, session_speed_rating, session_speed_rating_time_limit)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,
                   $22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33)
           ON CONFLICT (id) DO UPDATE SET
                survey_type = EXCLUDED.survey_type, title = EXCLUDED.title,
                description = EXCLUDED.description, description_done = EXCLUDED.description_done,
                background_image = EXCLUDED.background_image, active = EXCLUDED.active,
                user_id = EXCLUDED.user_id, access_token = EXCLUDED.access_token,
                questions_layout = EXCLUDED.questions_layout,
                questions_selection = EXCLUDED.questions_selection,
                progression_mode = EXCLUDED.progression_mode,
                access_mode = EXCLUDED.access_mode,
                users_login_required = EXCLUDED.users_login_required,
                users_can_go_back = EXCLUDED.users_can_go_back,
                is_attempts_limited = EXCLUDED.is_attempts_limited,
                attempts_limit = EXCLUDED.attempts_limit,
                is_time_limited = EXCLUDED.is_time_limited, time_limit = EXCLUDED.time_limit,
                scoring_type = EXCLUDED.scoring_type,
                scoring_success_min = EXCLUDED.scoring_success_min,
                certification = EXCLUDED.certification,
                certification_mail_template_id = EXCLUDED.certification_mail_template_id,
                certification_report_layout = EXCLUDED.certification_report_layout,
                certification_give_badge = EXCLUDED.certification_give_badge,
                certification_badge_key = EXCLUDED.certification_badge_key,
                session_state = EXCLUDED.session_state, session_code = EXCLUDED.session_code,
                session_question_id = EXCLUDED.session_question_id,
                session_start_time = EXCLUDED.session_start_time,
                session_question_start_time = EXCLUDED.session_question_start_time,
                session_speed_rating = EXCLUDED.session_speed_rating,
                session_speed_rating_time_limit = EXCLUDED.session_speed_rating_time_limit
           RETURNING *"#,
    )
    .bind(s.id)
    .bind(s.survey_type)
    .bind(&s.title)
    .bind(&s.description)
    .bind(&s.description_done)
    .bind(&s.background_image)
    .bind(s.active)
    .bind(s.user_id)
    .bind(&s.access_token)
    .bind(s.questions_layout)
    .bind(s.questions_selection)
    .bind(s.progression_mode)
    .bind(s.access_mode)
    .bind(s.users_login_required)
    .bind(s.users_can_go_back)
    .bind(s.is_attempts_limited)
    .bind(s.attempts_limit)
    .bind(s.is_time_limited)
    .bind(s.time_limit)
    .bind(s.scoring_type)
    .bind(s.scoring_success_min)
    .bind(s.certification)
    .bind(s.certification_mail_template_id)
    .bind(s.certification_report_layout)
    .bind(s.certification_give_badge)
    .bind(&s.certification_badge_key)
    .bind(s.session_state)
    .bind(&s.session_code)
    .bind(s.session_question_id)
    .bind(s.session_start_time)
    .bind(s.session_question_start_time)
    .bind(s.session_speed_rating)
    .bind(s.session_speed_rating_time_limit)
    .fetch_one(&mut *conn)
    .await?;
    Ok(out)
}

/// Persist every question column (same no-partial-write rule).
async fn persist_question(
    conn: &mut PgConnection,
    q: &crate::domain::entity::Question,
) -> Result<crate::domain::entity::Question, SurveyWriteError> {
    let out = sqlx::query_as::<_, crate::domain::entity::Question>(
        r#"INSERT INTO survey.survey_questions
               (id, survey_id, sequence, is_page, question_type, title, description,
                question_placeholder, background_image, random_questions_count,
                is_scored_question, answer_numerical_box, answer_date, answer_datetime,
                answer_score, save_as_email, save_as_nickname, matrix_subtype,
                scale_min, scale_max, scale_min_label, scale_mid_label, scale_max_label,
                is_time_limited, time_limit, is_time_customized, comments_allowed,
                comments_message, comment_count_as_answer, validation_required,
                validation_email, validation_length_min, validation_length_max,
                validation_min_float_value, validation_max_float_value,
                validation_min_date, validation_max_date,
                validation_min_datetime, validation_max_datetime,
                validation_error_msg, constr_error_msg, constr_mandatory, page_id)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                   $21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34,$35,$36,$37,$38,
                   $39,$40,$41,$42,$43)
           ON CONFLICT (id) DO UPDATE SET
                sequence = EXCLUDED.sequence, is_page = EXCLUDED.is_page,
                question_type = EXCLUDED.question_type, title = EXCLUDED.title,
                description = EXCLUDED.description,
                question_placeholder = EXCLUDED.question_placeholder,
                background_image = EXCLUDED.background_image,
                random_questions_count = EXCLUDED.random_questions_count,
                is_scored_question = EXCLUDED.is_scored_question,
                answer_numerical_box = EXCLUDED.answer_numerical_box,
                answer_date = EXCLUDED.answer_date, answer_datetime = EXCLUDED.answer_datetime,
                answer_score = EXCLUDED.answer_score,
                save_as_email = EXCLUDED.save_as_email,
                save_as_nickname = EXCLUDED.save_as_nickname,
                matrix_subtype = EXCLUDED.matrix_subtype,
                scale_min = EXCLUDED.scale_min, scale_max = EXCLUDED.scale_max,
                scale_min_label = EXCLUDED.scale_min_label,
                scale_mid_label = EXCLUDED.scale_mid_label,
                scale_max_label = EXCLUDED.scale_max_label,
                is_time_limited = EXCLUDED.is_time_limited, time_limit = EXCLUDED.time_limit,
                is_time_customized = EXCLUDED.is_time_customized,
                comments_allowed = EXCLUDED.comments_allowed,
                comments_message = EXCLUDED.comments_message,
                comment_count_as_answer = EXCLUDED.comment_count_as_answer,
                validation_required = EXCLUDED.validation_required,
                validation_email = EXCLUDED.validation_email,
                validation_length_min = EXCLUDED.validation_length_min,
                validation_length_max = EXCLUDED.validation_length_max,
                validation_min_float_value = EXCLUDED.validation_min_float_value,
                validation_max_float_value = EXCLUDED.validation_max_float_value,
                validation_min_date = EXCLUDED.validation_min_date,
                validation_max_date = EXCLUDED.validation_max_date,
                validation_min_datetime = EXCLUDED.validation_min_datetime,
                validation_max_datetime = EXCLUDED.validation_max_datetime,
                validation_error_msg = EXCLUDED.validation_error_msg,
                constr_error_msg = EXCLUDED.constr_error_msg,
                constr_mandatory = EXCLUDED.constr_mandatory, page_id = EXCLUDED.page_id
           RETURNING *"#,
    )
    .bind(q.id)
    .bind(q.survey_id)
    .bind(q.sequence)
    .bind(q.is_page)
    .bind(q.question_type)
    .bind(&q.title)
    .bind(&q.description)
    .bind(&q.question_placeholder)
    .bind(&q.background_image)
    .bind(q.random_questions_count)
    .bind(q.is_scored_question)
    .bind(q.answer_numerical_box)
    .bind(q.answer_date)
    .bind(q.answer_datetime)
    .bind(q.answer_score)
    .bind(q.save_as_email)
    .bind(q.save_as_nickname)
    .bind(q.matrix_subtype)
    .bind(q.scale_min)
    .bind(q.scale_max)
    .bind(&q.scale_min_label)
    .bind(&q.scale_mid_label)
    .bind(&q.scale_max_label)
    .bind(q.is_time_limited)
    .bind(q.time_limit)
    .bind(q.is_time_customized)
    .bind(q.comments_allowed)
    .bind(&q.comments_message)
    .bind(q.comment_count_as_answer)
    .bind(q.validation_required)
    .bind(q.validation_email)
    .bind(q.validation_length_min)
    .bind(q.validation_length_max)
    .bind(q.validation_min_float_value)
    .bind(q.validation_max_float_value)
    .bind(q.validation_min_date)
    .bind(q.validation_max_date)
    .bind(q.validation_min_datetime)
    .bind(q.validation_max_datetime)
    .bind(&q.validation_error_msg)
    .bind(&q.constr_error_msg)
    .bind(q.constr_mandatory)
    .bind(q.page_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(out)
}

// ─── Survey helper the clamp/TTL legs need ────────────────────────────────────

/// Read the operational `session_armed_at` stamp off the survey row's
/// metadata jsonb (written by the arm verb; beside — never colliding
/// with — the audit keys).
pub async fn session_armed_at(
    conn: &mut PgConnection,
    survey_id: Uuid,
) -> Result<Option<DateTime<Utc>>, SurveyWriteError> {
    let stamp: Option<String> = sqlx::query_scalar(
        r#"SELECT NULLIF(metadata->>'session_armed_at', '')
           FROM survey.survey_surveys WHERE id = $1"#,
    )
    .bind(survey_id)
    .fetch_optional(conn)
    .await?
    .flatten();
    Ok(stamp.and_then(|s| DateTime::parse_from_rfc3339(&s).ok().map(|t| t.with_timezone(&Utc))))
}

impl SurveyWriteService {
    /// The session preview read: the join-screen view of a live session
    /// code (title, description, live flag). NO writes, NO Tier B
    /// bookkeeping — a preview must not consume lockout counters.
    pub async fn survey_by_session_code(
        &self,
        code: &str,
    ) -> Result<Option<serde_json::Value>, SurveyWriteError> {
        let mut tx = self.pool.begin().await?;
        let survey = SurveySessionRepository::find_survey_by_session_code(&mut tx, code).await?;
        let Some(survey) = survey else {
            tx.commit().await?;
            return Ok(None);
        };
        let armed_at = session_armed_at(&mut tx, survey.id).await?;
        let live = session_code_live(&survey, armed_at, Utc::now());
        tx.commit().await?;
        Ok(Some(serde_json::json!({
            "survey_id": survey.id,
            "title": survey.title,
            "description": survey.description,
            "session_state": survey.session_state.map(|s| s.to_string()),
            "live": live,
        })))
    }
}
