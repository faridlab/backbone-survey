use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_question_type", rename_all = "snake_case")]
pub enum SurveyQuestionType {
    SimpleChoice,
    MultipleChoice,
    TextBox,
    CharBox,
    NumericalBox,
    Scale,
    Date,
    Datetime,
    Matrix,
}

impl std::fmt::Display for SurveyQuestionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SimpleChoice => write!(f, "simple_choice"),
            Self::MultipleChoice => write!(f, "multiple_choice"),
            Self::TextBox => write!(f, "text_box"),
            Self::CharBox => write!(f, "char_box"),
            Self::NumericalBox => write!(f, "numerical_box"),
            Self::Scale => write!(f, "scale"),
            Self::Date => write!(f, "date"),
            Self::Datetime => write!(f, "datetime"),
            Self::Matrix => write!(f, "matrix"),
        }
    }
}

impl FromStr for SurveyQuestionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "simple_choice" => Ok(Self::SimpleChoice),
            "multiple_choice" => Ok(Self::MultipleChoice),
            "text_box" => Ok(Self::TextBox),
            "char_box" => Ok(Self::CharBox),
            "numerical_box" => Ok(Self::NumericalBox),
            "scale" => Ok(Self::Scale),
            "date" => Ok(Self::Date),
            "datetime" => Ok(Self::Datetime),
            "matrix" => Ok(Self::Matrix),
            _ => Err(format!("Unknown SurveyQuestionType variant: {}", s)),
        }
    }
}

impl Default for SurveyQuestionType {
    fn default() -> Self {
        Self::SimpleChoice
    }
}
