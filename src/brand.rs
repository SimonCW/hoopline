use crate::error::AppError;

/// Branding for one Hoopline deployment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Brand {
    #[default]
    Hoopline,
    Slomos,
}

impl Brand {
    pub(crate) fn from_env() -> Result<Self, AppError> {
        match std::env::var("APP_BRAND").as_deref() {
            Ok("hoopline") | Err(std::env::VarError::NotPresent) => Ok(Self::Hoopline),
            Ok("slomos") => Ok(Self::Slomos),
            _ => Err(AppError::Configuration(
                "APP_BRAND must be hoopline or slomos",
            )),
        }
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Hoopline => "Hoopline",
            Self::Slomos => "Slomos",
        }
    }

    pub(crate) const fn is_slomos(self) -> bool {
        matches!(self, Self::Slomos)
    }
}
