use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "survey_report_layout", rename_all = "snake_case")]
pub enum SurveyReportLayout {
    ModernPurple,
    ModernBlue,
    ModernGold,
    ClassicPurple,
    ClassicBlue,
    ClassicGold,
}

impl std::fmt::Display for SurveyReportLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModernPurple => write!(f, "modern_purple"),
            Self::ModernBlue => write!(f, "modern_blue"),
            Self::ModernGold => write!(f, "modern_gold"),
            Self::ClassicPurple => write!(f, "classic_purple"),
            Self::ClassicBlue => write!(f, "classic_blue"),
            Self::ClassicGold => write!(f, "classic_gold"),
        }
    }
}

impl FromStr for SurveyReportLayout {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "modern_purple" => Ok(Self::ModernPurple),
            "modern_blue" => Ok(Self::ModernBlue),
            "modern_gold" => Ok(Self::ModernGold),
            "classic_purple" => Ok(Self::ClassicPurple),
            "classic_blue" => Ok(Self::ClassicBlue),
            "classic_gold" => Ok(Self::ClassicGold),
            _ => Err(format!("Unknown SurveyReportLayout variant: {}", s)),
        }
    }
}

impl Default for SurveyReportLayout {
    fn default() -> Self {
        Self::ModernPurple
    }
}
