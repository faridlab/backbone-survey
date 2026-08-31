use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::SurveySurveyType;
use super::SurveyQuestionsLayout;
use super::SurveyQuestionsSelection;
use super::SurveyProgressionMode;
use super::SurveyAccessMode;
use super::SurveyScoringType;
use super::SurveyReportLayout;
use super::SurveySessionState;
use super::AuditMetadata;

/// Strongly-typed ID for Survey
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurveyId(pub Uuid);

impl SurveyId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for SurveyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SurveyId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for SurveyId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<SurveyId> for Uuid {
    fn from(id: SurveyId) -> Self { id.0 }
}

impl AsRef<Uuid> for SurveyId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for SurveyId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Survey {
    pub id: Uuid,
    pub survey_type: SurveySurveyType,
    pub title: String,
    pub description: Option<String>,
    pub description_done: Option<String>,
    pub background_image: Option<String>,
    pub active: bool,
    pub user_id: Option<Uuid>,
    pub access_token: String,
    pub questions_layout: SurveyQuestionsLayout,
    pub questions_selection: SurveyQuestionsSelection,
    pub progression_mode: SurveyProgressionMode,
    pub access_mode: SurveyAccessMode,
    pub users_login_required: bool,
    pub users_can_go_back: bool,
    pub is_attempts_limited: bool,
    pub attempts_limit: i32,
    pub is_time_limited: bool,
    pub time_limit: f64,
    pub scoring_type: SurveyScoringType,
    pub scoring_success_min: f64,
    pub certification: bool,
    pub certification_mail_template_id: Option<Uuid>,
    pub certification_report_layout: SurveyReportLayout,
    pub certification_give_badge: bool,
    pub certification_badge_key: Option<String>,
    pub session_state: Option<SurveySessionState>,
    pub session_code: Option<String>,
    pub session_question_id: Option<Uuid>,
    pub session_start_time: Option<DateTime<Utc>>,
    pub session_question_start_time: Option<DateTime<Utc>>,
    pub session_speed_rating: bool,
    pub session_speed_rating_time_limit: Option<i32>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Survey {
    /// Create a builder for Survey
    pub fn builder() -> SurveyBuilder {
        <SurveyBuilder as Default>::default()
    }

    /// Create a new Survey with required fields
    pub fn new(survey_type: SurveySurveyType, title: String, active: bool, access_token: String, questions_layout: SurveyQuestionsLayout, questions_selection: SurveyQuestionsSelection, progression_mode: SurveyProgressionMode, access_mode: SurveyAccessMode, users_login_required: bool, users_can_go_back: bool, is_attempts_limited: bool, attempts_limit: i32, is_time_limited: bool, time_limit: f64, scoring_type: SurveyScoringType, scoring_success_min: f64, certification: bool, certification_report_layout: SurveyReportLayout, certification_give_badge: bool, session_speed_rating: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            survey_type,
            title,
            description: None,
            description_done: None,
            background_image: None,
            active,
            user_id: None,
            access_token,
            questions_layout,
            questions_selection,
            progression_mode,
            access_mode,
            users_login_required,
            users_can_go_back,
            is_attempts_limited,
            attempts_limit,
            is_time_limited,
            time_limit,
            scoring_type,
            scoring_success_min,
            certification,
            certification_mail_template_id: None,
            certification_report_layout,
            certification_give_badge,
            certification_badge_key: None,
            session_state: None,
            session_code: None,
            session_question_id: None,
            session_start_time: None,
            session_question_start_time: None,
            session_speed_rating,
            session_speed_rating_time_limit: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> SurveyId {
        SurveyId(self.id)
    }

    /// Get when this entity was created
    pub fn created_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.created_at.as_ref()
    }

    /// Get when this entity was last updated
    pub fn updated_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.updated_at.as_ref()
    }

