use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::SurveyAnswerType;
use super::AuditMetadata;

/// Strongly-typed ID for UserInputLine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserInputLineId(pub Uuid);

impl UserInputLineId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for UserInputLineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for UserInputLineId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for UserInputLineId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<UserInputLineId> for Uuid {
    fn from(id: UserInputLineId) -> Self { id.0 }
}

impl AsRef<Uuid> for UserInputLineId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for UserInputLineId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserInputLine {
    pub id: Uuid,
    pub user_input_id: Uuid,
    pub survey_id: Uuid,
    pub question_id: Uuid,
    pub suggested_answer_id: Option<Uuid>,
    pub matrix_row_id: Option<Uuid>,
    pub skipped: bool,
    pub answer_type: Option<SurveyAnswerType>,
    pub value_char_box: Option<String>,
    pub value_text_box: Option<String>,
    pub value_numerical_box: Option<f64>,
    pub value_scale: Option<i32>,
    pub value_date: Option<NaiveDate>,
    pub value_datetime: Option<DateTime<Utc>>,
    pub answer_score: Option<f64>,
    pub answer_is_correct: Option<bool>,
    pub speed_seconds: Option<i32>,
    pub answered_at: DateTime<Utc>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl UserInputLine {
    /// Create a builder for UserInputLine
    pub fn builder() -> UserInputLineBuilder {
        <UserInputLineBuilder as Default>::default()
    }

    /// Create a new UserInputLine with required fields
    pub fn new(user_input_id: Uuid, survey_id: Uuid, question_id: Uuid, skipped: bool, answered_at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_input_id,
            survey_id,
            question_id,
            suggested_answer_id: None,
            matrix_row_id: None,
            skipped,
            answer_type: None,
            value_char_box: None,
            value_text_box: None,
            value_numerical_box: None,
            value_scale: None,
            value_date: None,
            value_datetime: None,
            answer_score: None,
            answer_is_correct: None,
            speed_seconds: None,
            answered_at,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> UserInputLineId {
        UserInputLineId(self.id)
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

    /// Set the suggested_answer_id field (chainable)
    pub fn with_suggested_answer_id(mut self, value: Uuid) -> Self {
        self.suggested_answer_id = Some(value);
        self
    }

    /// Set the matrix_row_id field (chainable)
    pub fn with_matrix_row_id(mut self, value: Uuid) -> Self {
        self.matrix_row_id = Some(value);
        self
    }

    /// Set the answer_type field (chainable)
    pub fn with_answer_type(mut self, value: SurveyAnswerType) -> Self {
        self.answer_type = Some(value);
        self
    }

    /// Set the value_char_box field (chainable)
    pub fn with_value_char_box(mut self, value: String) -> Self {
        self.value_char_box = Some(value);
        self
    }

    /// Set the value_text_box field (chainable)
    pub fn with_value_text_box(mut self, value: String) -> Self {
        self.value_text_box = Some(value);
        self
    }

    /// Set the value_numerical_box field (chainable)
    pub fn with_value_numerical_box(mut self, value: f64) -> Self {
        self.value_numerical_box = Some(value);
        self
    }

    /// Set the value_scale field (chainable)
    pub fn with_value_scale(mut self, value: i32) -> Self {
        self.value_scale = Some(value);
        self
    }

    /// Set the value_date field (chainable)
    pub fn with_value_date(mut self, value: NaiveDate) -> Self {
        self.value_date = Some(value);
        self
    }

    /// Set the value_datetime field (chainable)
    pub fn with_value_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.value_datetime = Some(value);
        self
    }

    /// Set the answer_score field (chainable)
    pub fn with_answer_score(mut self, value: f64) -> Self {
        self.answer_score = Some(value);
        self
    }

    /// Set the answer_is_correct field (chainable)
    pub fn with_answer_is_correct(mut self, value: bool) -> Self {
        self.answer_is_correct = Some(value);
        self
    }

    /// Set the speed_seconds field (chainable)
    pub fn with_speed_seconds(mut self, value: i32) -> Self {
        self.speed_seconds = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "user_input_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_input_id = v; }
                }
                "survey_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.survey_id = v; }
                }
                "question_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.question_id = v; }
                }
                "suggested_answer_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.suggested_answer_id = v; }
                }
                "matrix_row_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.matrix_row_id = v; }
                }
                "skipped" => {
                    if let Ok(v) = serde_json::from_value(value) { self.skipped = v; }
                }
                "answer_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answer_type = v; }
                }
                "value_char_box" => {
                    if let Ok(v) = serde_json::from_value(value) { self.value_char_box = v; }
                }
                "value_text_box" => {
                    if let Ok(v) = serde_json::from_value(value) { self.value_text_box = v; }
                }
                "value_numerical_box" => {
                    if let Ok(v) = serde_json::from_value(value) { self.value_numerical_box = v; }
                }
                "value_scale" => {
                    if let Ok(v) = serde_json::from_value(value) { self.value_scale = v; }
                }
                "value_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.value_date = v; }
                }
                "value_datetime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.value_datetime = v; }
                }
                "answer_score" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answer_score = v; }
                }
                "answer_is_correct" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answer_is_correct = v; }
                }
                "speed_seconds" => {
                    if let Ok(v) = serde_json::from_value(value) { self.speed_seconds = v; }
                }
                "answered_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answered_at = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for UserInputLine {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "UserInputLine"
    }
}

