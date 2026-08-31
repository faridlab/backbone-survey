use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_progression_mode", rename_all = "snake_case")]
pub enum SurveyProgressionMode {
    Percent,
    Number,
}

impl std::fmt::Display for SurveyProgressionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Percent => write!(f, "percent"),
            Self::Number => write!(f, "number"),
        }
    }
}

impl FromStr for SurveyProgressionMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "percent" => Ok(Self::Percent),
            "number" => Ok(Self::Number),
            _ => Err(format!("Unknown SurveyProgressionMode variant: {}", s)),
        }
    }
}

impl Default for SurveyProgressionMode {
    fn default() -> Self {
        Self::Percent
    }
}