    /// Check if this entity is soft deleted
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted_at.is_some()
    }

    /// Check if this entity is active (not deleted)
    pub fn is_active(&self) -> bool {
        self.metadata.deleted_at.is_none()
    }

    /// Get when this entity was deleted
    pub fn deleted_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.deleted_at.as_ref()
    }

    /// Get who created this entity
    pub fn created_by(&self) -> Option<&Uuid> {
        self.metadata.created_by.as_ref()
    }

    /// Get who last updated this entity
    pub fn updated_by(&self) -> Option<&Uuid> {
        self.metadata.updated_by.as_ref()
    }

    /// Get who deleted this entity
    pub fn deleted_by(&self) -> Option<&Uuid> {
        self.metadata.deleted_by.as_ref()
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the description field (chainable)
    pub fn with_description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the description_done field (chainable)
    pub fn with_description_done(mut self, value: String) -> Self {
        self.description_done = Some(value);
        self
    }

    /// Set the background_image field (chainable)
    pub fn with_background_image(mut self, value: String) -> Self {
        self.background_image = Some(value);
        self
    }

    /// Set the user_id field (chainable)
    pub fn with_user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the certification_mail_template_id field (chainable)
    pub fn with_certification_mail_template_id(mut self, value: Uuid) -> Self {
        self.certification_mail_template_id = Some(value);
        self
    }

    /// Set the certification_badge_key field (chainable)
    pub fn with_certification_badge_key(mut self, value: String) -> Self {
        self.certification_badge_key = Some(value);
        self
    }

    /// Set the session_state field (chainable)
    pub fn with_session_state(mut self, value: SurveySessionState) -> Self {
        self.session_state = Some(value);
        self
    }

    /// Set the session_code field (chainable)
    pub fn with_session_code(mut self, value: String) -> Self {
        self.session_code = Some(value);
        self
    }

    /// Set the session_question_id field (chainable)
    pub fn with_session_question_id(mut self, value: Uuid) -> Self {
        self.session_question_id = Some(value);
        self
    }

    /// Set the session_start_time field (chainable)
    pub fn with_session_start_time(mut self, value: DateTime<Utc>) -> Self {
        self.session_start_time = Some(value);
        self
    }

    /// Set the session_question_start_time field (chainable)
    pub fn with_session_question_start_time(mut self, value: DateTime<Utc>) -> Self {
        self.session_question_start_time = Some(value);
        self
    }

    /// Set the session_speed_rating_time_limit field (chainable)
    pub fn with_session_speed_rating_time_limit(mut self, value: i32) -> Self {
        self.session_speed_rating_time_limit = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "survey_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.survey_type = v; }
                }
                "title" => {
                    if let Ok(v) = serde_json::from_value(value) { self.title = v; }
                }
                "description" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description = v; }
                }
                "description_done" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description_done = v; }
                }
                "background_image" => {
                    if let Ok(v) = serde_json::from_value(value) { self.background_image = v; }
                }
                "active" => {
                    if let Ok(v) = serde_json::from_value(value) { self.active = v; }
                }
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                "access_token" => {
                    if let Ok(v) = serde_json::from_value(value) { self.access_token = v; }
                }
                "questions_layout" => {
                    if let Ok(v) = serde_json::from_value(value) { self.questions_layout = v; }
                }
                "questions_selection" => {
                    if let Ok(v) = serde_json::from_value(value) { self.questions_selection = v; }
                }
                "progression_mode" => {
                    if let Ok(v) = serde_json::from_value(value) { self.progression_mode = v; }
                }
                "access_mode" => {
                    if let Ok(v) = serde_json::from_value(value) { self.access_mode = v; }
                }
                "users_login_required" => {
                    if let Ok(v) = serde_json::from_value(value) { self.users_login_required = v; }
                }
                "users_can_go_back" => {
                    if let Ok(v) = serde_json::from_value(value) { self.users_can_go_back = v; }
                }
                "is_attempts_limited" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_attempts_limited = v; }
                }
                "attempts_limit" => {
                    if let Ok(v) = serde_json::from_value(value) { self.attempts_limit = v; }
                }
                "is_time_limited" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_time_limited = v; }
                }
                "time_limit" => {
                    if let Ok(v) = serde_json::from_value(value) { self.time_limit = v; }
                }
                "scoring_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scoring_type = v; }
                }
                "scoring_success_min" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scoring_success_min = v; }
                }
                "certification" => {
                    if let Ok(v) = serde_json::from_value(value) { self.certification = v; }
                }
                "certification_mail_template_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.certification_mail_template_id = v; }
                }
                "certification_report_layout" => {
                    if let Ok(v) = serde_json::from_value(value) { self.certification_report_layout = v; }
                }
                "certification_give_badge" => {
                    if let Ok(v) = serde_json::from_value(value) { self.certification_give_badge = v; }
                }
                "certification_badge_key" => {
                    if let Ok(v) = serde_json::from_value(value) { self.certification_badge_key = v; }
                }
                "session_state" => {
                    if let Ok(v) = serde_json::from_value(value) { self.session_state = v; }
                }
                "session_code" => {
                    if let Ok(v) = serde_json::from_value(value) { self.session_code = v; }
                }
                "session_question_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.session_question_id = v; }
                }
                "session_start_time" => {
                    if let Ok(v) = serde_json::from_value(value) { self.session_start_time = v; }
                }
                "session_question_start_time" => {
                    if let Ok(v) = serde_json::from_value(value) { self.session_question_start_time = v; }
                }
                "session_speed_rating" => {
                    if let Ok(v) = serde_json::from_value(value) { self.session_speed_rating = v; }
                }
                "session_speed_rating_time_limit" => {
                    if let Ok(v) = serde_json::from_value(value) { self.session_speed_rating_time_limit = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Survey {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Survey"
    }
}

impl backbone_core::PersistentEntity for Survey {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            self.id = uuid;
        }
    }
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.created_at
    }
    fn set_created_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.created_at = Some(ts);
    }
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.updated_at
    }
    fn set_updated_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.updated_at = Some(ts);
    }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.deleted_at
    }
    fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<chrono::Utc>>) {
        self.metadata.deleted_at = ts;
    }
}

