//! Credential redaction utilities
//!
//! This module provides utilities to prevent Azure credentials and other sensitive
//! information from appearing in logs. Per PRD security requirements, connection
//! strings, SAS tokens, and account keys must never be logged in plain text.

use regex::Regex;
use std::sync::OnceLock;
use tracing::warn;

/// Regex pattern for Azure connection strings
static CONNECTION_STRING_REGEX: OnceLock<Regex> = OnceLock::new();

/// Regex pattern for Azure SAS tokens
static SAS_TOKEN_REGEX: OnceLock<Regex> = OnceLock::new();

/// Regex pattern for SharedAccessSignature
static SHARED_ACCESS_SIG_REGEX: OnceLock<Regex> = OnceLock::new();

/// Initialize regex patterns for credential detection
fn get_connection_string_regex() -> &'static Regex {
    CONNECTION_STRING_REGEX.get_or_init(|| Regex::new(r"AccountKey=([^;]+)").unwrap())
}

fn get_sas_token_regex() -> &'static Regex {
    SAS_TOKEN_REGEX.get_or_init(|| {
        // Match SAS token parameters: sv=...&ss=...&srt=... etc
        Regex::new(r"\??(sv=[^&\s]+(&[^&\s]+)*)").unwrap()
    })
}

fn get_shared_access_sig_regex() -> &'static Regex {
    SHARED_ACCESS_SIG_REGEX.get_or_init(|| {
        // Match SharedAccessSignature= pattern
        Regex::new(r"SharedAccessSignature=[^;\s]+").unwrap()
    })
}

/// Redact Azure credentials from a string
///
/// This function replaces connection strings, SAS tokens, and account keys
/// with `***REDACTED***` to prevent credential leakage in logs.
///
/// # Examples
///
/// ```
/// use three_dgs_processor::logging::redact_credentials;
///
/// let input = "AccountKey=abc123key==;Other=value";
/// let output = redact_credentials(input);
/// assert!(output.contains("***REDACTED***"));
/// assert!(!output.contains("abc123key"));
/// ```
pub fn redact_credentials(text: &str) -> String {
    let mut result = text.to_string();

    // Redact connection string account keys
    let conn_regex = get_connection_string_regex();
    result = conn_regex
        .replace_all(&result, "AccountKey=***REDACTED***")
        .to_string();

    // Redact SharedAccessSignature parameters
    let shared_sig_regex = get_shared_access_sig_regex();
    result = shared_sig_regex
        .replace_all(&result, "SharedAccessSignature=***REDACTED***")
        .to_string();

    // Redact URL-style SAS tokens
    let sas_regex = get_sas_token_regex();
    result = sas_regex
        .replace_all(&result, "***REDACTED_SAS_TOKEN***")
        .to_string();

    // Redact standalone account keys (base64 format) - but be careful not to over-match
    // Only match if preceded by = or : to avoid false positives
    let key_pattern = Regex::new(r"[=:]\s*([A-Za-z0-9+/]{40,}==)").unwrap();
    result = key_pattern
        .replace_all(&result, "=***REDACTED_KEY***")
        .to_string();

    result
}

/// Check if a string contains potential credentials and log a warning
///
/// This function scans for patterns that look like credentials and logs
/// a warning if any are detected. Use this to catch accidental credential
/// exposure in error messages or debug output.
pub fn check_for_credentials(text: &str, context: &str) {
    let conn_regex = get_connection_string_regex();
    let sas_regex = get_sas_token_regex();
    let shared_sig_regex = get_shared_access_sig_regex();
    let key_pattern = Regex::new(r"[=:]\s*([A-Za-z0-9+/]{40,}==)").unwrap();

    if conn_regex.is_match(text)
        || sas_regex.is_match(text)
        || shared_sig_regex.is_match(text)
        || key_pattern.is_match(text)
    {
        warn!(
            context = %context,
            "Potential credential detected in output - this should be redacted"
        );
    }
}

/// Macro to skip credential fields in tracing instrumentation
///
/// Use this macro with `#[instrument]` to skip logging sensitive fields:
///
/// ```ignore
/// #[instrument(skip(conn_str, sas_token))]
/// async fn authenticate(conn_str: &str, sas_token: &str) -> Result<()> {
///     // Implementation
/// }
/// ```
#[macro_export]
macro_rules! skip_credentials {
    () => {
        [
            "connection_string",
            "conn_str",
            "sas_token",
            "account_key",
            "key",
        ]
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_connection_string() {
        let input = "DefaultEndpointsProtocol=https;AccountName=test;AccountKey=abc123def456ghi789==;EndpointSuffix=core.windows.net";
        let output = redact_credentials(input);

        assert!(output.contains("AccountKey=***REDACTED***"));
        assert!(!output.contains("abc123def456ghi789=="));
        assert!(output.contains("AccountName=test"));
    }

    #[test]
    fn test_redact_sas_token() {
        let input = "https://account.blob.core.windows.net/?sv=2021-06-08&ss=b&srt=sco&sp=rwdlac&se=2026-12-31T23:59:59Z";
        let output = redact_credentials(input);

        assert!(output.contains("***REDACTED_SAS_TOKEN***"));
        assert!(!output.contains("sv=2021-06-08"));
    }

    #[test]
    fn test_redact_account_key() {
        let key = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+/abcdefghij==";
        let input = format!("Using key: {}", key);
        let output = redact_credentials(&input);

        assert!(output.contains("***REDACTED_KEY***"));
        assert!(!output.contains(key));
    }

    #[test]
    fn test_no_redaction_needed() {
        let input = "This is a normal log message with no credentials";
        let output = redact_credentials(input);

        assert_eq!(input, output);
    }

    #[test]
    fn test_check_for_credentials_warning() {
        // This test verifies the function doesn't panic - actual warning testing
        // would require capturing tracing output
        let input = "AccountKey=secretkey==";
        check_for_credentials(input, "test_context");
    }

    #[test]
    fn test_multiple_credentials() {
        let input = "AccountKey=key1== and AccountKey=key2==";
        let output = redact_credentials(input);

        // Both AccountKey= sections should be redacted
        assert!(!output.contains("key1=="));
        assert!(!output.contains("key2=="));
        // Should have both AccountKey redacted
        assert!(output.contains("AccountKey=***REDACTED***"));
    }
}
