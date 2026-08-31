use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_access_mode", rename_all = "snake_case")]
pub enum SurveyAccessMode {
    Public,
    Token,
}

impl std::fmt::Display for SurveyAccessMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::Token => write!(f, "token"),
        }
    }
}

impl FromStr for SurveyAccessMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "public" => Ok(Self::Public),
            "token" => Ok(Self::Token),
            _ => Err(format!("Unknown SurveyAccessMode variant: {}", s)),
        }
    }
}

impl Default for SurveyAccessMode {
    fn default() -> Self {
        Self::Public
    }
}