impl backbone_orm::EntityRepoMeta for Survey {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m.insert("certification_mail_template_id".to_string(), "uuid".to_string());
        m.insert("session_question_id".to_string(), "uuid".to_string());
        m.insert("survey_type".to_string(), "survey_survey_type".to_string());
        m.insert("questions_layout".to_string(), "survey_questions_layout".to_string());
        m.insert("questions_selection".to_string(), "survey_questions_selection".to_string());
        m.insert("progression_mode".to_string(), "survey_progression_mode".to_string());
        m.insert("access_mode".to_string(), "survey_access_mode".to_string());
        m.insert("scoring_type".to_string(), "survey_scoring_type".to_string());
        m.insert("certification_report_layout".to_string(), "survey_report_layout".to_string());
        m.insert("session_state".to_string(), "survey_session_state".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["title", "access_token"]
    }
}

/// Builder for Survey entity
///
/// Provides a fluent API for constructing Survey instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct SurveyBuilder {
    survey_type: Option<SurveySurveyType>,
    title: Option<String>,
    description: Option<String>,
    description_done: Option<String>,
    background_image: Option<String>,
    active: Option<bool>,
    user_id: Option<Uuid>,
    access_token: Option<String>,
    questions_layout: Option<SurveyQuestionsLayout>,
    questions_selection: Option<SurveyQuestionsSelection>,
    progression_mode: Option<SurveyProgressionMode>,
    access_mode: Option<SurveyAccessMode>,
    users_login_required: Option<bool>,
    users_can_go_back: Option<bool>,
    is_attempts_limited: Option<bool>,
    attempts_limit: Option<i32>,
    is_time_limited: Option<bool>,
    time_limit: Option<f64>,
    scoring_type: Option<SurveyScoringType>,
    scoring_success_min: Option<f64>,
    certification: Option<bool>,
    certification_mail_template_id: Option<Uuid>,
    certification_report_layout: Option<SurveyReportLayout>,
    certification_give_badge: Option<bool>,
    certification_badge_key: Option<String>,
    session_state: Option<SurveySessionState>,
    session_code: Option<String>,
    session_question_id: Option<Uuid>,
    session_start_time: Option<DateTime<Utc>>,
    session_question_start_time: Option<DateTime<Utc>>,
    session_speed_rating: Option<bool>,
    session_speed_rating_time_limit: Option<i32>,
}

