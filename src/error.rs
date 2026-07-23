use std::{error::Error, fmt};

pub type Result<T> = std::result::Result<T, CoralError>;

#[derive(Debug)]
pub struct CoralError(String);

impl CoralError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for CoralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for CoralError {}

impl From<std::io::Error> for CoralError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for CoralError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<toml::de::Error> for CoralError {
    fn from(error: toml::de::Error) -> Self {
        Self(format!("invalid capability manifest TOML: {error}"))
    }
}

impl From<toml::ser::Error> for CoralError {
    fn from(error: toml::ser::Error) -> Self {
        Self(format!("could not serialize TOML: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coral_error_displays_message() {
        let err = CoralError::new("test error");
        assert_eq!(format!("{}", err), "test error");
    }

    #[test]
    fn coral_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: CoralError = io_err.into();
        assert!(format!("{}", err).contains("file not found"));
    }

    #[test]
    fn coral_error_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: CoralError = json_err.into();
        assert!(!format!("{}", err).is_empty());
    }
}
