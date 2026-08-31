use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::SurveyInputState;
use super::AuditMetadata;

use crate::domain::state_machine::{survey_input_stateStateMachine, survey_input_stateState, StateMachineError};

/// Strongly-typed ID for UserInput
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserInputId(pub Uuid);

impl UserInputId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for UserInputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for UserInputId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for UserInputId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<UserInputId> for Uuid {
    fn from(id: UserInputId) -> Self { id.0 }
}

impl AsRef<Uuid> for UserInputId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for UserInputId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserInput {
    pub id: Uuid,
    pub survey_id: Uuid,
    pub token_nonce: String,
    pub token_expires_at: DateTime<Utc>,
    pub invite_token: Option<String>,
    pub partner_id: Option<Uuid>,
    pub email: Option<String>,
    pub nickname: Option<String>,
    pub user_id: Option<Uuid>,
    pub wire_identity_key: Option<String>,
    pub test_entry: bool,
    pub(crate) state: SurveyInputState,
    pub start_datetime: Option<DateTime<Utc>>,
    pub end_datetime: Option<DateTime<Utc>>,
    pub deadline: Option<DateTime<Utc>>,
    pub is_session_answer: bool,
    pub last_displayed_page_id: Option<Uuid>,
    pub scoring_percentage: f64,
    pub scoring_total: f64,
    pub scoring_success: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl UserInput {
    /// Create a builder for UserInput
    pub fn builder() -> UserInputBuilder {
        <UserInputBuilder as Default>::default()
    }

    /// Create a new UserInput with required fields
    pub fn new(survey_id: Uuid, token_nonce: String, token_expires_at: DateTime<Utc>, test_entry: bool, state: SurveyInputState, is_session_answer: bool, scoring_percentage: f64, scoring_total: f64, scoring_success: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            survey_id,
            token_nonce,
            token_expires_at,
            invite_token: None,
            partner_id: None,
            email: None,
            nickname: None,
            user_id: None,
            wire_identity_key: None,
            test_entry,
            state,
            start_datetime: None,
            end_datetime: None,
            deadline: None,
            is_session_answer,
            last_displayed_page_id: None,
            scoring_percentage,
            scoring_total,
            scoring_success,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> UserInputId {
        UserInputId(self.id)
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

    /// Set the invite_token field (chainable)
    pub fn with_invite_token(mut self, value: String) -> Self {
        self.invite_token = Some(value);
        self
    }

    /// Set the partner_id field (chainable)
    pub fn with_partner_id(mut self, value: Uuid) -> Self {
        self.partner_id = Some(value);
        self
    }

    /// Set the email field (chainable)
    pub fn with_email(mut self, value: String) -> Self {
        self.email = Some(value);
        self
    }

    /// Set the nickname field (chainable)
    pub fn with_nickname(mut self, value: String) -> Self {
        self.nickname = Some(value);
        self
    }

    /// Set the user_id field (chainable)
    pub fn with_user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the wire_identity_key field (chainable)
    pub fn with_wire_identity_key(mut self, value: String) -> Self {
        self.wire_identity_key = Some(value);
        self
    }

    /// Set the start_datetime field (chainable)
    pub fn with_start_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.start_datetime = Some(value);
        self
    }

    /// Set the end_datetime field (chainable)
    pub fn with_end_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.end_datetime = Some(value);
        self
    }

    /// Set the deadline field (chainable)
    pub fn with_deadline(mut self, value: DateTime<Utc>) -> Self {
        self.deadline = Some(value);
        self
    }

    /// Set the last_displayed_page_id field (chainable)
    pub fn with_last_displayed_page_id(mut self, value: Uuid) -> Self {
        self.last_displayed_page_id = Some(value);
        self
    }

    // ==========================================================
    // State Machine
    // ==========================================================

    /// Transition to a new state via the state state machine.
    ///
    /// Returns `Err` if the transition is not permitted from the current state.
    /// Use this method instead of assigning `self.state` directly.
    pub fn transition_to(&mut self, new_state: survey_input_stateState) -> Result<(), StateMachineError> {
        let current = self.state.to_string().parse::<survey_input_stateState>()?;
        let mut sm = survey_input_stateStateMachine::from_state(current);
        sm.transition_to_state(new_state)?;
        self.state = new_state.to_string().parse::<SurveyInputState>()
            .map_err(|e| StateMachineError::InvalidState(e.to_string()))?;
        Ok(())
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "survey_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.survey_id = v; }
                }
                "token_nonce" => {
                    if let Ok(v) = serde_json::from_value(value) { self.token_nonce = v; }
                }
                "token_expires_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.token_expires_at = v; }
                }
                "invite_token" => {
                    if let Ok(v) = serde_json::from_value(value) { self.invite_token = v; }
                }
                "partner_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.partner_id = v; }
                }
                "email" => {
                    if let Ok(v) = serde_json::from_value(value) { self.email = v; }
                }
                "nickname" => {
                    if let Ok(v) = serde_json::from_value(value) { self.nickname = v; }
                }
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                "wire_identity_key" => {
                    if let Ok(v) = serde_json::from_value(value) { self.wire_identity_key = v; }
                }
                "test_entry" => {
                    if let Ok(v) = serde_json::from_value(value) { self.test_entry = v; }
                }
                "start_datetime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.start_datetime = v; }
                }
                "end_datetime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.end_datetime = v; }
                }
                "deadline" => {
                    if let Ok(v) = serde_json::from_value(value) { self.deadline = v; }
                }
                "is_session_answer" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_session_answer = v; }
                }
                "last_displayed_page_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_displayed_page_id = v; }
                }
                "scoring_percentage" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scoring_percentage = v; }
                }
                "scoring_total" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scoring_total = v; }
                }
                "scoring_success" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scoring_success = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for UserInput {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "UserInput"
    }
}