impl SurveyBuilder {
    /// Set the survey_type field (default: `SurveySurveyType::default()`)
    pub fn survey_type(mut self, value: SurveySurveyType) -> Self {
        self.survey_type = Some(value);
        self
    }

    /// Set the title field (required)
    pub fn title(mut self, value: String) -> Self {
        self.title = Some(value);
        self
    }

    /// Set the description field (optional)
    pub fn description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the description_done field (optional)
    pub fn description_done(mut self, value: String) -> Self {
        self.description_done = Some(value);
        self
    }

    /// Set the background_image field (optional)
    pub fn background_image(mut self, value: String) -> Self {
        self.background_image = Some(value);
        self
    }

    /// Set the active field (default: `true`)
    pub fn active(mut self, value: bool) -> Self {
        self.active = Some(value);
        self
    }

    /// Set the user_id field (optional)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the access_token field (required)
    pub fn access_token(mut self, value: String) -> Self {
        self.access_token = Some(value);
        self
    }

    /// Set the questions_layout field (default: `SurveyQuestionsLayout::default()`)
    pub fn questions_layout(mut self, value: SurveyQuestionsLayout) -> Self {
        self.questions_layout = Some(value);
        self
    }

    /// Set the questions_selection field (default: `SurveyQuestionsSelection::default()`)
    pub fn questions_selection(mut self, value: SurveyQuestionsSelection) -> Self {
        self.questions_selection = Some(value);
        self
    }

    /// Set the progression_mode field (default: `SurveyProgressionMode::default()`)
    pub fn progression_mode(mut self, value: SurveyProgressionMode) -> Self {
        self.progression_mode = Some(value);
        self
    }

    /// Set the access_mode field (default: `SurveyAccessMode::default()`)
    pub fn access_mode(mut self, value: SurveyAccessMode) -> Self {
        self.access_mode = Some(value);
        self
    }

    /// Set the users_login_required field (default: `false`)
    pub fn users_login_required(mut self, value: bool) -> Self {
        self.users_login_required = Some(value);
        self
    }

    /// Set the users_can_go_back field (default: `false`)
    pub fn users_can_go_back(mut self, value: bool) -> Self {
        self.users_can_go_back = Some(value);
        self
    }

    /// Set the is_attempts_limited field (default: `false`)
    pub fn is_attempts_limited(mut self, value: bool) -> Self {
        self.is_attempts_limited = Some(value);
        self
    }

    /// Set the attempts_limit field (default: `1`)
    pub fn attempts_limit(mut self, value: i32) -> Self {
        self.attempts_limit = Some(value);
        self
    }

    /// Set the is_time_limited field (default: `false`)
    pub fn is_time_limited(mut self, value: bool) -> Self {
        self.is_time_limited = Some(value);
        self
    }

    /// Set the time_limit field (default: `10_f64`)
    pub fn time_limit(mut self, value: f64) -> Self {
        self.time_limit = Some(value);
        self
    }

    /// Set the scoring_type field (default: `SurveyScoringType::default()`)
    pub fn scoring_type(mut self, value: SurveyScoringType) -> Self {
        self.scoring_type = Some(value);
        self
    }

    /// Set the scoring_success_min field (default: `80_f64`)
    pub fn scoring_success_min(mut self, value: f64) -> Self {
        self.scoring_success_min = Some(value);
        self
    }

    /// Set the certification field (default: `false`)
    pub fn certification(mut self, value: bool) -> Self {
        self.certification = Some(value);
        self
    }

