use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for SurveyUserInputPredefinedQuestion
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurveyUserInputPredefinedQuestionId(pub Uuid);

impl SurveyUserInputPredefinedQuestionId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for SurveyUserInputPredefinedQuestionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SurveyUserInputPredefinedQuestionId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for SurveyUserInputPredefinedQuestionId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<SurveyUserInputPredefinedQuestionId> for Uuid {
    fn from(id: SurveyUserInputPredefinedQuestionId) -> Self { id.0 }
}

impl AsRef<Uuid> for SurveyUserInputPredefinedQuestionId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for SurveyUserInputPredefinedQuestionId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SurveyUserInputPredefinedQuestion {
    pub id: Uuid,
    pub user_input_id: Uuid,
    pub question_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl SurveyUserInputPredefinedQuestion {
    /// Create a builder for SurveyUserInputPredefinedQuestion
    pub fn builder() -> SurveyUserInputPredefinedQuestionBuilder {
        <SurveyUserInputPredefinedQuestionBuilder as Default>::default()
    }

    /// Create a new SurveyUserInputPredefinedQuestion with required fields
    pub fn new(user_input_id: Uuid, question_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_input_id,
            question_id,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> SurveyUserInputPredefinedQuestionId {
        SurveyUserInputPredefinedQuestionId(self.id)
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
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "user_input_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_input_id = v; }
                }
                "question_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.question_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for SurveyUserInputPredefinedQuestion {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "SurveyUserInputPredefinedQuestion"
    }
}

impl backbone_core::PersistentEntity for SurveyUserInputPredefinedQuestion {
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

impl backbone_orm::EntityRepoMeta for SurveyUserInputPredefinedQuestion {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("user_input_id".to_string(), "uuid".to_string());
        m.insert("question_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("userInput", "survey_user_inputs", "userInputId"), ("question", "survey_questions", "questionId")]
    }
}

/// Builder for SurveyUserInputPredefinedQuestion entity
///
/// Provides a fluent API for constructing SurveyUserInputPredefinedQuestion instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct SurveyUserInputPredefinedQuestionBuilder {
    user_input_id: Option<Uuid>,
    question_id: Option<Uuid>,
}

impl SurveyUserInputPredefinedQuestionBuilder {
    /// Set the user_input_id field (required)
    pub fn user_input_id(mut self, value: Uuid) -> Self {
        self.user_input_id = Some(value);
        self
    }

    /// Set the question_id field (required)
    pub fn question_id(mut self, value: Uuid) -> Self {
        self.question_id = Some(value);
        self
    }

    /// Build the SurveyUserInputPredefinedQuestion entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<SurveyUserInputPredefinedQuestion, String> {
        let user_input_id = self.user_input_id.ok_or_else(|| "user_input_id is required".to_string())?;
        let question_id = self.question_id.ok_or_else(|| "question_id is required".to_string())?;

        Ok(SurveyUserInputPredefinedQuestion {
            id: Uuid::new_v4(),
            user_input_id,
            question_id,
            metadata: AuditMetadata::default(),
        })
    }
}
