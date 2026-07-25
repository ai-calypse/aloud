//! Reading the Claude Code OAuth access token from the macOS login Keychain.
//!
//! Claude Code stores its credentials as a generic-password item under the
//! service `"Claude Code-credentials"`. The password blob is JSON shaped
//! `{ "claudeAiOauth": { "accessToken": "…", "expiresAt": … } }` — we want the
//! access token. This is the same item and shape notchi's `KeychainManager`
//! reads.
//!
//! Reading another app's Keychain item triggers a macOS access-control prompt
//! the first time (and, depending on the ACL, on later reads). That prompt is
//! the OS's, and the user's to accept — nothing here can or should suppress it.

use crate::QuotaError;

/// The generic-password service name Claude Code writes under.
#[cfg(target_os = "macos")]
const SERVICE: &str = "Claude Code-credentials";

/// The Claude Code OAuth access token, or an error saying why not.
/// A process-lifetime cache of the read token. The Keychain read triggers a
/// macOS access prompt; caching means it fires at most ONCE per app run, not on
/// every poll — the loop of prompts came from re-reading each cycle. `poison()`
/// clears it when the API rejects the token, so a refresh re-reads exactly once.
#[cfg(target_os = "macos")]
static CACHED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[cfg(target_os = "macos")]
pub fn claude_token() -> Result<String, QuotaError> {
    if let Some(token) = CACHED.lock().ok().and_then(|c| c.clone()) {
        return Ok(token);
    }
    let token = read_token_from_keychain()?;
    if let Ok(mut cache) = CACHED.lock() {
        *cache = Some(token.clone());
    }
    Ok(token)
}

/// Forget the cached token, so the next [`claude_token`] re-reads the Keychain.
/// Called after the API rejects the token — it may have been refreshed by Claude
/// Code. One re-prompt then, not a loop.
#[cfg(target_os = "macos")]
pub fn poison() {
    if let Ok(mut cache) = CACHED.lock() {
        *cache = None;
    }
}

#[cfg(target_os = "macos")]
fn read_token_from_keychain() -> Result<String, QuotaError> {
    use security_framework::passwords::get_generic_password;

    // ONE query, with the login account. Trying several account names meant
    // several failed reads, and a wrong-account read can raise its own prompt —
    // multiplying the dialog. The item's account is the macOS login user.
    let account = std::env::var("USER").map_err(|_| QuotaError::NoToken)?;
    let blob = get_generic_password(SERVICE, &account).map_err(|_| QuotaError::NoToken)?;

    let json: serde_json::Value = serde_json::from_slice(&blob).map_err(|_| QuotaError::NoToken)?;
    json.get("claudeAiOauth")
        .and_then(|o| o.get("accessToken"))
        .and_then(|t| t.as_str())
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .ok_or(QuotaError::NoToken)
}

#[cfg(not(target_os = "macos"))]
pub fn poison() {}

#[cfg(not(target_os = "macos"))]
pub fn claude_token() -> Result<String, QuotaError> {
    Err(QuotaError::Unsupported)
}
