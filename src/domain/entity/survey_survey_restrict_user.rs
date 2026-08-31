use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for SurveySurveyRestrictUser
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurveySurveyRestrictUserId(pub Uuid);

impl SurveySurveyRestrictUserId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for SurveySurveyRestrictUserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SurveySurveyRestrictUserId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for SurveySurveyRestrictUserId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<SurveySurveyRestrictUserId> for Uuid {
    fn from(id: SurveySurveyRestrictUserId) -> Self { id.0 }
}

impl AsRef<Uuid> for SurveySurveyRestrictUserId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for SurveySurveyRestrictUserId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SurveySurveyRestrictUser {
    pub id: Uuid,
    pub survey_id: Uuid,
    pub user_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl SurveySurveyRestrictUser {
    /// Create a builder for SurveySurveyRestrictUser
    pub fn builder() -> SurveySurveyRestrictUserBuilder {
        <SurveySurveyRestrictUserBuilder as Default>::default()
    }

    /// Create a new SurveySurveyRestrictUser with required fields
    pub fn new(survey_id: Uuid, user_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            survey_id,
            user_id,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> SurveySurveyRestrictUserId {
        SurveySurveyRestrictUserId(self.id)
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
                "survey_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.survey_id = v; }
                }
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for SurveySurveyRestrictUser {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "SurveySurveyRestrictUser"
    }
}

impl backbone_core::PersistentEntity for SurveySurveyRestrictUser {
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

impl backbone_orm::EntityRepoMeta for SurveySurveyRestrictUser {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("survey_id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("survey", "survey_surveys", "surveyId")]
    }
}

/// Builder for SurveySurveyRestrictUser entity
///
/// Provides a fluent API for constructing SurveySurveyRestrictUser instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct SurveySurveyRestrictUserBuilder {
    survey_id: Option<Uuid>,
    user_id: Option<Uuid>,
}

impl SurveySurveyRestrictUserBuilder {
    /// Set the survey_id field (required)
    pub fn survey_id(mut self, value: Uuid) -> Self {
        self.survey_id = Some(value);
        self
    }

    /// Set the user_id field (required)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Build the SurveySurveyRestrictUser entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<SurveySurveyRestrictUser, String> {
        let survey_id = self.survey_id.ok_or_else(|| "survey_id is required".to_string())?;
        let user_id = self.user_id.ok_or_else(|| "user_id is required".to_string())?;

        Ok(SurveySurveyRestrictUser {
            id: Uuid::new_v4(),
            survey_id,
            user_id,
            metadata: AuditMetadata::default(),
        })
    }
}
