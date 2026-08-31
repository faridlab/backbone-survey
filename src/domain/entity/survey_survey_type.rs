use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_survey_type", rename_all = "snake_case")]
pub enum SurveySurveyType {
    Survey,
    LiveSession,
    Assessment,
    Custom,
}

impl std::fmt::Display for SurveySurveyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Survey => write!(f, "survey"),
            Self::LiveSession => write!(f, "live_session"),
            Self::Assessment => write!(f, "assessment"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

impl FromStr for SurveySurveyType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "survey" => Ok(Self::Survey),
            "live_session" => Ok(Self::LiveSession),
            "assessment" => Ok(Self::Assessment),
            "custom" => Ok(Self::Custom),
            _ => Err(format!("Unknown SurveySurveyType variant: {}", s)),
        }
    }
}

impl Default for SurveySurveyType {
    fn default() -> Self {
        Self::Custom
    }
}
