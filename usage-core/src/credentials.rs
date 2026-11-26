use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CredError {
    #[error("Claude Code credentials not found")]
    NotFound,
    #[error("Claude Code credentials are malformed")]
    Malformed,
    #[error("Claude Code access token has expired")]
    Expired,
}

/// Extract the access token from Claude Code's credentials JSON
/// (`{"claudeAiOauth": {"accessToken": "...", "expiresAt": <epoch ms>}}`).
/// A missing `expiresAt` is treated as not expired.
pub fn parse_credentials(json: &str, now: DateTime<Utc>) -> Result<String, CredError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| CredError::Malformed)?;
    let oauth = value.get("claudeAiOauth").ok_or(CredError::Malformed)?;
    let token = oauth
        .get("accessToken")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .ok_or(CredError::Malformed)?;
    if let Some(expires_at) = oauth.get("expiresAt").and_then(|e| e.as_i64()) {
        if expires_at <= now.timestamp_millis() {
            return Err(CredError::Expired);
        }
    }
    Ok(token.to_string())
}

/// Read the token from the credentials file at `path`. Any failure to read the
/// file is reported as `NotFound`.
#[cfg(not(target_os = "macos"))]
fn read_from_file(path: &std::path::Path, now: DateTime<Utc>) -> Result<String, CredError> {
    let json = std::fs::read_to_string(path).map_err(|_| CredError::NotFound)?;
    parse_credentials(&json, now)
}

/// Read Claude Code's access token (read-only; the token is never logged).
#[cfg(target_os = "windows")]
pub fn read_access_token(now: DateTime<Utc>) -> Result<String, CredError> {
    let profile = std::env::var_os("USERPROFILE").ok_or(CredError::NotFound)?;
    let path = std::path::PathBuf::from(profile).join(".claude").join(".credentials.json");
    read_from_file(&path, now)
}

/// Read Claude Code's access token from the macOS Keychain (read-only).
#[cfg(target_os = "macos")]
pub fn read_access_token(now: DateTime<Utc>) -> Result<String, CredError> {
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .map_err(|_| CredError::NotFound)?;
    if !out.status.success() {
        return Err(CredError::NotFound);
    }
    let json = String::from_utf8(out.stdout).map_err(|_| CredError::Malformed)?;
    parse_credentials(&json, now)
}

/// Other platforms (Linux; only used for local compile checks).
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn read_access_token(now: DateTime<Utc>) -> Result<String, CredError> {
    let home = std::env::var_os("HOME").ok_or(CredError::NotFound)?;
    let path = std::path::PathBuf::from(home).join(".claude").join(".credentials.json");
    read_from_file(&path, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 22, 4, 0, 0).unwrap()
    }

    #[test]
    fn valid_returns_token() {
        let exp = now().timestamp_millis() + 3_600_000;
        let j = format!(r#"{{"claudeAiOauth":{{"accessToken":"tok-123","expiresAt":{exp},"other":1}}}}"#);
        assert_eq!(parse_credentials(&j, now()), Ok("tok-123".to_string()));
    }

    #[test]
    fn missing_expires_at_is_not_expired() {
        let j = r#"{"claudeAiOauth":{"accessToken":"tok"}}"#;
        assert_eq!(parse_credentials(j, now()), Ok("tok".to_string()));
    }

    #[test]
    fn missing_key_is_malformed() {
        assert_eq!(parse_credentials(r#"{"other":{}}"#, now()), Err(CredError::Malformed));
        assert_eq!(parse_credentials(r#"{"claudeAiOauth":{}}"#, now()), Err(CredError::Malformed));
        assert_eq!(parse_credentials("not json", now()), Err(CredError::Malformed));
    }

    #[test]
    fn past_expiry_is_expired() {
        let exp = now().timestamp_millis() - 1;
        let j = format!(r#"{{"claudeAiOauth":{{"accessToken":"tok","expiresAt":{exp}}}}}"#);
        assert_eq!(parse_credentials(&j, now()), Err(CredError::Expired));
    }

    #[test]
    fn empty_string_is_malformed() {
        assert_eq!(parse_credentials("", now()), Err(CredError::Malformed));
        assert_eq!(parse_credentials("  \n", now()), Err(CredError::Malformed));
    }

    #[test]
    fn empty_token_is_malformed() {
        let j = r#"{"claudeAiOauth":{"accessToken":""}}"#;
        assert_eq!(parse_credentials(j, now()), Err(CredError::Malformed));
    }
}
