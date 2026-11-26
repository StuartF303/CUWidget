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

const EPOCH_SECONDS_LIMIT: i64 = 100_000_000_000;

/// Extract the access token from Claude Code's credentials JSON
/// (`{"claudeAiOauth": {"accessToken": "...", "expiresAt": <epoch ms>}}`).
/// A missing `expiresAt` is treated as not expired. An `expiresAt` below
/// 100_000_000_000 is taken to be epoch seconds, otherwise epoch milliseconds.
pub fn parse_credentials(json: &str, now: DateTime<Utc>) -> Result<String, CredError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| CredError::Malformed)?;
    let oauth = value.get("claudeAiOauth").ok_or(CredError::Malformed)?;
    let token = oauth
        .get("accessToken")
        .and_then(|t| t.as_str())
        .filter(|t| !t.is_empty())
        .ok_or(CredError::Malformed)?;
    if let Some(expires_at) = oauth.get("expiresAt").and_then(|e| e.as_i64()) {
        let expires_ms = if expires_at < EPOCH_SECONDS_LIMIT { expires_at.saturating_mul(1000) } else { expires_at };
        if expires_ms <= now.timestamp_millis() {
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

/// Deadline for the Keychain read; the poller enforces a slightly longer one.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const KEYCHAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// Run `cmd` and return its stdout if it exits successfully within `timeout`.
/// On expiry the child is killed and reaped, so none is left behind. Failure
/// to start, a non-zero exit or a timeout are all `NotFound`.
#[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
fn run_with_deadline(mut cmd: std::process::Command, timeout: std::time::Duration) -> Result<Vec<u8>, CredError> {
    use std::io::Read;
    use std::process::Stdio;

    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| CredError::NotFound)?;
    let mut stdout = child.stdout.take().ok_or(CredError::NotFound)?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });

    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    match status {
        // The reader ends when the pipe closes. After a kill it is detached
        // instead of joined, in case a grandchild still holds the pipe open.
        Some(s) if s.success() => reader.join().map_err(|_| CredError::NotFound),
        _ => Err(CredError::NotFound),
    }
}

/// Read Claude Code's access token from the macOS Keychain (read-only).
/// Uses the absolute path of `security` and a deadline, so a locked keychain
/// or a permission prompt cannot hang the caller.
#[cfg(target_os = "macos")]
pub fn read_access_token(now: DateTime<Utc>) -> Result<String, CredError> {
    let mut cmd = std::process::Command::new("/usr/bin/security");
    cmd.args(["find-generic-password", "-s", "Claude Code-credentials", "-w"]);
    let stdout = run_with_deadline(cmd, KEYCHAIN_TIMEOUT)?;
    let json = String::from_utf8(stdout).map_err(|_| CredError::Malformed)?;
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
    fn expires_at_in_epoch_seconds() {
        let future = now().timestamp() + 3600;
        let j = format!(r#"{{"claudeAiOauth":{{"accessToken":"tok","expiresAt":{future}}}}}"#);
        assert_eq!(parse_credentials(&j, now()), Ok("tok".to_string()));
        let past = now().timestamp() - 1;
        let j = format!(r#"{{"claudeAiOauth":{{"accessToken":"tok","expiresAt":{past}}}}}"#);
        assert_eq!(parse_credentials(&j, now()), Err(CredError::Expired));
    }

    #[test]
    fn expires_at_unit_boundary() {
        // 99_999_999_999 is seconds (year 5138, in the future); 100_000_000_000 is
        // milliseconds (1973, long past).
        let j = r#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":99999999999}}"#;
        assert_eq!(parse_credentials(j, now()), Ok("tok".to_string()));
        let j = r#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":100000000000}}"#;
        assert_eq!(parse_credentials(j, now()), Err(CredError::Expired));
    }

    #[cfg(unix)]
    #[test]
    fn deadline_returns_output_of_fast_command() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "printf hello"]);
        assert_eq!(run_with_deadline(cmd, std::time::Duration::from_secs(5)), Ok(b"hello".to_vec()));
    }

    #[cfg(unix)]
    #[test]
    fn deadline_kills_slow_command_and_failure_is_not_found() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "sleep 30"]);
        let started = std::time::Instant::now();
        assert_eq!(run_with_deadline(cmd, std::time::Duration::from_millis(200)), Err(CredError::NotFound));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));

        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "exit 3"]);
        assert_eq!(run_with_deadline(cmd, std::time::Duration::from_secs(5)), Err(CredError::NotFound));
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
