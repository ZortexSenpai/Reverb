use reverb_core::subsonic::SubsonicError;
use serde::ser::SerializeStruct;

/// Command-level error, serialized to the frontend as `{ code, message }`.
#[derive(Debug, thiserror::Error)]
pub enum ReverbError {
    #[error(transparent)]
    Subsonic(#[from] SubsonicError),
    #[error("keychain: {0}")]
    Keyring(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    State(String),
}

impl ReverbError {
    fn code(&self) -> &'static str {
        match self {
            // Subsonic error 40 = wrong credentials; the frontend's session
            // gate watches for the "auth" code.
            ReverbError::Subsonic(SubsonicError::Api { code: 40..=44, .. }) => "auth",
            ReverbError::Subsonic(SubsonicError::NotConfigured) => "not-configured",
            ReverbError::Subsonic(SubsonicError::BadUrl(_)) => "bad-url",
            ReverbError::Subsonic(SubsonicError::Http(_)) => "network",
            ReverbError::Subsonic(_) => "server",
            ReverbError::Keyring(_) => "keychain",
            ReverbError::Io(_) => "io",
            ReverbError::State(_) => "state",
        }
    }
}

impl serde::Serialize for ReverbError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("ReverbError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<std::io::Error> for ReverbError {
    fn from(e: std::io::Error) -> Self {
        ReverbError::Io(e.to_string())
    }
}
