use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionConfig {
    pub enabled: bool,
    pub api_keys: PatternRedactionConfig,
    pub jwt_tokens: PatternRedactionConfig,
    pub db_connections: PatternRedactionConfig,
    pub auth_tokens: PatternRedactionConfig,
    pub emails: PatternRedactionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRedactionConfig {
    pub enabled: bool,
    pub strategy: String, // "mask" or "remove"
    pub prefix_length: usize,
    pub suffix_length: usize,
}

#[derive(Debug, Clone)]
pub struct RedactionWarning {
    pub pattern_type: String,
    pub field: String,
    pub count: usize,
}

impl Default for RedactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_keys: PatternRedactionConfig {
                enabled: true,
                strategy: "mask".to_string(),
                prefix_length: 3,
                suffix_length: 2,
            },
            jwt_tokens: PatternRedactionConfig {
                enabled: true,
                strategy: "mask".to_string(),
                prefix_length: 3,
                suffix_length: 3,
            },
            db_connections: PatternRedactionConfig {
                enabled: true,
                strategy: "mask".to_string(),
                prefix_length: 0, // Special handling for DB connections
                suffix_length: 0,
            },
            auth_tokens: PatternRedactionConfig {
                enabled: true,
                strategy: "mask".to_string(),
                prefix_length: 2,
                suffix_length: 2,
            },
            emails: PatternRedactionConfig {
                enabled: true,
                strategy: "mask".to_string(),
                prefix_length: 1,
                suffix_length: 3,
            },
        }
    }
}

