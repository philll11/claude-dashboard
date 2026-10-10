//! Claude Code's login, read without changing it.

use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::roots::{DataRoot, RootSource};

/// The file in Claude Code's configuration folder that holds its login: on Linux and Windows,
/// and on macOS when the Keychain cannot be written.
const CREDENTIALS_FILE: &str = ".credentials.json";

/// Claude Code's login, as far as the usage endpoint needs it. Its token is never shown.
#[derive(Clone, PartialEq, Eq)]
pub struct Login {
    token: String,
    /// When the login expires, in milliseconds since the Unix epoch.
    expires_ms: Option<i64>,
}

impl Login {
    /// When the login expires, in milliseconds since the Unix epoch.
    #[must_use]
    pub fn expires_ms(&self) -> Option<i64> {
        self.expires_ms
    }

    /// Whether the login has expired, so that only Claude Code can renew it.
    #[must_use]
    pub fn expired(&self, now_ms: i64) -> bool {
        self.expires_ms.is_some_and(|expires| expires <= now_ms)
    }

    pub(super) fn token(&self) -> &str {
        &self.token
    }
}

impl fmt::Debug for Login {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Login")
            .field("token", &"(hidden)")
            .field("expires_ms", &self.expires_ms)
            .finish()
    }
}

/// Why Claude Code's login cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoginError {
    /// No claude.ai login, such as when Claude Code uses an API key or has never signed in.
    #[error("Claude Code has no claude.ai login on this computer")]
    Missing,
    #[error("Claude Code's login cannot be read: {0}")]
    Unreadable(String),
}

/// Where Claude Code keeps the login of one configuration folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginSource {
    folder: PathBuf,
    /// The `CLAUDE_CONFIG_DIR` value that names the folder, which the macOS Keychain entry is
    /// keyed to; `None` for the default folder.
    config_dir: Option<String>,
}

impl LoginSource {
    /// The login of Claude Code's own configuration folder: the first data root that
    /// `CLAUDE_CONFIG_DIR` or the default names, not one the person added, such as a WSL home.
    #[must_use]
    pub fn of(roots: &[DataRoot], config_dir: Option<&OsStr>) -> Option<Self> {
        let root = roots.iter().find(|root| root.source != RootSource::Added)?;
        let config_dir = match root.source {
            RootSource::Environment => config_dir
                .and_then(OsStr::to_str)
                .and_then(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .find(|part| !part.is_empty())
                })
                .map(str::to_owned),
            RootSource::Default | RootSource::Added => None,
        };
        Some(Self {
            folder: root.path.clone(),
            config_dir,
        })
    }

    /// Claude Code's configuration folder.
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// Reads the login without changing it. On macOS the Keychain asks the person, the first
    /// time, whether claude-dashboard may read it.
    ///
    /// # Errors
    ///
    /// [`LoginError`] when there is no login or it cannot be read.
    pub fn read(&self) -> Result<Login, LoginError> {
        stored(self)
    }

    /// The macOS Keychain entry Claude Code keeps this folder's login in: one of its own for each
    /// `CLAUDE_CONFIG_DIR`, named after the start of the value's SHA-256 hash.
    #[must_use]
    pub fn keychain_service(&self) -> String {
        const SERVICE: &str = "Claude Code-credentials";
        match &self.config_dir {
            None => SERVICE.to_owned(),
            Some(config_dir) => {
                let digest = ring::digest::digest(&ring::digest::SHA256, config_dir.as_bytes());
                let start = digest
                    .as_ref()
                    .iter()
                    .take(4)
                    .fold(0_u32, |start, byte| start << 8 | u32::from(*byte));
                format!("{SERVICE}-{start:08x}")
            }
        }
    }

    fn credentials_file(&self) -> PathBuf {
        self.folder.join(CREDENTIALS_FILE)
    }
}

/// Claude Code keeps its login in the Keychain, and in the file only when the Keychain rejected a
/// write, so either can be the older one.
#[cfg(target_os = "macos")]
fn stored(source: &LoginSource) -> Result<Login, LoginError> {
    let keychain = keychain::read(&source.keychain_service());
    match (keychain, read_file(&source.credentials_file())) {
        (Ok(keychain), Ok(file)) => Ok(
            if file.expires_ms.unwrap_or(i64::MIN) > keychain.expires_ms.unwrap_or(i64::MIN) {
                file
            } else {
                keychain
            },
        ),
        (Ok(login), Err(_)) | (Err(_), Ok(login)) => Ok(login),
        (Err(LoginError::Missing), Err(error)) | (Err(error), Err(_)) => Err(error),
    }
}

