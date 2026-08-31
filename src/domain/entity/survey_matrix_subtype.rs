use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_matrix_subtype", rename_all = "snake_case")]
pub enum SurveyMatrixSubtype {
    Simple,
    Multiple,
}

impl std::fmt::Display for SurveyMatrixSubtype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simple => write!(f, "simple"),
            Self::Multiple => write!(f, "multiple"),
        }
    }
}

impl FromStr for SurveyMatrixSubtype {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "simple" => Ok(Self::Simple),
            "multiple" => Ok(Self::Multiple),
            _ => Err(format!("Unknown SurveyMatrixSubtype variant: {}", s)),
        }
    }
}

impl Default for SurveyMatrixSubtype {
    fn default() -> Self {
        Self::Simple
    }
}
