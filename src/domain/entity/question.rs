use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::SurveyQuestionType;
use super::SurveyMatrixSubtype;
use super::AuditMetadata;

/// Strongly-typed ID for Question
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QuestionId(pub Uuid);

impl QuestionId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for QuestionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for QuestionId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for QuestionId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<QuestionId> for Uuid {
    fn from(id: QuestionId) -> Self { id.0 }
}

impl AsRef<Uuid> for QuestionId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for QuestionId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Question {
    pub id: Uuid,
    pub survey_id: Uuid,
    pub sequence: i32,
    pub is_page: bool,
    pub question_type: Option<SurveyQuestionType>,
    pub title: String,
    pub description: Option<String>,
    pub question_placeholder: Option<String>,
    pub background_image: Option<String>,
    pub random_questions_count: i32,
    pub is_scored_question: bool,
    pub answer_numerical_box: Option<f64>,
    pub answer_date: Option<NaiveDate>,
    pub answer_datetime: Option<DateTime<Utc>>,
    pub answer_score: f64,
    pub save_as_email: bool,
    pub save_as_nickname: bool,
    pub matrix_subtype: SurveyMatrixSubtype,
    pub scale_min: i32,
    pub scale_max: i32,
    pub scale_min_label: Option<String>,
    pub scale_mid_label: Option<String>,
    pub scale_max_label: Option<String>,
    pub is_time_limited: bool,
    pub time_limit: Option<i32>,
    pub is_time_customized: bool,
    pub comments_allowed: bool,
    pub comments_message: Option<String>,
    pub comment_count_as_answer: bool,
    pub validation_required: bool,
    pub validation_email: bool,
    pub validation_length_min: i32,
    pub validation_length_max: i32,
    pub validation_min_float_value: Option<f64>,
    pub validation_max_float_value: Option<f64>,
    pub validation_min_date: Option<NaiveDate>,
    pub validation_max_date: Option<NaiveDate>,
    pub validation_min_datetime: Option<DateTime<Utc>>,
    pub validation_max_datetime: Option<DateTime<Utc>>,
    pub validation_error_msg: Option<String>,
    pub constr_error_msg: Option<String>,
    pub constr_mandatory: bool,
    pub page_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Question {
    /// Create a builder for Question
    pub fn builder() -> QuestionBuilder {
        <QuestionBuilder as Default>::default()
    }

    /// Create a new Question with required fields
    pub fn new(survey_id: Uuid, sequence: i32, is_page: bool, title: String, random_questions_count: i32, is_scored_question: bool, answer_score: f64, save_as_email: bool, save_as_nickname: bool, matrix_subtype: SurveyMatrixSubtype, scale_min: i32, scale_max: i32, is_time_limited: bool, is_time_customized: bool, comments_allowed: bool, comment_count_as_answer: bool, validation_required: bool, validation_email: bool, validation_length_min: i32, validation_length_max: i32, constr_mandatory: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            survey_id,
            sequence,
            is_page,
            question_type: None,
            title,
            description: None,
            question_placeholder: None,
            background_image: None,
            random_questions_count,
            is_scored_question,
            answer_numerical_box: None,
            answer_date: None,
            answer_datetime: None,
            answer_score,
            save_as_email,
            save_as_nickname,
            matrix_subtype,
            scale_min,
            scale_max,
            scale_min_label: None,
            scale_mid_label: None,
            scale_max_label: None,
            is_time_limited,
            time_limit: None,
            is_time_customized,
            comments_allowed,
            comments_message: None,
            comment_count_as_answer,
            validation_required,
            validation_email,
            validation_length_min,
            validation_length_max,
            validation_min_float_value: None,
            validation_max_float_value: None,
            validation_min_date: None,
            validation_max_date: None,
            validation_min_datetime: None,
            validation_max_datetime: None,
            validation_error_msg: None,
            constr_error_msg: None,
            constr_mandatory,
            page_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> QuestionId {
        QuestionId(self.id)
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

    /// Set the question_type field (chainable)
    pub fn with_question_type(mut self, value: SurveyQuestionType) -> Self {
        self.question_type = Some(value);
        self
    }

    /// Set the description field (chainable)
    pub fn with_description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the question_placeholder field (chainable)
    pub fn with_question_placeholder(mut self, value: String) -> Self {
        self.question_placeholder = Some(value);
        self
    }

    /// Set the background_image field (chainable)
    pub fn with_background_image(mut self, value: String) -> Self {
        self.background_image = Some(value);
        self
    }

    /// Set the answer_numerical_box field (chainable)
    pub fn with_answer_numerical_box(mut self, value: f64) -> Self {
        self.answer_numerical_box = Some(value);
        self
    }

    /// Set the answer_date field (chainable)
    pub fn with_answer_date(mut self, value: NaiveDate) -> Self {
        self.answer_date = Some(value);
        self
    }

    /// Set the answer_datetime field (chainable)
    pub fn with_answer_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.answer_datetime = Some(value);
        self
    }

    /// Set the scale_min_label field (chainable)
    pub fn with_scale_min_label(mut self, value: String) -> Self {
        self.scale_min_label = Some(value);
        self
    }

    /// Set the scale_mid_label field (chainable)
    pub fn with_scale_mid_label(mut self, value: String) -> Self {
        self.scale_mid_label = Some(value);
        self
    }

    /// Set the scale_max_label field (chainable)
    pub fn with_scale_max_label(mut self, value: String) -> Self {
        self.scale_max_label = Some(value);
        self
    }

    /// Set the time_limit field (chainable)
    pub fn with_time_limit(mut self, value: i32) -> Self {
        self.time_limit = Some(value);
        self
    }

    /// Set the comments_message field (chainable)
    pub fn with_comments_message(mut self, value: String) -> Self {
        self.comments_message = Some(value);
        self
    }

    /// Set the validation_min_float_value field (chainable)
    pub fn with_validation_min_float_value(mut self, value: f64) -> Self {
        self.validation_min_float_value = Some(value);
        self
    }

    /// Set the validation_max_float_value field (chainable)
    pub fn with_validation_max_float_value(mut self, value: f64) -> Self {
        self.validation_max_float_value = Some(value);
        self
    }

    /// Set the validation_min_date field (chainable)
    pub fn with_validation_min_date(mut self, value: NaiveDate) -> Self {
        self.validation_min_date = Some(value);
        self
    }

    /// Set the validation_max_date field (chainable)
    pub fn with_validation_max_date(mut self, value: NaiveDate) -> Self {
        self.validation_max_date = Some(value);
        self
    }

    /// Set the validation_min_datetime field (chainable)
    pub fn with_validation_min_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.validation_min_datetime = Some(value);
        self
    }

    /// Set the validation_max_datetime field (chainable)
    pub fn with_validation_max_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.validation_max_datetime = Some(value);
        self
    }

