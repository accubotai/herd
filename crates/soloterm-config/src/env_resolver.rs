use regex::Regex;
use std::env;
use std::sync::LazyLock;

static ENV_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // Matches ${VAR} and ${VAR:-default}
    Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}").unwrap()
});

/// Resolve environment variable references in a string.
///
/// Supports:
/// - `${VAR}` — replaced with the value of VAR, or empty string if unset
/// - `${VAR:-default}` — replaced with the value of VAR, or "default" if unset
pub fn resolve(input: &str) -> String {
    ENV_PATTERN
        .replace_all(input, |caps: &regex::Captures| {
            let var_name = &caps[1];
            let default_value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            env::var(var_name).unwrap_or_else(|_| default_value.to_string())
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_resolve_existing_var() {
        env::set_var("SOLOTERM_TEST_VAR", "hello");
        assert_eq!(resolve("${SOLOTERM_TEST_VAR}"), "hello");
        env::remove_var("SOLOTERM_TEST_VAR");
    }

    #[test]
    fn test_resolve_missing_var_empty() {
        env::remove_var("SOLOTERM_NONEXISTENT");
        assert_eq!(resolve("${SOLOTERM_NONEXISTENT}"), "");
    }

    #[test]
    fn test_resolve_missing_var_with_default() {
        env::remove_var("SOLOTERM_NONEXISTENT");
        assert_eq!(resolve("${SOLOTERM_NONEXISTENT:-fallback}"), "fallback");
    }

    #[test]
    fn test_resolve_mixed_content() {
        env::set_var("SOLOTERM_HOST", "localhost");
        assert_eq!(
            resolve("http://${SOLOTERM_HOST}:${SOLOTERM_PORT:-8080}/api"),
            "http://localhost:8080/api"
        );
        env::remove_var("SOLOTERM_HOST");
    }

    #[test]
    fn test_resolve_no_vars() {
        assert_eq!(resolve("plain string"), "plain string");
    }

    #[test]
    fn test_resolve_existing_var_ignores_default() {
        env::set_var("SOLOTERM_SET_VAR", "actual");
        assert_eq!(resolve("${SOLOTERM_SET_VAR:-default}"), "actual");
        env::remove_var("SOLOTERM_SET_VAR");
    }
}
