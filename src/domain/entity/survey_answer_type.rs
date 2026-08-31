use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_answer_type", rename_all = "snake_case")]
pub enum SurveyAnswerType {
    TextBox,
    CharBox,
    NumericalBox,
    Scale,
    Date,
    Datetime,
    Suggestion,
}

impl std::fmt::Display for SurveyAnswerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TextBox => write!(f, "text_box"),
            Self::CharBox => write!(f, "char_box"),
            Self::NumericalBox => write!(f, "numerical_box"),
            Self::Scale => write!(f, "scale"),
            Self::Date => write!(f, "date"),
            Self::Datetime => write!(f, "datetime"),
            Self::Suggestion => write!(f, "suggestion"),
        }
    }
}

impl FromStr for SurveyAnswerType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text_box" => Ok(Self::TextBox),
            "char_box" => Ok(Self::CharBox),
            "numerical_box" => Ok(Self::NumericalBox),
            "scale" => Ok(Self::Scale),
            "date" => Ok(Self::Date),
            "datetime" => Ok(Self::Datetime),
            "suggestion" => Ok(Self::Suggestion),
            _ => Err(format!("Unknown SurveyAnswerType variant: {}", s)),
        }
    }
}

impl Default for SurveyAnswerType {
    fn default() -> Self {
        Self::TextBox
    }
}