    /// Set the validation_error_msg field (chainable)
    pub fn with_validation_error_msg(mut self, value: String) -> Self {
        self.validation_error_msg = Some(value);
        self
    }

    /// Set the constr_error_msg field (chainable)
    pub fn with_constr_error_msg(mut self, value: String) -> Self {
        self.constr_error_msg = Some(value);
        self
    }

    /// Set the page_id field (chainable)
    pub fn with_page_id(mut self, value: Uuid) -> Self {
        self.page_id = Some(value);
        self
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
                "sequence" => {
                    if let Ok(v) = serde_json::from_value(value) { self.sequence = v; }
                }
                "is_page" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_page = v; }
                }
                "question_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.question_type = v; }
                }
                "title" => {
                    if let Ok(v) = serde_json::from_value(value) { self.title = v; }
                }
                "description" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description = v; }
                }
                "question_placeholder" => {
                    if let Ok(v) = serde_json::from_value(value) { self.question_placeholder = v; }
                }
                "background_image" => {
                    if let Ok(v) = serde_json::from_value(value) { self.background_image = v; }
                }
                "random_questions_count" => {
                    if let Ok(v) = serde_json::from_value(value) { self.random_questions_count = v; }
                }
                "is_scored_question" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_scored_question = v; }
                }
                "answer_numerical_box" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answer_numerical_box = v; }
                }
                "answer_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answer_date = v; }
                }
                "answer_datetime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answer_datetime = v; }
                }
                "answer_score" => {
                    if let Ok(v) = serde_json::from_value(value) { self.answer_score = v; }
                }
                "save_as_email" => {
                    if let Ok(v) = serde_json::from_value(value) { self.save_as_email = v; }
                }
                "save_as_nickname" => {
                    if let Ok(v) = serde_json::from_value(value) { self.save_as_nickname = v; }
                }
                "matrix_subtype" => {
                    if let Ok(v) = serde_json::from_value(value) { self.matrix_subtype = v; }
                }
                "scale_min" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scale_min = v; }
                }
                "scale_max" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scale_max = v; }
                }
                "scale_min_label" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scale_min_label = v; }
                }
                "scale_mid_label" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scale_mid_label = v; }
                }
                "scale_max_label" => {
                    if let Ok(v) = serde_json::from_value(value) { self.scale_max_label = v; }
                }
                "is_time_limited" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_time_limited = v; }
                }
                "time_limit" => {
                    if let Ok(v) = serde_json::from_value(value) { self.time_limit = v; }
                }
                "is_time_customized" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_time_customized = v; }
                }
                "comments_allowed" => {
                    if let Ok(v) = serde_json::from_value(value) { self.comments_allowed = v; }
                }
                "comments_message" => {
                    if let Ok(v) = serde_json::from_value(value) { self.comments_message = v; }
                }
                "comment_count_as_answer" => {
                    if let Ok(v) = serde_json::from_value(value) { self.comment_count_as_answer = v; }
                }
                "validation_required" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_required = v; }
                }
                "validation_email" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_email = v; }
                }
                "validation_length_min" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_length_min = v; }
                }
                "validation_length_max" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_length_max = v; }
                }
                "validation_min_float_value" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_min_float_value = v; }
                }
                "validation_max_float_value" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_max_float_value = v; }
                }
                "validation_min_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_min_date = v; }
                }
                "validation_max_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_max_date = v; }
                }
                "validation_min_datetime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_min_datetime = v; }
                }
                "validation_max_datetime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_max_datetime = v; }
                }
                "validation_error_msg" => {
                    if let Ok(v) = serde_json::from_value(value) { self.validation_error_msg = v; }
                }
                "constr_error_msg" => {
                    if let Ok(v) = serde_json::from_value(value) { self.constr_error_msg = v; }
                }
                "constr_mandatory" => {
                    if let Ok(v) = serde_json::from_value(value) { self.constr_mandatory = v; }
                }
                "page_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.page_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Question {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Question"
    }
}

