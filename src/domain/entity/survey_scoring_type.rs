use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_scoring_type", rename_all = "snake_case")]
pub enum SurveyScoringType {
    NoScoring,
    ScoringWithAnswersAfterPage,
    ScoringWithAnswers,
    ScoringWithoutAnswers,
}

impl std::fmt::Display for SurveyScoringType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoScoring => write!(f, "no_scoring"),
            Self::ScoringWithAnswersAfterPage => write!(f, "scoring_with_answers_after_page"),
            Self::ScoringWithAnswers => write!(f, "scoring_with_answers"),
            Self::ScoringWithoutAnswers => write!(f, "scoring_without_answers"),
        }
    }
}

impl FromStr for SurveyScoringType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "no_scoring" => Ok(Self::NoScoring),
            "scoring_with_answers_after_page" => Ok(Self::ScoringWithAnswersAfterPage),
            "scoring_with_answers" => Ok(Self::ScoringWithAnswers),
            "scoring_without_answers" => Ok(Self::ScoringWithoutAnswers),
            _ => Err(format!("Unknown SurveyScoringType variant: {}", s)),
        }
    }
}

impl Default for SurveyScoringType {
    fn default() -> Self {
        Self::NoScoring
    }
}
