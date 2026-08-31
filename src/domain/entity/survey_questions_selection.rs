use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_questions_selection", rename_all = "snake_case")]
pub enum SurveyQuestionsSelection {
    All,
    Random,
}

impl std::fmt::Display for SurveyQuestionsSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => write!(f, "all"),
            Self::Random => write!(f, "random"),
        }
    }
}

impl FromStr for SurveyQuestionsSelection {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(Self::All),
            "random" => Ok(Self::Random),
            _ => Err(format!("Unknown SurveyQuestionsSelection variant: {}", s)),
        }
    }
}

impl Default for SurveyQuestionsSelection {
    fn default() -> Self {
        Self::All
    }
}