impl backbone_core::PersistentEntity for UserInputLine {
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

impl backbone_orm::EntityRepoMeta for UserInputLine {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("user_input_id".to_string(), "uuid".to_string());
        m.insert("survey_id".to_string(), "uuid".to_string());
        m.insert("question_id".to_string(), "uuid".to_string());
        m.insert("suggested_answer_id".to_string(), "uuid".to_string());
        m.insert("matrix_row_id".to_string(), "uuid".to_string());
        m.insert("answer_type".to_string(), "survey_answer_type".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("userInput", "survey_user_inputs", "userInputId"), ("question", "survey_questions", "questionId")]
    }
}

/// Builder for UserInputLine entity
///
/// Provides a fluent API for constructing UserInputLine instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct UserInputLineBuilder {
    user_input_id: Option<Uuid>,
    survey_id: Option<Uuid>,
    question_id: Option<Uuid>,
    suggested_answer_id: Option<Uuid>,
    matrix_row_id: Option<Uuid>,
    skipped: Option<bool>,
    answer_type: Option<SurveyAnswerType>,
    value_char_box: Option<String>,
    value_text_box: Option<String>,
    value_numerical_box: Option<f64>,
    value_scale: Option<i32>,
    value_date: Option<NaiveDate>,
    value_datetime: Option<DateTime<Utc>>,
    answer_score: Option<f64>,
    answer_is_correct: Option<bool>,
    speed_seconds: Option<i32>,
    answered_at: Option<DateTime<Utc>>,
}

impl UserInputLineBuilder {
    /// Set the user_input_id field (required)
    pub fn user_input_id(mut self, value: Uuid) -> Self {
        self.user_input_id = Some(value);
        self
    }

    /// Set the survey_id field (required)
    pub fn survey_id(mut self, value: Uuid) -> Self {
        self.survey_id = Some(value);
        self
    }

    /// Set the question_id field (required)
    pub fn question_id(mut self, value: Uuid) -> Self {
        self.question_id = Some(value);
        self
    }

    /// Set the suggested_answer_id field (optional)
    pub fn suggested_answer_id(mut self, value: Uuid) -> Self {
        self.suggested_answer_id = Some(value);
        self
    }

    /// Set the matrix_row_id field (optional)
    pub fn matrix_row_id(mut self, value: Uuid) -> Self {
        self.matrix_row_id = Some(value);
        self
    }

    /// Set the skipped field (default: `false`)
    pub fn skipped(mut self, value: bool) -> Self {
        self.skipped = Some(value);
        self
    }

    /// Set the answer_type field (optional)
    pub fn answer_type(mut self, value: SurveyAnswerType) -> Self {
        self.answer_type = Some(value);
        self
    }

    /// Set the value_char_box field (optional)
    pub fn value_char_box(mut self, value: String) -> Self {
        self.value_char_box = Some(value);
        self
    }

    /// Set the value_text_box field (optional)
    pub fn value_text_box(mut self, value: String) -> Self {
        self.value_text_box = Some(value);
        self
    }

    /// Set the value_numerical_box field (optional)
    pub fn value_numerical_box(mut self, value: f64) -> Self {
        self.value_numerical_box = Some(value);
        self
    }

    /// Set the value_scale field (optional)
    pub fn value_scale(mut self, value: i32) -> Self {
        self.value_scale = Some(value);
        self
    }

    /// Set the value_date field (optional)
    pub fn value_date(mut self, value: NaiveDate) -> Self {
        self.value_date = Some(value);
        self
    }

    /// Set the value_datetime field (optional)
    pub fn value_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.value_datetime = Some(value);
        self
    }

    /// Set the answer_score field (optional)
    pub fn answer_score(mut self, value: f64) -> Self {
        self.answer_score = Some(value);
        self
    }

    /// Set the answer_is_correct field (optional)
    pub fn answer_is_correct(mut self, value: bool) -> Self {
        self.answer_is_correct = Some(value);
        self
    }

    /// Set the speed_seconds field (optional)
    pub fn speed_seconds(mut self, value: i32) -> Self {
        self.speed_seconds = Some(value);
        self
    }

    /// Set the answered_at field (default: `Utc::now()`)
    pub fn answered_at(mut self, value: DateTime<Utc>) -> Self {
        self.answered_at = Some(value);
        self
    }

    /// Build the UserInputLine entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<UserInputLine, String> {
        let user_input_id = self.user_input_id.ok_or_else(|| "user_input_id is required".to_string())?;
        let survey_id = self.survey_id.ok_or_else(|| "survey_id is required".to_string())?;
        let question_id = self.question_id.ok_or_else(|| "question_id is required".to_string())?;

        Ok(UserInputLine {
            id: Uuid::new_v4(),
            user_input_id,
            survey_id,
            question_id,
            suggested_answer_id: self.suggested_answer_id,
            matrix_row_id: self.matrix_row_id,
            skipped: self.skipped.unwrap_or(false),
            answer_type: self.answer_type,
            value_char_box: self.value_char_box,
            value_text_box: self.value_text_box,
            value_numerical_box: self.value_numerical_box,
            value_scale: self.value_scale,
            value_date: self.value_date,
            value_datetime: self.value_datetime,
            answer_score: self.answer_score,
            answer_is_correct: self.answer_is_correct,
            speed_seconds: self.speed_seconds,
            answered_at: self.answered_at.unwrap_or(Utc::now()),
            metadata: AuditMetadata::default(),
        })
    }
}
