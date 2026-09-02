use std::{error::Error, fmt};

pub type Result<T> = std::result::Result<T, TuffError>;

/// What kind of failure this is (RFC-105 D6).
///
/// The kind exists so callers can branch without reading prose: a script
/// checks the exit code, `--json` output carries the kind string, and a
/// library consumer matches the enum. Every variant answers a different
/// question about whose problem it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Wrong arguments or flags for the situation. The user's input.
    Usage,
    /// A capability, lockfile, agent, or catalog id that does not exist.
    NotFound,
    /// A never-overwrite guard fired: Tuff refused to clobber something.
    Refused,
    /// Local changes block the operation; `--force` is usually the answer.
    Drift,
    /// A source failed: git, an OCI registry, the catalog, the network.
    Source,
    /// A file Tuff must parse is not valid: lockfile, manifest, config.
    Corrupt,
    /// Understood but not supported: a newer schema, an unknown transport,
    /// an adapter that cannot express this capability.
    Unsupported,
    /// The filesystem said no.
    Io,
    /// An invariant Tuff itself broke. Always a bug worth reporting.
    Internal,
}

impl ErrorKind {
    /// The stable string used in `--json` output and in tests.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::NotFound => "not_found",
            Self::Refused => "refused",
            Self::Drift => "drift",
            Self::Source => "source",
            Self::Corrupt => "corrupt",
            Self::Unsupported => "unsupported",
            Self::Io => "io",
            Self::Internal => "internal",
        }
    }

    /// The process exit code for this kind.
    ///
    /// Three codes are what scripts actually branch on: 2 separates "you
    /// typed it wrong" from a real failure, and 70 (the BSD `EX_SOFTWARE`
    /// convention) separates "Tuff has a bug" from both, so a CI log can
    /// tell them apart without parsing text. Everything else is 1.
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Usage => 2,
            Self::Internal => 70,
            _ => 1,
        }
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One failure, with the kind of thing that went wrong, what happened, and
/// optionally what to do about it.
#[derive(Debug)]
pub struct TuffError {
    kind: ErrorKind,
    message: String,
    hint: Option<String>,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl TuffError {
    /// Build an error of a given kind. Prefer the per-kind constructors.
    pub fn of(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
            source: None,
        }
    }

    /// An error for an invariant Tuff itself broke.
    ///
    /// This is also the landing spot for messages not yet classified during
    /// the migration to typed kinds, which is why the lint gate counts its
    /// uses outside this module and only lets that count fall.
    pub fn new(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Internal, message)
    }

    pub fn usage(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Usage, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::NotFound, message)
    }

    pub fn refused(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Refused, message)
    }

    pub fn drift(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Drift, message)
    }

    pub fn source_failed(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Source, message)
    }

    pub fn corrupt(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Corrupt, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Unsupported, message)
    }

    /// Attach one imperative line telling the user what to do next. Kept
    /// separate from the message so `--json` can carry it as its own field
    /// and humans get it on its own line.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Attach the underlying error, preserving the cause chain.
    #[must_use]
    pub fn with_source(mut self, source: impl Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    pub fn exit_code(&self) -> i32 {
        self.kind.exit_code()
    }
}

impl fmt::Display for TuffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for TuffError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn Error + 'static))
    }
}

impl From<std::io::Error> for TuffError {
    fn from(error: std::io::Error) -> Self {
        Self::of(ErrorKind::Io, error.to_string()).with_source(error)
    }
}

impl From<serde_json::Error> for TuffError {
    fn from(error: serde_json::Error) -> Self {
        Self::of(ErrorKind::Corrupt, error.to_string()).with_source(error)
    }
}

impl From<toml::de::Error> for TuffError {
    fn from(error: toml::de::Error) -> Self {
        Self::of(
            ErrorKind::Corrupt,
            format!("invalid capability manifest TOML: {error}"),
        )
        .with_source(error)
    }
}

impl From<toml::ser::Error> for TuffError {
    fn from(error: toml::ser::Error) -> Self {
        Self::of(
            ErrorKind::Internal,
            format!("could not serialize TOML: {error}"),
        )
        .with_source(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tuff_error_displays_message() {
        let err = TuffError::new("test error");
        assert_eq!(format!("{}", err), "test error");
    }

    #[test]
    fn tuff_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: TuffError = io_err.into();
        assert!(format!("{}", err).contains("file not found"));
        assert_eq!(err.kind(), ErrorKind::Io);
        assert!(err.source().is_some(), "the cause chain is preserved");
    }

    #[test]
    fn tuff_error_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let err: TuffError = json_err.into();
        assert!(!format!("{}", err).is_empty());
        assert_eq!(err.kind(), ErrorKind::Corrupt);
    }

    #[test]
    fn exit_codes_separate_the_three_audiences() {
        // A script needs to tell "I called it wrong" from "it failed" from
        // "this is a bug in tuff" without reading the message.
        assert_eq!(TuffError::usage("bad flag").exit_code(), 2);
        assert_eq!(TuffError::new("invariant broken").exit_code(), 70);
        for error in [
            TuffError::not_found("x"),
            TuffError::refused("x"),
            TuffError::drift("x"),
            TuffError::source_failed("x"),
            TuffError::corrupt("x"),
            TuffError::unsupported("x"),
        ] {
            assert_eq!(error.exit_code(), 1, "{:?}", error.kind());
        }
    }

    #[test]
    fn a_hint_is_carried_separately_from_the_message() {
        let error =
            TuffError::drift("'x' has local changes").with_hint("use --force to replace it");
        assert_eq!(format!("{error}"), "'x' has local changes");
        assert_eq!(error.hint(), Some("use --force to replace it"));
        assert_eq!(error.kind().as_str(), "drift");
    }
}