#[cfg(not(target_os = "macos"))]
fn stored(source: &LoginSource) -> Result<Login, LoginError> {
    read_file(&source.credentials_file())
}

fn read_file(path: &Path) -> Result<Login, LoginError> {
    match std::fs::read(path) {
        Ok(bytes) => parse(&bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(LoginError::Missing),
        Err(error) => Err(LoginError::Unreadable(error.to_string())),
    }
}

/// Reads the login from what Claude Code stores: `{"claudeAiOauth": {"accessToken": …,
/// "expiresAt": …}}`. Nothing of the content goes into an error.
fn parse(bytes: &[u8]) -> Result<Login, LoginError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| LoginError::Unreadable("it is not in a form this version knows".to_owned()))?;
    let oauth = value.get("claudeAiOauth").ok_or(LoginError::Missing)?;
    let token = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or(LoginError::Missing)?;
    Ok(Login {
        token: token.to_owned(),
        expires_ms: oauth.get("expiresAt").and_then(Value::as_i64),
    })
}

#[cfg(target_os = "macos")]
mod keychain {
    use security_framework::item::{ItemClass, ItemSearchOptions, SearchResult};

    use super::{Login, LoginError, parse};

    /// No such item.
    const NOT_FOUND: i32 = -25_300;
    /// The person denied access when the Keychain asked.
    const DENIED: [i32; 2] = [-25_293, -128];

    pub(super) fn read(service: &str) -> Result<Login, LoginError> {
        let found = ItemSearchOptions::new()
            .class(ItemClass::generic_password())
            .service(service)
            .load_data(true)
            .limit(1)
            .search();
        match found {
            Ok(results) => results
                .into_iter()
                .find_map(|result| match result {
                    SearchResult::Data(bytes) => Some(parse(&bytes)),
                    _ => None,
                })
                .unwrap_or(Err(LoginError::Missing)),
            Err(error) if error.code() == NOT_FOUND => Err(LoginError::Missing),
            Err(error) if DENIED.contains(&error.code()) => Err(LoginError::Unreadable(
                "the Keychain did not allow claude-dashboard to read it".to_owned(),
            )),
            Err(error) => {
                Err(LoginError::Unreadable(error.message().unwrap_or_else(
                    || format!("Keychain error {}", error.code()),
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(config_dir: Option<&str>) -> LoginSource {
        LoginSource {
            folder: PathBuf::new(),
            config_dir: config_dir.map(str::to_owned),
        }
    }

    #[test]
    fn the_default_folder_uses_claude_codes_own_keychain_entry() {
        assert_eq!(source(None).keychain_service(), "Claude Code-credentials");
    }

    #[test]
    fn another_folder_uses_an_entry_named_after_its_hash() {
        // The first 8 hex digits of SHA-256("/Users/you/.claude-work").
        assert_eq!(
            source(Some("/Users/you/.claude-work")).keychain_service(),
            "Claude Code-credentials-f7aef36f"
        );
    }

    #[test]
    fn a_login_is_read_without_showing_its_token() {
        let login = parse(
            br#"{"claudeAiOauth": {"accessToken": "made-up-token", "expiresAt": 1760000000000,
                "refreshToken": "made-up-refresh", "scopes": ["user:profile"]}}"#,
        )
        .unwrap();

        assert_eq!(login.token(), "made-up-token");
        assert_eq!(login.expires_ms(), Some(1_760_000_000_000));
        assert!(login.expired(1_760_000_000_000));
        assert!(!login.expired(1_759_999_999_999));
        assert!(!format!("{login:?}").contains("made-up"));
    }

    #[test]
    fn credentials_without_a_claude_ai_login_have_no_login() {
        assert_eq!(parse(br#"{"apiKey": "made-up"}"#), Err(LoginError::Missing));
        assert_eq!(
            parse(br#"{"claudeAiOauth": {"accessToken": ""}}"#),
            Err(LoginError::Missing)
        );
    }

    #[test]
    fn unreadable_credentials_never_quote_their_content() {
        let error = parse(b"made-up-token-in-a-broken-file").unwrap_err();
        assert!(matches!(error, LoginError::Unreadable(_)));
        assert!(!error.to_string().contains("made-up"));
    }
}