impl backbone_core::PersistentEntity for Question {
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

impl backbone_orm::EntityRepoMeta for Question {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("survey_id".to_string(), "uuid".to_string());
        m.insert("page_id".to_string(), "uuid".to_string());
        m.insert("question_type".to_string(), "survey_question_type".to_string());
        m.insert("matrix_subtype".to_string(), "survey_matrix_subtype".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["title"]
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("survey", "survey_surveys", "surveyId")]
    }
}

/// Builder for Question entity
///
/// Provides a fluent API for constructing Question instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct QuestionBuilder {
    survey_id: Option<Uuid>,
    sequence: Option<i32>,
    is_page: Option<bool>,
    question_type: Option<SurveyQuestionType>,
    title: Option<String>,
    description: Option<String>,
    question_placeholder: Option<String>,
    background_image: Option<String>,
    random_questions_count: Option<i32>,
    is_scored_question: Option<bool>,
    answer_numerical_box: Option<f64>,
    answer_date: Option<NaiveDate>,
    answer_datetime: Option<DateTime<Utc>>,
    answer_score: Option<f64>,
    save_as_email: Option<bool>,
    save_as_nickname: Option<bool>,
    matrix_subtype: Option<SurveyMatrixSubtype>,
    scale_min: Option<i32>,
    scale_max: Option<i32>,
    scale_min_label: Option<String>,
    scale_mid_label: Option<String>,
    scale_max_label: Option<String>,
    is_time_limited: Option<bool>,
    time_limit: Option<i32>,
    is_time_customized: Option<bool>,
    comments_allowed: Option<bool>,
    comments_message: Option<String>,
    comment_count_as_answer: Option<bool>,
    validation_required: Option<bool>,
    validation_email: Option<bool>,
    validation_length_min: Option<i32>,
    validation_length_max: Option<i32>,
    validation_min_float_value: Option<f64>,
    validation_max_float_value: Option<f64>,
    validation_min_date: Option<NaiveDate>,
    validation_max_date: Option<NaiveDate>,
    validation_min_datetime: Option<DateTime<Utc>>,
    validation_max_datetime: Option<DateTime<Utc>>,
    validation_error_msg: Option<String>,
    constr_error_msg: Option<String>,
    constr_mandatory: Option<bool>,
    page_id: Option<Uuid>,
}

impl QuestionBuilder {
    /// Set the survey_id field (required)
    pub fn survey_id(mut self, value: Uuid) -> Self {
        self.survey_id = Some(value);
        self
    }

    /// Set the sequence field (default: `10`)
    pub fn sequence(mut self, value: i32) -> Self {
        self.sequence = Some(value);
        self
    }

