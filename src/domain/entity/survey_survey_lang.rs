use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for SurveySurveyLang
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurveySurveyLangId(pub Uuid);

impl SurveySurveyLangId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for SurveySurveyLangId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SurveySurveyLangId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for SurveySurveyLangId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<SurveySurveyLangId> for Uuid {
    fn from(id: SurveySurveyLangId) -> Self { id.0 }
}

impl AsRef<Uuid> for SurveySurveyLangId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for SurveySurveyLangId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SurveySurveyLang {
    pub id: Uuid,
    pub survey_id: Uuid,
    pub lang_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl SurveySurveyLang {
    /// Create a builder for SurveySurveyLang
    pub fn builder() -> SurveySurveyLangBuilder {
        <SurveySurveyLangBuilder as Default>::default()
    }

    /// Create a new SurveySurveyLang with required fields
    pub fn new(survey_id: Uuid, lang_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            survey_id,
            lang_id,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> SurveySurveyLangId {
        SurveySurveyLangId(self.id)
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
                "lang_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.lang_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for SurveySurveyLang {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "SurveySurveyLang"
    }
}

impl backbone_core::PersistentEntity for SurveySurveyLang {
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

impl backbone_orm::EntityRepoMeta for SurveySurveyLang {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("survey_id".to_string(), "uuid".to_string());
        m.insert("lang_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("survey", "survey_surveys", "surveyId")]
    }
}

/// Builder for SurveySurveyLang entity
///
/// Provides a fluent API for constructing SurveySurveyLang instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct SurveySurveyLangBuilder {
    survey_id: Option<Uuid>,
    lang_id: Option<Uuid>,
}

impl SurveySurveyLangBuilder {
    /// Set the survey_id field (required)
    pub fn survey_id(mut self, value: Uuid) -> Self {
        self.survey_id = Some(value);
        self
    }

    /// Set the lang_id field (required)
    pub fn lang_id(mut self, value: Uuid) -> Self {
        self.lang_id = Some(value);
        self
    }

    /// Build the SurveySurveyLang entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<SurveySurveyLang, String> {
        let survey_id = self.survey_id.ok_or_else(|| "survey_id is required".to_string())?;
        let lang_id = self.lang_id.ok_or_else(|| "lang_id is required".to_string())?;

        Ok(SurveySurveyLang {
            id: Uuid::new_v4(),
            survey_id,
            lang_id,
            metadata: AuditMetadata::default(),
        })
    }
}
