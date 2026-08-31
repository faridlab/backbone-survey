use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for QuestionAnswer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QuestionAnswerId(pub Uuid);

impl QuestionAnswerId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for QuestionAnswerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for QuestionAnswerId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for QuestionAnswerId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<QuestionAnswerId> for Uuid {
    fn from(id: QuestionAnswerId) -> Self { id.0 }
}

impl AsRef<Uuid> for QuestionAnswerId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for QuestionAnswerId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct QuestionAnswer {
    pub id: Uuid,
    pub question_id: Option<Uuid>,
    pub matrix_question_id: Option<Uuid>,
    pub sequence: i32,
    pub value: Option<String>,
    pub value_image: Option<String>,
    pub value_image_filename: Option<String>,
    pub is_correct: bool,
    pub answer_score: f64,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl QuestionAnswer {
    /// Create a builder for QuestionAnswer
    pub fn builder() -> QuestionAnswerBuilder {
        <QuestionAnswerBuilder as Default>::default()
    }

    /// Create a new QuestionAnswer with required fields
    pub fn new(sequence: i32, is_correct: bool, answer_score: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            question_id: None,
            matrix_question_id: None,
            sequence,
            value: None,
            value_image: None,
            value_image_filename: None,
            is_correct,
            answer_score,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> QuestionAnswerId {
        QuestionAnswerId(self.id)
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

    /// Set the question_id field (chainable)
    pub fn with_question_id(mut self, value: Uuid) -> Self {
        self.question_id = Some(value);
        self
    }

    /// Set the matrix_question_id field (chainable)
    pub fn with_matrix_question_id(mut self, value: Uuid) -> Self {
        self.matrix_question_id = Some(value);
        self
    }

    /// Set the value field (chainable)
    pub fn with_value(mut self, value: String) -> Self {
        self.value = Some(value);
        self
    }

    /// Set the value_image field (chainable)
    pub fn with_value_image(mut self, value: String) -> Self {
        self.value_image = Some(value);
        self
    }

    /// Set the value_image_filename field (chainable)
    pub fn with_value_image_filename(mut self, value: String) -> Self {
        self.value_image_filename = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "question_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.question_id = v; }
                }
                "matrix_question_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.matrix_question_id = v; }
                }
                "sequence" => {
                    if let Ok(v) = serde_json::from_value(value) { self.sequence = v; }
                }
                "value" => {
                    if let Ok(v) = serde_json::from_value(value) { self.value = v; }
                }
                "value_image" => {
                    if let Ok(v) = serde_json::from_value(value) { self.value_image = v; }
                }
                "value_image_filename" => {
                    if let Ok(v) = serde_json::from_value(value) { self.value_image_filename = v; }
                }
                "is_correct" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_correct = v; }
                }
                "answer_score" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answer_score = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for QuestionAnswer {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "QuestionAnswer"
    }
}

impl backbone_core::PersistentEntity for QuestionAnswer {
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

impl backbone_orm::EntityRepoMeta for QuestionAnswer {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("question_id".to_string(), "uuid".to_string());
        m.insert("matrix_question_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("question", "survey_questions", "questionId"), ("matrixQuestion", "survey_questions", "matrixQuestionId")]
    }
}

/// Builder for QuestionAnswer entity
///
/// Provides a fluent API for constructing QuestionAnswer instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct QuestionAnswerBuilder {
    question_id: Option<Uuid>,
    matrix_question_id: Option<Uuid>,
    sequence: Option<i32>,
    value: Option<String>,
    value_image: Option<String>,
    value_image_filename: Option<String>,
    is_correct: Option<bool>,
    answer_score: Option<f64>,
}

impl QuestionAnswerBuilder {
    /// Set the question_id field (optional)
    pub fn question_id(mut self, value: Uuid) -> Self {
        self.question_id = Some(value);
        self
    }

    /// Set the matrix_question_id field (optional)
    pub fn matrix_question_id(mut self, value: Uuid) -> Self {
        self.matrix_question_id = Some(value);
        self
    }

    /// Set the sequence field (default: `10`)
    pub fn sequence(mut self, value: i32) -> Self {
        self.sequence = Some(value);
        self
    }

    /// Set the value field (optional)
    pub fn value(mut self, value: String) -> Self {
        self.value = Some(value);
        self
    }

    /// Set the value_image field (optional)
    pub fn value_image(mut self, value: String) -> Self {
        self.value_image = Some(value);
        self
    }

    /// Set the value_image_filename field (optional)
    pub fn value_image_filename(mut self, value: String) -> Self {
        self.value_image_filename = Some(value);
        self
    }

    /// Set the is_correct field (default: `false`)
    pub fn is_correct(mut self, value: bool) -> Self {
        self.is_correct = Some(value);
        self
    }

    /// Set the answer_score field (default: `0_f64`)
    pub fn answer_score(mut self, value: f64) -> Self {
        self.answer_score = Some(value);
        self
    }

    /// Build the QuestionAnswer entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<QuestionAnswer, String> {

        Ok(QuestionAnswer {
            id: Uuid::new_v4(),
            question_id: self.question_id,
            matrix_question_id: self.matrix_question_id,
            sequence: self.sequence.unwrap_or(10),
            value: self.value,
            value_image: self.value_image,
            value_image_filename: self.value_image_filename,
            is_correct: self.is_correct.unwrap_or(false),
            answer_score: self.answer_score.unwrap_or(0_f64),
            metadata: AuditMetadata::default(),
        })
    }
}
