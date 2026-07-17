#[derive(Debug)]
pub enum ConfigError {
    Missing(String),
    Invalid {
        key: String,
        value: String,
        reason: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Missing(key) => write!(f, "missing required env var: {key}"),
            ConfigError::Invalid { key, value, reason } => {
                write!(f, "invalid env var {key}={value:?}: {reason}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// Read an optional env var. Absent or empty -> `None`.
pub fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// Read a required env var. Absent or empty -> `ConfigError::Missing`.
pub fn env_req(key: &str) -> Result<String, ConfigError> {
    env_opt(key).ok_or_else(|| ConfigError::Missing(key.to_string()))
}

/// Read an env var or fall back to `default`. Absent or empty -> `default`.
pub fn env_or(key: &str, default: &str) -> String {
    env_opt(key).unwrap_or_else(|| default.to_string())
}

/// Parse an env var into `T`; absent or empty -> `default`, unparseable ->
/// `ConfigError::Invalid` carrying the parser's own error message.
pub fn env_parse<T>(key: &str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match env_opt(key) {
        Some(v) => v.parse::<T>().map_err(|e| ConfigError::Invalid {
            key: key.to_string(),
            value: v,
            reason: e.to_string(),
        }),
        None => Ok(default),
    }
}

/// Load `KEY=VALUE` pairs from a `.env` file into the process environment.
///
/// Skips blank lines and `#` comments, trims whitespace, strips a single pair
/// of surrounding quotes, and never overrides a variable that is already set.
pub fn load_dotenv_from(path: &std::path::Path) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if key.is_empty() || std::env::var_os(key).is_some() {
            continue;
        }
        // Safe: callers invoke this before the async runtime spawns threads
        // (see api `main`), or from a nextest-isolated single-thread process.
        unsafe { std::env::set_var(key, value) };
    }
}

#[cfg(test)]
mod env_helper_tests {
    use super::*;

    #[test]
    fn env_req_missing_errors() {
        unsafe { std::env::remove_var("T_REQ_MISSING") };
        let err = env_req("T_REQ_MISSING").unwrap_err();
        assert!(matches!(err, ConfigError::Missing(k) if k == "T_REQ_MISSING"));
    }

    #[test]
    fn env_req_present_ok() {
        unsafe { std::env::set_var("T_REQ_PRESENT", "hello") };
        assert_eq!(env_req("T_REQ_PRESENT").unwrap(), "hello");
    }

    #[test]
    fn env_opt_absent_is_none() {
        unsafe { std::env::remove_var("T_OPT_ABSENT") };
        assert_eq!(env_opt("T_OPT_ABSENT"), None);
    }

    #[test]
    fn env_opt_empty_is_none() {
        unsafe { std::env::set_var("T_OPT_EMPTY", "") };
        assert_eq!(env_opt("T_OPT_EMPTY"), None);
    }

    #[test]
    fn env_or_uses_default_when_absent() {
        unsafe { std::env::remove_var("T_OR_ABSENT") };
        assert_eq!(env_or("T_OR_ABSENT", "def"), "def");
    }

    #[test]
    fn empty_is_treated_as_absent_across_readers() {
        unsafe { std::env::set_var("T_EMPTY", "") };
        // env_or / env_parse fall back to default; env_req errors as missing.
        assert_eq!(env_or("T_EMPTY", "def"), "def");
        assert_eq!(env_parse::<u64>("T_EMPTY", 7).unwrap(), 7);
        assert!(
            matches!(env_req("T_EMPTY").unwrap_err(), ConfigError::Missing(k) if k == "T_EMPTY")
        );
    }

    #[test]
    fn env_parse_invalid_carries_parser_message() {
        unsafe { std::env::set_var("T_PARSE_MSG", "notanum") };
        let err = env_parse::<u64>("T_PARSE_MSG", 0).unwrap_err();
        match err {
            ConfigError::Invalid { reason, .. } => {
                // Real parser message, not a hardcoded placeholder.
                assert!(reason.contains("invalid digit"), "got: {reason}");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn env_parse_default_and_coerce() {
        unsafe { std::env::remove_var("T_PARSE_ABSENT") };
        assert_eq!(env_parse::<u64>("T_PARSE_ABSENT", 10).unwrap(), 10);

        unsafe { std::env::set_var("T_PARSE_NUM", "42") };
        assert_eq!(env_parse::<u64>("T_PARSE_NUM", 10).unwrap(), 42);

        unsafe { std::env::set_var("T_PARSE_BOOL", "true") };
        assert!(env_parse::<bool>("T_PARSE_BOOL", false).unwrap());

        unsafe { std::env::set_var("T_PARSE_BAD", "notanum") };
        let err = env_parse::<u64>("T_PARSE_BAD", 10).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid { key, .. } if key == "T_PARSE_BAD"));
    }
}

#[cfg(test)]
mod dotenv_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_skips_comments_and_respects_existing() {
        // Pre-set a var that must NOT be overridden.
        unsafe { std::env::set_var("DOTENV_EXISTING", "keep") };
        unsafe { std::env::remove_var("DOTENV_NEW") };
        unsafe { std::env::remove_var("DOTENV_QUOTED") };

        let dir = std::env::temp_dir().join(format!("dotenv_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".env");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "# a comment").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "DOTENV_EXISTING=override_attempt").unwrap();
        writeln!(f, "DOTENV_NEW=fresh").unwrap();
        writeln!(f, "DOTENV_QUOTED=\"quoted value\"").unwrap();
        f.flush().unwrap();

        load_dotenv_from(&path);

        assert_eq!(std::env::var("DOTENV_EXISTING").unwrap(), "keep"); // not overridden
        assert_eq!(std::env::var("DOTENV_NEW").unwrap(), "fresh");
        assert_eq!(std::env::var("DOTENV_QUOTED").unwrap(), "quoted value");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_is_noop() {
        load_dotenv_from(std::path::Path::new("/nonexistent/path/.env"));
        // no panic == pass
    }
}