    /// Set the is_page field (default: `false`)
    pub fn is_page(mut self, value: bool) -> Self {
        self.is_page = Some(value);
        self
    }

    /// Set the question_type field (optional)
    pub fn question_type(mut self, value: SurveyQuestionType) -> Self {
        self.question_type = Some(value);
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

    /// Set the question_placeholder field (optional)
    pub fn question_placeholder(mut self, value: String) -> Self {
        self.question_placeholder = Some(value);
        self
    }

    /// Set the background_image field (optional)
    pub fn background_image(mut self, value: String) -> Self {
        self.background_image = Some(value);
        self
    }

    /// Set the random_questions_count field (default: `1`)
    pub fn random_questions_count(mut self, value: i32) -> Self {
        self.random_questions_count = Some(value);
        self
    }

    /// Set the is_scored_question field (default: `false`)
    pub fn is_scored_question(mut self, value: bool) -> Self {
        self.is_scored_question = Some(value);
        self
    }

    /// Set the answer_numerical_box field (optional)
    pub fn answer_numerical_box(mut self, value: f64) -> Self {
        self.answer_numerical_box = Some(value);
        self
    }

    /// Set the answer_date field (optional)
    pub fn answer_date(mut self, value: NaiveDate) -> Self {
        self.answer_date = Some(value);
        self
    }

    /// Set the answer_datetime field (optional)
    pub fn answer_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.answer_datetime = Some(value);
        self
    }

    /// Set the answer_score field (default: `0_f64`)
    pub fn answer_score(mut self, value: f64) -> Self {
        self.answer_score = Some(value);
        self
    }

    /// Set the save_as_email field (default: `false`)
    pub fn save_as_email(mut self, value: bool) -> Self {
        self.save_as_email = Some(value);
        self
    }

    /// Set the save_as_nickname field (default: `false`)
    pub fn save_as_nickname(mut self, value: bool) -> Self {
        self.save_as_nickname = Some(value);
        self
    }

    /// Set the matrix_subtype field (default: `SurveyMatrixSubtype::default()`)
    pub fn matrix_subtype(mut self, value: SurveyMatrixSubtype) -> Self {
        self.matrix_subtype = Some(value);
        self
    }

    /// Set the scale_min field (default: `0`)
    pub fn scale_min(mut self, value: i32) -> Self {
        self.scale_min = Some(value);
        self
    }

    /// Set the scale_max field (default: `10`)
    pub fn scale_max(mut self, value: i32) -> Self {
        self.scale_max = Some(value);
        self
    }

    /// Set the scale_min_label field (optional)
    pub fn scale_min_label(mut self, value: String) -> Self {
        self.scale_min_label = Some(value);
        self
    }

    /// Set the scale_mid_label field (optional)
    pub fn scale_mid_label(mut self, value: String) -> Self {
        self.scale_mid_label = Some(value);
        self
    }

    /// Set the scale_max_label field (optional)
    pub fn scale_max_label(mut self, value: String) -> Self {
        self.scale_max_label = Some(value);
        self
    }

    /// Set the is_time_limited field (default: `false`)
    pub fn is_time_limited(mut self, value: bool) -> Self {
        self.is_time_limited = Some(value);
        self
    }

    /// Set the time_limit field (optional)
    pub fn time_limit(mut self, value: i32) -> Self {
        self.time_limit = Some(value);
        self
    }

    /// Set the is_time_customized field (default: `false`)
    pub fn is_time_customized(mut self, value: bool) -> Self {
        self.is_time_customized = Some(value);
        self
    }

    /// Set the comments_allowed field (default: `false`)
    pub fn comments_allowed(mut self, value: bool) -> Self {
        self.comments_allowed = Some(value);
        self
    }

    /// Set the comments_message field (optional)
    pub fn comments_message(mut self, value: String) -> Self {
        self.comments_message = Some(value);
        self
    }

    /// Set the comment_count_as_answer field (default: `false`)
    pub fn comment_count_as_answer(mut self, value: bool) -> Self {
        self.comment_count_as_answer = Some(value);
        self
    }

    /// Set the validation_required field (default: `false`)
    pub fn validation_required(mut self, value: bool) -> Self {
        self.validation_required = Some(value);
        self
    }

    /// Set the validation_email field (default: `false`)
    pub fn validation_email(mut self, value: bool) -> Self {
        self.validation_email = Some(value);
        self
    }

    /// Set the validation_length_min field (default: `0`)
    pub fn validation_length_min(mut self, value: i32) -> Self {
        self.validation_length_min = Some(value);
        self
    }

