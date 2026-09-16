//! The TMDB API Read Access Token and where it is kept: the OS credential
//! store, never Bingee's own files (ADR-0009).

use std::fmt;

use crate::APP_ID;
use crate::error::{AppError, ErrorKind};

/// Account name of the one secret in the credential store.
const ACCOUNT: &str = "tmdb-api-read-access-token";

/// Where `KeyringStore` keeps the token, as the UI names it.
pub const STORE_NAME: &str = if cfg!(windows) {
    "Windows Credential Manager"
} else if cfg!(target_os = "macos") {
    "the macOS Keychain"
} else {
    "the Secret Service keyring"
};

/// A TMDB access token. `Debug` is redacted and there is no `Display`, so it
/// cannot end up in logs or error messages by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// A pasted token: surrounding whitespace removed. `None` if it is empty,
    /// contains whitespace or control characters, or is implausibly long.
    pub fn parse(input: &str) -> Option<Self> {
        let token = input.trim();
        let plausible = !token.is_empty()
            && token.len() <= 1024
            && !token.chars().any(|c| c.is_whitespace() || c.is_control());
        plausible.then(|| Self(token.to_owned()))
    }

    /// The secret itself, for the `Authorization` header only.
    pub fn secret(&self) -> &str {
        &self.0
    }

    /// The last four characters, the most the UI ever shows.
    pub fn hint(&self) -> String {
        let tail: Vec<char> = self.0.chars().rev().take(4).collect();
        format!("…{}", tail.into_iter().rev().collect::<String>())
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token([redacted])")
    }
}

/// The token in use right now, so features other than Discover (the detail
/// refresh) can see whether remote access is configured without owning the
/// credential lifecycle. Only `remote` writes it.
#[derive(Clone, Default)]
pub struct SharedToken(std::sync::Arc<std::sync::Mutex<Option<Token>>>);

impl SharedToken {
    pub fn set(&self, token: Option<Token>) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = token;
    }

    pub fn get(&self) -> Option<Token> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Where the token is kept. Calls may block (an unlock prompt on Linux), so
/// they run on the network workers, never on the UI thread.
pub trait SecretStore: Send + Sync {
    fn load(&self) -> Result<Option<Token>, AppError>;
    fn save(&self, token: &Token) -> Result<(), AppError>;
    /// Succeeds if there was nothing to delete.
    fn delete(&self) -> Result<(), AppError>;
}

/// The OS credential store: Windows Credential Manager, macOS Keychain, or
/// the Secret Service on Linux.
pub struct KeyringStore;

impl KeyringStore {
    fn entry() -> Result<keyring::Entry, AppError> {
        keyring::Entry::new(APP_ID, ACCOUNT).map_err(unavailable)
    }
}

impl SecretStore for KeyringStore {
    fn load(&self) -> Result<Option<Token>, AppError> {
        match Self::entry()?.get_password() {
            Ok(secret) => Ok(Token::parse(&secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(unavailable(err)),
        }
    }

    fn save(&self, token: &Token) -> Result<(), AppError> {
        Self::entry()?
            .set_password(token.secret())
            .map_err(unavailable)
    }

    fn delete(&self) -> Result<(), AppError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(unavailable(err)),
        }
    }
}

/// Keeps only the error's `Display` text: some variants carry the stored
/// bytes, which `Debug` would print.
fn unavailable(err: keyring::Error) -> AppError {
    AppError::new(
        ErrorKind::Configuration,
        "The system's secure credential storage is not available.",
    )
    .with_source(err.to_string())
}

/// An in-memory store for tests. `failing` simulates an unavailable store.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryStore {
    pub token: std::sync::Mutex<Option<Token>>,
    pub failing: std::sync::atomic::AtomicBool,
}

#[cfg(test)]
impl MemoryStore {
    pub fn with(token: &str) -> Self {
        let store = Self::default();
        *store.token.lock().unwrap() = Token::parse(token);
        store
    }

    pub fn current(&self) -> Option<String> {
        self.token.lock().unwrap().as_ref().map(|t| t.0.clone())
    }

    fn check(&self) -> Result<(), AppError> {
        match self.failing.load(std::sync::atomic::Ordering::SeqCst) {
            true => Err(AppError::new(ErrorKind::Configuration, "No store.")),
            false => Ok(()),
        }
    }
}

#[cfg(test)]
impl SecretStore for MemoryStore {
    fn load(&self) -> Result<Option<Token>, AppError> {
        self.check()?;
        Ok(self.token.lock().unwrap().clone())
    }

    fn save(&self, token: &Token) -> Result<(), AppError> {
        self.check()?;
        *self.token.lock().unwrap() = Some(token.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), AppError> {
        self.check()?;
        *self.token.lock().unwrap() = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_never_printed() {
        let token = Token::parse("  eyJhbGciOiJIUzI1NiJ9.secret-part.sig1234 \n").unwrap();
        assert_eq!(token.secret(), "eyJhbGciOiJIUzI1NiJ9.secret-part.sig1234");
        assert_eq!(format!("{token:?}"), "Token([redacted])");
        assert_eq!(format!("{:?}", Some(&token)), "Some(Token([redacted]))");
        assert_eq!(token.hint(), "…1234");
        assert_eq!(Token::parse("ab").unwrap().hint(), "…ab");
    }

    #[test]
    fn implausible_input_is_not_a_token() {
        for input in ["", "   ", "two words", "tab\there", &"x".repeat(1025)] {
            assert!(Token::parse(input).is_none(), "{input:?}");
        }
    }

    #[test]
    fn keyring_errors_keep_only_their_display_text() {
        let error = unavailable(keyring::Error::BadEncoding(b"secret-bytes".to_vec()));
        assert_eq!(error.kind, ErrorKind::Configuration);
        let logged = error.to_string();
        assert!(!logged.contains("secret-bytes"), "{logged}");
        assert!(!format!("{error:?}").contains("secret"), "{error:?}");
    }

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryStore::default();
        assert_eq!(store.load().unwrap(), None);
        store.save(&Token::parse("abc").unwrap()).unwrap();
        assert_eq!(store.current().as_deref(), Some("abc"));
        store.delete().unwrap();
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }
}
