use regex::Regex;
use std::env;
use std::sync::LazyLock;

static ENV_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    // Matches ${VAR} and ${VAR:-default}
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}").expect("env var regex is valid")
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
            let default_value = caps.get(2).map_or("", |m| m.as_str());
            env::var(var_name).unwrap_or_else(|_| default_value.to_string())
        })
        .into_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_resolve_existing_var() {
        env::set_var("HERD_TEST_VAR", "hello");
        assert_eq!(resolve("${HERD_TEST_VAR}"), "hello");
        env::remove_var("HERD_TEST_VAR");
    }

    #[test]
    fn test_resolve_missing_var_empty() {
        env::remove_var("HERD_NONEXISTENT");
        assert_eq!(resolve("${HERD_NONEXISTENT}"), "");
    }

    #[test]
    fn test_resolve_missing_var_with_default() {
        env::remove_var("HERD_NONEXISTENT");
        assert_eq!(resolve("${HERD_NONEXISTENT:-fallback}"), "fallback");
    }

    #[test]
    fn test_resolve_mixed_content() {
        env::set_var("HERD_HOST", "localhost");
        assert_eq!(
            resolve("http://${HERD_HOST}:${HERD_PORT:-8080}/api"),
            "http://localhost:8080/api"
        );
        env::remove_var("HERD_HOST");
    }

    #[test]
    fn test_resolve_no_vars() {
        assert_eq!(resolve("plain string"), "plain string");
    }

    #[test]
    fn test_resolve_existing_var_ignores_default() {
        env::set_var("HERD_SET_VAR", "actual");
        assert_eq!(resolve("${HERD_SET_VAR:-default}"), "actual");
        env::remove_var("HERD_SET_VAR");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Resolver must never panic on arbitrary input strings.
        #[test]
        fn resolve_never_panics(input in "\\PC{0,256}") {
            let _ = resolve(&input);
        }

        /// Strings without ${} patterns pass through unchanged.
        #[test]
        fn no_vars_passthrough(input in "[a-zA-Z0-9 ,.!?/:-]{0,128}") {
            // Only if input doesn't contain ${ ... }
            if !input.contains("${") {
                prop_assert_eq!(resolve(&input), input);
            }
        }

        /// Output never contains unresolved ${VAR} patterns
        /// (they resolve to empty string or default).
        #[test]
        fn output_has_no_unresolved_vars(
            var_name in "[A-Z_]{1,8}",
            default in "[a-z]{0,8}"
        ) {
            let input = format!("${{{var_name}:-{default}}}");
            let output = resolve(&input);
            prop_assert!(!output.contains("${"), "output still contains ${{: {output}");
        }
    }
}
