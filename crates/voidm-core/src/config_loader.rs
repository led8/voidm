//! Configuration loading with hierarchy support
//!
//! Priority order (lowest to highest):
//! 1. File (config.toml)
//! 2. Environment variables (VOIDM_*)
//! 3. CLI arguments (overrides both file and env)
//!
//! Env variable naming: VOIDM_SECTION_SUBSECTION_PARAM
//! Examples:
//! - VOIDM_DATABASE_BACKEND → database.backend
//! - VOIDM_SEARCH_MODE → search.mode
//! - VOIDM_SEARCH_RERANKER_ENABLED → search.reranker.enabled

use std::env;

/// Helper for parsing environment variables with VOIDM_ prefix
pub struct EnvHelper;

impl EnvHelper {
    const PREFIX: &'static str = "VOIDM_";

    /// Get an environment variable with VOIDM_ prefix
    /// Example: get("SEARCH_MODE") reads VOIDM_SEARCH_MODE
    pub fn get(key: &str) -> Option<String> {
        env::var(format!("{}{}", Self::PREFIX, key)).ok()
    }

    /// Parse bool from env (true, 1, yes | false, 0, no)
    pub fn get_bool(key: &str) -> Option<bool> {
        Self::get(key).and_then(|v| match v.to_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        })
    }

    /// Parse usize from env
    pub fn get_usize(key: &str) -> Option<usize> {
        Self::get(key).and_then(|v| v.parse().ok())
    }

    /// Parse f32 from env
    pub fn get_f32(key: &str) -> Option<f32> {
        Self::get(key).and_then(|v| v.parse().ok())
    }

    /// Parse Vec<String> from env (comma-separated)
    pub fn get_vec_string(key: &str) -> Option<Vec<String>> {
        Self::get(key).map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
    }

    /// List all VOIDM_ environment variables (for debugging)
    pub fn list_all() -> Vec<(String, String)> {
        env::vars()
            .filter(|(k, _)| k.starts_with(Self::PREFIX))
            .collect()
    }
}

/// Trait for configs that can be merged from environment variables
pub trait MergeFromEnv: Sized {
    /// Merge self with environment variables (env vars override self)
    fn merge_from_env(self) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_helper_get() {
        // This test would use env::set_var in real scenarios
        // Just verify the struct is constructible
        let _helper = EnvHelper;
    }

    #[test]
    fn test_bool_parsing() {
        assert_eq!(EnvHelper::get_bool("TRUE_TEST"), None); // Not set
                                                            // In real tests with env::set_var:
                                                            // env::set_var("VOIDM_TEST_BOOL", "true");
                                                            // assert_eq!(EnvHelper::get_bool("TEST_BOOL"), Some(true));
    }
}
