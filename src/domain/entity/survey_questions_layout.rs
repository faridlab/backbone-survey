use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_questions_layout", rename_all = "snake_case")]
pub enum SurveyQuestionsLayout {
    PagePerQuestion,
    PagePerSection,
    OnePage,
}

impl std::fmt::Display for SurveyQuestionsLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PagePerQuestion => write!(f, "page_per_question"),
            Self::PagePerSection => write!(f, "page_per_section"),
            Self::OnePage => write!(f, "one_page"),
        }
    }
}

impl FromStr for SurveyQuestionsLayout {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "page_per_question" => Ok(Self::PagePerQuestion),
            "page_per_section" => Ok(Self::PagePerSection),
            "one_page" => Ok(Self::OnePage),
            _ => Err(format!("Unknown SurveyQuestionsLayout variant: {}", s)),
        }
    }
}

impl Default for SurveyQuestionsLayout {
    fn default() -> Self {
        Self::PagePerQuestion
    }
}
