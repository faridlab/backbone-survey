use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_session_state", rename_all = "snake_case")]
pub enum SurveySessionState {
    Ready,
    InProgress,
}

impl std::fmt::Display for SurveySessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready => write!(f, "ready"),
            Self::InProgress => write!(f, "in_progress"),
        }
    }
}

impl FromStr for SurveySessionState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ready" => Ok(Self::Ready),
            "in_progress" => Ok(Self::InProgress),
            _ => Err(format!("Unknown SurveySessionState variant: {}", s)),
        }
    }
}

impl Default for SurveySessionState {
    fn default() -> Self {
        Self::Ready
    }
}