    /// Set the certification_mail_template_id field (optional)
    pub fn certification_mail_template_id(mut self, value: Uuid) -> Self {
        self.certification_mail_template_id = Some(value);
        self
    }

    /// Set the certification_report_layout field (default: `SurveyReportLayout::default()`)
    pub fn certification_report_layout(mut self, value: SurveyReportLayout) -> Self {
        self.certification_report_layout = Some(value);
        self
    }

    /// Set the certification_give_badge field (default: `false`)
    pub fn certification_give_badge(mut self, value: bool) -> Self {
        self.certification_give_badge = Some(value);
        self
    }

    /// Set the certification_badge_key field (optional)
    pub fn certification_badge_key(mut self, value: String) -> Self {
        self.certification_badge_key = Some(value);
        self
    }

    /// Set the session_state field (optional)
    pub fn session_state(mut self, value: SurveySessionState) -> Self {
        self.session_state = Some(value);
        self
    }

    /// Set the session_code field (optional)
    pub fn session_code(mut self, value: String) -> Self {
        self.session_code = Some(value);
        self
    }

    /// Set the session_question_id field (optional)
    pub fn session_question_id(mut self, value: Uuid) -> Self {
        self.session_question_id = Some(value);
        self
    }

    /// Set the session_start_time field (optional)
    pub fn session_start_time(mut self, value: DateTime<Utc>) -> Self {
        self.session_start_time = Some(value);
        self
    }

    /// Set the session_question_start_time field (optional)
    pub fn session_question_start_time(mut self, value: DateTime<Utc>) -> Self {
        self.session_question_start_time = Some(value);
        self
    }

    /// Set the session_speed_rating field (default: `false`)
    pub fn session_speed_rating(mut self, value: bool) -> Self {
        self.session_speed_rating = Some(value);
        self
    }

    /// Set the session_speed_rating_time_limit field (optional)
    pub fn session_speed_rating_time_limit(mut self, value: i32) -> Self {
        self.session_speed_rating_time_limit = Some(value);
        self
    }

    /// Build the Survey entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Survey, String> {
        let title = self.title.ok_or_else(|| "title is required".to_string())?;
        let access_token = self.access_token.ok_or_else(|| "access_token is required".to_string())?;

        Ok(Survey {
            id: Uuid::new_v4(),
            survey_type: self.survey_type.unwrap_or_default(),
            title,
            description: self.description,
            description_done: self.description_done,
            background_image: self.background_image,
            active: self.active.unwrap_or(true),
            user_id: self.user_id,
            access_token,
            questions_layout: self.questions_layout.unwrap_or_default(),
            questions_selection: self.questions_selection.unwrap_or_default(),
            progression_mode: self.progression_mode.unwrap_or_default(),
            access_mode: self.access_mode.unwrap_or_default(),
            users_login_required: self.users_login_required.unwrap_or(false),
            users_can_go_back: self.users_can_go_back.unwrap_or(false),
            is_attempts_limited: self.is_attempts_limited.unwrap_or(false),
            attempts_limit: self.attempts_limit.unwrap_or(1),
            is_time_limited: self.is_time_limited.unwrap_or(false),
            time_limit: self.time_limit.unwrap_or(10_f64),
            scoring_type: self.scoring_type.unwrap_or_default(),
            scoring_success_min: self.scoring_success_min.unwrap_or(80_f64),
            certification: self.certification.unwrap_or(false),
            certification_mail_template_id: self.certification_mail_template_id,
            certification_report_layout: self.certification_report_layout.unwrap_or_default(),
            certification_give_badge: self.certification_give_badge.unwrap_or(false),
            certification_badge_key: self.certification_badge_key,
            session_state: self.session_state,
            session_code: self.session_code,
            session_question_id: self.session_question_id,
            session_start_time: self.session_start_time,
            session_question_start_time: self.session_question_start_time,
            session_speed_rating: self.session_speed_rating.unwrap_or(false),
            session_speed_rating_time_limit: self.session_speed_rating_time_limit,
            metadata: AuditMetadata::default(),
        })
    }
}