    /// Set the validation_length_max field (default: `0`)
    pub fn validation_length_max(mut self, value: i32) -> Self {
        self.validation_length_max = Some(value);
        self
    }

    /// Set the validation_min_float_value field (optional)
    pub fn validation_min_float_value(mut self, value: f64) -> Self {
        self.validation_min_float_value = Some(value);
        self
    }

    /// Set the validation_max_float_value field (optional)
    pub fn validation_max_float_value(mut self, value: f64) -> Self {
        self.validation_max_float_value = Some(value);
        self
    }

    /// Set the validation_min_date field (optional)
    pub fn validation_min_date(mut self, value: NaiveDate) -> Self {
        self.validation_min_date = Some(value);
        self
    }

    /// Set the validation_max_date field (optional)
    pub fn validation_max_date(mut self, value: NaiveDate) -> Self {
        self.validation_max_date = Some(value);
        self
    }

    /// Set the validation_min_datetime field (optional)
    pub fn validation_min_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.validation_min_datetime = Some(value);
        self
    }

    /// Set the validation_max_datetime field (optional)
    pub fn validation_max_datetime(mut self, value: DateTime<Utc>) -> Self {
        self.validation_max_datetime = Some(value);
        self
    }

    /// Set the validation_error_msg field (optional)
    pub fn validation_error_msg(mut self, value: String) -> Self {
        self.validation_error_msg = Some(value);
        self
    }

    /// Set the constr_error_msg field (optional)
    pub fn constr_error_msg(mut self, value: String) -> Self {
        self.constr_error_msg = Some(value);
        self
    }

    /// Set the constr_mandatory field (default: `false`)
    pub fn constr_mandatory(mut self, value: bool) -> Self {
        self.constr_mandatory = Some(value);
        self
    }

    /// Set the page_id field (optional)
    pub fn page_id(mut self, value: Uuid) -> Self {
        self.page_id = Some(value);
        self
    }

    /// Build the Question entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Question, String> {
        let survey_id = self.survey_id.ok_or_else(|| "survey_id is required".to_string())?;
        let title = self.title.ok_or_else(|| "title is required".to_string())?;

        Ok(Question {
            id: Uuid::new_v4(),
            survey_id,
            sequence: self.sequence.unwrap_or(10),
            is_page: self.is_page.unwrap_or(false),
            question_type: self.question_type,
            title,
            description: self.description,
            question_placeholder: self.question_placeholder,
            background_image: self.background_image,
            random_questions_count: self.random_questions_count.unwrap_or(1),
            is_scored_question: self.is_scored_question.unwrap_or(false),
            answer_numerical_box: self.answer_numerical_box,
            answer_date: self.answer_date,
            answer_datetime: self.answer_datetime,
            answer_score: self.answer_score.unwrap_or(0_f64),
            save_as_email: self.save_as_email.unwrap_or(false),
            save_as_nickname: self.save_as_nickname.unwrap_or(false),
            matrix_subtype: self.matrix_subtype.unwrap_or_default(),
            scale_min: self.scale_min.unwrap_or(0),
            scale_max: self.scale_max.unwrap_or(10),
            scale_min_label: self.scale_min_label,
            scale_mid_label: self.scale_mid_label,
            scale_max_label: self.scale_max_label,
            is_time_limited: self.is_time_limited.unwrap_or(false),
            time_limit: self.time_limit,
            is_time_customized: self.is_time_customized.unwrap_or(false),
            comments_allowed: self.comments_allowed.unwrap_or(false),
            comments_message: self.comments_message,
            comment_count_as_answer: self.comment_count_as_answer.unwrap_or(false),
            validation_required: self.validation_required.unwrap_or(false),
            validation_email: self.validation_email.unwrap_or(false),
            validation_length_min: self.validation_length_min.unwrap_or(0),
            validation_length_max: self.validation_length_max.unwrap_or(0),
            validation_min_float_value: self.validation_min_float_value,
            validation_max_float_value: self.validation_max_float_value,
            validation_min_date: self.validation_min_date,
            validation_max_date: self.validation_max_date,
            validation_min_datetime: self.validation_min_datetime,
            validation_max_datetime: self.validation_max_datetime,
            validation_error_msg: self.validation_error_msg,
            constr_error_msg: self.constr_error_msg,
            constr_mandatory: self.constr_mandatory.unwrap_or(false),
            page_id: self.page_id,
            metadata: AuditMetadata::default(),
        })
    }
}