impl backbone_core::PersistentEntity for UserInput {
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

impl backbone_orm::EntityRepoMeta for UserInput {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("survey_id".to_string(), "uuid".to_string());
        m.insert("partner_id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m.insert("last_displayed_page_id".to_string(), "uuid".to_string());
        m.insert("state".to_string(), "survey_input_state".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["token_nonce"]
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("survey", "survey_surveys", "surveyId")]
    }
}

/// Builder for UserInput entity
///
/// Provides a fluent API for constructing UserInput instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct UserInputBuilder {
    survey_id: Option<Uuid>,
    token_nonce: Option<String>,
    token_expires_at: Option<DateTime<Utc>>,
    invite_token: Option<String>,
    partner_id: Option<Uuid>,
    email: Option<String>,
    nickname: Option<String>,
    user_id: Option<Uuid>,
    wire_identity_key: Option<String>,
    test_entry: Option<bool>,
    state: Option<SurveyInputState>,
    start_datetime: Option<DateTime<Utc>>,
    end_datetime: Option<DateTime<Utc>>,
    deadline: Option<DateTime<Utc>>,
    is_session_answer: Option<bool>,
    last_displayed_page_id: Option<Uuid>,
    scoring_percentage: Option<f64>,
    scoring_total: Option<f64>,
    scoring_success: Option<bool>,
}

impl UserInputBuilder {
    /// Set the survey_id field (required)
    pub fn survey_id(mut self, value: Uuid) -> Self {
        self.survey_id = Some(value);
        self
    }

    /// Set the token_nonce field (required)
    pub fn token_nonce(mut self, value: String) -> Self {
        self.token_nonce = Some(value);
        self
    }

    /// Set the token_expires_at field (required)
    pub fn token_expires_at(mut self, value: DateTime<Utc>) -> Self {
        self.token_expires_at = Some(value);
        self
    }

    /// Set the invite_token field (optional)
    pub fn invite_token(mut self, value: String) -> Self {
        self.invite_token = Some(value);
        self
    }

    /// Set the partner_id field (optional)
    pub fn partner_id(mut self, value: Uuid) -> Self {
        self.partner_id = Some(value);
        self
    }

    /// Set the email field (optional)
    pub fn email(mut self, value: String) -> Self {
        self.email = Some(value);
        self
    }

    /// Set the nickname field (optional)
    pub fn nickname(mut self, value: String) -> Self {
        self.nickname = Some(value);
        self
    }

    /// Set the user_id field (optional)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the wire_identity_key field (optional)
    pub fn wire_identity_key(mut self, value: String) -> Self {
        self.wire_identity_key = Some(value);
        self
    }

    /// Set the test_entry field (default: `false`)
    pub fn test_entry(mut self, value: bool) -> Self {
        self.test_entry = Some(value);
        self
    }

    /// Set the state field (default: `SurveyInputState::default()`)
    pub fn state(mut self, value: SurveyInputState) -> Self {
        self.state = Some(value);
        self
    }

    /// Set the start_datetime field (optional)
    pub fn start_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.start_datetime = Some(value);
        self
    }

    /// Set the end_datetime field (optional)
    pub fn end_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.end_datetime = Some(value);
        self
    }

    /// Set the deadline field (optional)
    pub fn deadline(mut self, value: DateTime<Utc>) -> Self {
        self.deadline = Some(value);
        self
    }

    /// Set the is_session_answer field (default: `false`)
    pub fn is_session_answer(mut self, value: bool) -> Self {
        self.is_session_answer = Some(value);
        self
    }

    /// Set the last_displayed_page_id field (optional)
    pub fn last_displayed_page_id(mut self, value: Uuid) -> Self {
        self.last_displayed_page_id = Some(value);
        self
    }

    /// Set the scoring_percentage field (default: `0_f64`)
    pub fn scoring_percentage(mut self, value: f64) -> Self {
        self.scoring_percentage = Some(value);
        self
    }

    /// Set the scoring_total field (default: `0_f64`)
    pub fn scoring_total(mut self, value: f64) -> Self {
        self.scoring_total = Some(value);
        self
    }

    /// Set the scoring_success field (default: `false`)
    pub fn scoring_success(mut self, value: bool) -> Self {
        self.scoring_success = Some(value);
        self
    }

    /// Build the UserInput entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<UserInput, String> {
        let survey_id = self.survey_id.ok_or_else(|| "survey_id is required".to_string())?;
        let token_nonce = self.token_nonce.ok_or_else(|| "token_nonce is required".to_string())?;
        let token_expires_at = self.token_expires_at.ok_or_else(|| "token_expires_at is required".to_string())?;

        Ok(UserInput {
            id: Uuid::new_v4(),
            survey_id,
            token_nonce,
            token_expires_at,
            invite_token: self.invite_token,
            partner_id: self.partner_id,
            email: self.email,
            nickname: self.nickname,
            user_id: self.user_id,
            wire_identity_key: self.wire_identity_key,
            test_entry: self.test_entry.unwrap_or(false),
            state: self.state.unwrap_or_default(),
            start_datetime: self.start_datetime,
            end_datetime: self.end_datetime,
            deadline: self.deadline,
            is_session_answer: self.is_session_answer.unwrap_or(false),
            last_displayed_page_id: self.last_displayed_page_id,
            scoring_percentage: self.scoring_percentage.unwrap_or(0_f64),
            scoring_total: self.scoring_total.unwrap_or(0_f64),
            scoring_success: self.scoring_success.unwrap_or(false),
            metadata: AuditMetadata::default(),
        })
    }
}