// Define patterns as lazy statics to compile once
lazy_static! {
    // API Keys patterns
    // OpenAI: sk-...
    static ref OPENAI_API_KEY: Regex =
        Regex::new(r"sk-[A-Za-z0-9\-_]{20,}").unwrap();

    // AWS Access Key ID
    static ref AWS_ACCESS_KEY: Regex =
        Regex::new(r"AKIA[0-9A-Z]{16}").unwrap();

    // Generic API key patterns: api_key=..., API_KEY=...
    // Requires at least 24 characters (real API keys are typically longer than placeholder strings)
    static ref GENERIC_API_KEY: Regex =
        Regex::new(r#"(?i)(api_key|apikey)\s*=\s*['""]?([A-Za-z0-9\-_.]{24,})['""]?"#).unwrap();

    // Database connection strings
    // MySQL: mysql://user:pass@host:port/db
    static ref MYSQL_CONNECTION: Regex =
        Regex::new(r"mysql://[^\s:]+:[^\s@]+@[^\s:/]+(?::\d+)?/[^\s;]+").unwrap();

    // PostgreSQL: postgresql://... or postgres://...
    static ref POSTGRES_CONNECTION: Regex =
        Regex::new(r"(?:postgresql|postgres)://[^\s:]+:[^\s@]+@[^\s:/]+(?::\d+)?/[^\s;]+").unwrap();

    // MongoDB: mongodb://...
    static ref MONGODB_CONNECTION: Regex =
        Regex::new(r"mongodb://[^\s:]+:[^\s@]+@[^\s:/]+(?::\d+)?/[^\s;]+").unwrap();

    // Generic connection string pattern
    static ref GENERIC_CONNECTION_STRING: Regex =
        Regex::new(r#"(?i)connection_string\s*=\s*['""]?([^'"";\s]+://[^'"";\s]+)['""]?"#).unwrap();

    // JWT tokens: eyJ...eyJ... (three parts separated by dots)
    static ref JWT_TOKEN: Regex =
        Regex::new(r"eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+").unwrap();

    // Bearer tokens: Bearer [token] (min 20 alphanumeric chars)
    static ref BEARER_TOKEN: Regex =
        Regex::new(r"Bearer\s+([A-Za-z0-9\-_\.]{20,})").unwrap();

    // Session tokens: session_id=..., auth_token=..., etc.
    // Requires at least 20 characters (real tokens are significantly longer than placeholders)
    static ref SESSION_TOKEN: Regex =
        Regex::new(r#"(?i)(session_id|auth_token|token|sessiontoken)\s*=\s*['""]?([A-Za-z0-9\-_.]{20,})['""]?"#).unwrap();

    // Email addresses: loose matching word@word.word
    static ref EMAIL_ADDRESS: Regex =
        Regex::new(r"\b[A-Za-z0-9_\.\-]+@[A-Za-z0-9_\.\-]+\.[A-Za-z]{2,}\b").unwrap();
}

/// Redact a single text string and return (redacted_text, warnings)
pub fn redact_text(text: &str, config: &RedactionConfig) -> (String, Vec<RedactionWarning>) {
    if !config.enabled {
        return (text.to_string(), Vec::new());
    }

    let mut result = text.to_string();
    let mut warnings = Vec::new();

    // API Keys (try all patterns)
    if config.api_keys.enabled {
        // OpenAI keys
        let count = OPENAI_API_KEY.find_iter(&result).count();
        if count > 0 {
            result = redact_pattern(&result, &OPENAI_API_KEY, &config.api_keys);
            warnings.push(RedactionWarning {
                pattern_type: "OpenAI API Key".to_string(),
                field: "content".to_string(),
                count,
            });
        }

        // AWS keys
        let count = AWS_ACCESS_KEY.find_iter(&result).count();
        if count > 0 {
            result = redact_pattern(&result, &AWS_ACCESS_KEY, &config.api_keys);
            warnings.push(RedactionWarning {
                pattern_type: "AWS Access Key".to_string(),
                field: "content".to_string(),
                count,
            });
        }

        // Generic API keys
        let count = GENERIC_API_KEY.find_iter(&result).count();
        if count > 0 {
            result = redact_pattern(&result, &GENERIC_API_KEY, &config.api_keys);
            warnings.push(RedactionWarning {
                pattern_type: "API Key".to_string(),
                field: "content".to_string(),
                count,
            });
        }
    }

    // Database Connection Strings
    if config.db_connections.enabled {
        let mut total_db_count = 0;

        // MySQL
        let count = MYSQL_CONNECTION.find_iter(&result).count();
        if count > 0 {
            result = redact_db_connection(&result, &MYSQL_CONNECTION);
            total_db_count += count;
        }

        // PostgreSQL
        let count = POSTGRES_CONNECTION.find_iter(&result).count();
        if count > 0 {
            result = redact_db_connection(&result, &POSTGRES_CONNECTION);
            total_db_count += count;
        }

        // MongoDB
        let count = MONGODB_CONNECTION.find_iter(&result).count();
        if count > 0 {
            result = redact_db_connection(&result, &MONGODB_CONNECTION);
            total_db_count += count;
        }

        // Generic connection strings
        let count = GENERIC_CONNECTION_STRING.find_iter(&result).count();
        if count > 0 {
            result = redact_pattern(&result, &GENERIC_CONNECTION_STRING, &config.db_connections);
            total_db_count += count;
        }

        if total_db_count > 0 {
            warnings.push(RedactionWarning {
                pattern_type: "Database Connection".to_string(),
                field: "content".to_string(),
                count: total_db_count,
            });
        }
    }

    // JWT Tokens
    if config.jwt_tokens.enabled {
        let count = JWT_TOKEN.find_iter(&result).count();
        if count > 0 {
            result = redact_pattern(&result, &JWT_TOKEN, &config.jwt_tokens);
            warnings.push(RedactionWarning {
                pattern_type: "JWT Token".to_string(),
                field: "content".to_string(),
                count,
            });
        }
    }

    // Auth Tokens (Bearer + Session)
    if config.auth_tokens.enabled {
        let mut total_auth_count = 0;

        // Bearer tokens
        let count = BEARER_TOKEN.find_iter(&result).count();
        if count > 0 {
            result = redact_pattern(&result, &BEARER_TOKEN, &config.auth_tokens);
            total_auth_count += count;
        }

        // Session tokens
        let count = SESSION_TOKEN.find_iter(&result).count();
        if count > 0 {
            result = redact_pattern(&result, &SESSION_TOKEN, &config.auth_tokens);
            total_auth_count += count;
        }

        if total_auth_count > 0 {
            warnings.push(RedactionWarning {
                pattern_type: "Auth Token".to_string(),
                field: "content".to_string(),
                count: total_auth_count,
            });
        }
    }

    // Email Addresses (loose matching)
    if config.emails.enabled {
        let count = EMAIL_ADDRESS.find_iter(&result).count();
        if count > 0 {
            result = redact_pattern(&result, &EMAIL_ADDRESS, &config.emails);
            warnings.push(RedactionWarning {
                pattern_type: "Email Address".to_string(),
                field: "content".to_string(),
                count,
            });
        }
    }

    (result, warnings)
}

/// Mask a matched pattern: keep first N and last M characters
fn mask_pattern(matched: &str, config: &PatternRedactionConfig) -> String {
    if matched.len() <= (config.prefix_length + config.suffix_length) {
        return "[REDACTED]".to_string();
    }

    let prefix = &matched[..config.prefix_length.min(matched.len())];
    let suffix = if config.suffix_length > 0 {
        &matched[matched.len().saturating_sub(config.suffix_length)..]
    } else {
        ""
    };

    if suffix.is_empty() {
        format!("{}...", prefix)
    } else {
        format!("{}...{}", prefix, suffix)
    }
}

/// Generic pattern redaction (uses mask strategy)
fn redact_pattern(text: &str, regex: &Regex, config: &PatternRedactionConfig) -> String {
    match config.strategy.as_str() {
        "remove" => regex.replace_all(text, "").to_string(),
        "mask" | _ => regex
            .replace_all(text, |caps: &regex::Captures| {
                let matched = caps.get(0).unwrap().as_str();
                mask_pattern(matched, config)
            })
            .to_string(),
    }
}

/// Special redaction for database connection strings (hide credentials)
fn redact_db_connection(text: &str, regex: &Regex) -> String {
    regex
        .replace_all(text, |caps: &regex::Captures| {
            let matched = caps.get(0).unwrap().as_str();

            // Pattern: scheme://user:pass@host:port/db
            // Redact to: scheme://...@host:port/db
            if let Some(at_pos) = matched.rfind('@') {
                let before_at = &matched[..=at_pos - 1];
                if let Some(scheme_end) = before_at.find("://") {
                    let scheme = &before_at[..scheme_end + 3];
                    let after_at = &matched[at_pos + 1..];
                    return format!("{}...@{}", scheme, after_at);
                }
            }

            // Fallback: just mask the whole thing
            "[REDACTED_DB_CONNECTION]".to_string()
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_api_key() {
        let text = "my api key is sk-1a2b3c4d5e6f7g8h9i0j and keep going";
        let config = RedactionConfig::default();
        let (redacted, warnings) = redact_text(text, &config);

        assert!(!redacted.contains("sk-1a2b3c"));
        assert!(redacted.contains("sk-..."));
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_aws_access_key() {
        let text = "AWS key: AKIA1234567890ABCDEF in config";
        let config = RedactionConfig::default();
        let (redacted, warnings) = redact_text(text, &config);

        assert!(!redacted.contains("AKIA1234567890"));
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_jwt_token() {
        let text = "token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let config = RedactionConfig::default();
        let (redacted, _) = redact_text(text, &config);

        assert!(!redacted.contains("eyJhbGciOiJIUzI1NiI"));
        assert!(redacted.contains("eyJ..."));
    }

    #[test]
    fn test_mysql_connection() {
        let text = "DB: mysql://admin:password123@localhost:3306/mydb in code";
        let config = RedactionConfig::default();
        let (redacted, warnings) = redact_text(text, &config);

        assert!(!redacted.contains("admin:password123"));
        assert!(redacted.contains("mysql://...@localhost:3306"));
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_postgresql_connection() {
        let text = "postgres://user:secret@db.example.com:5432/prod";
        let config = RedactionConfig::default();
        let (redacted, _) = redact_text(text, &config);

        assert!(!redacted.contains("user:secret"));
        assert!(redacted.contains("postgres://...@"));
    }

    #[test]
    fn test_bearer_token() {
        let text = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9token123456";
        let config = RedactionConfig::default();
        let (redacted, warnings) = redact_text(text, &config);

        assert!(!redacted.contains("Bearer eyJhbGciOi"));
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_email_address() {
        let text = "Contact: user@example.com or admin@test.org";
        let config = RedactionConfig::default();
        let (redacted, warnings) = redact_text(text, &config);

        assert!(!redacted.contains("user@example.com"));
        assert!(!redacted.contains("admin@test.org"));
        assert!(warnings.len() > 0);
    }

    #[test]
    fn test_false_negative_prevention() {
        let text = "Use api_key=YOUR_API_KEY_HERE in config";
        let config = RedactionConfig::default();
        let (redacted, warnings) = redact_text(text, &config);

        // Should NOT redact dummy values
        assert!(redacted.contains("YOUR_API_KEY_HERE"));
        assert!(warnings.is_empty() || !warnings.iter().any(|w| w.pattern_type.contains("API")));
    }

    #[test]
    fn test_disabled_redaction() {
        let mut config = RedactionConfig::default();
        config.enabled = false;

        let text = "secret: sk-1a2b3c4d5e6f";
        let (redacted, warnings) = redact_text(text, &config);

        assert_eq!(text, redacted);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_masking_prefix_suffix() {
        let config = PatternRedactionConfig {
            enabled: true,
            strategy: "mask".to_string(),
            prefix_length: 3,
            suffix_length: 2,
        };

        let masked = mask_pattern("sk-1a2b3c4d5e6f", &config);
        assert!(masked.contains("sk-") || masked.starts_with("sk-"));
        assert!(masked.contains("..."));
    }
}
