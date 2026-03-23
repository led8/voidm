use anyhow::{anyhow, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug, Clone)]
pub struct CheckUpdateArgs {
    /// Output JSON (machine-readable)
    #[arg(long)]
    pub json: bool,

    /// Force refresh cache (ignore 24h TTL and fetch fresh)
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub html_url: String,
    pub published_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub download_url: String,
    pub published_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedRelease {
    tag_name: String,
    html_url: String,
    published_at: String,
    cached_at: u64,
}

/// Get cache file path: ~/.config/voidm/update-check.json (or platform equivalent)
fn get_cache_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| anyhow!("Could not determine config directory"))?
        .join("voidm");
    Ok(config_dir.join("update-check.json"))
}

/// Check if cache is still valid (< 24 hours old)
fn is_cache_valid(cached_at: u64) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let age_secs = now.saturating_sub(cached_at);
    age_secs < 86400 // 24 hours
}

/// Load cached release info if available and valid
/// If ignore_ttl is true, return cache regardless of age
fn load_cached_release(ignore_ttl: bool) -> Option<CachedRelease> {
    let cache_path = get_cache_path().ok()?;
    let content = fs::read_to_string(&cache_path).ok()?;
    let cached: CachedRelease = serde_json::from_str(&content).ok()?;

    if ignore_ttl || is_cache_valid(cached.cached_at) {
        return Some(cached);
    }

    None
}

/// Save release info to cache
fn save_cached_release(release: &GitHubRelease) -> Result<()> {
    let cache_path = get_cache_path()?;

    // Create config directory if it doesn't exist
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let cached = CachedRelease {
        tag_name: release.tag_name.clone(),
        html_url: release.html_url.clone(),
        published_at: release.published_at.clone(),
        cached_at: now,
    };

    let json = serde_json::to_string_pretty(&cached)?;
    fs::write(&cache_path, json)?;
    Ok(())
}

/// Parse version string, handling both "v0.8.0" and "0.8.0" formats
/// Returns (major, minor, patch, prerelease_num)
/// For "0.8.0-rc.10", returns (0, 8, 0, Some(10))
/// For "0.8.0", returns (0, 8, 0, None)
fn parse_version(version_str: &str) -> Result<(u32, u32, u32, Option<u32>)> {
    let version = version_str.trim_start_matches('v');

    // First check if there's a pre-release marker
    let (base_version, prerelease_part) = if let Some(dash_idx) = version.find('-') {
        (&version[..dash_idx], Some(&version[dash_idx + 1..]))
    } else {
        (version, None)
    };

    let parts: Vec<&str> = base_version.split('.').collect();

    if parts.len() < 3 {
        return Err(anyhow!("Invalid version format: {}", version_str));
    }

    let major = parts[0].parse::<u32>()?;
    let minor = parts[1].parse::<u32>()?;
    let patch = parts[2].parse::<u32>()?;

    // Extract pre-release number if present
    let prerelease_num = prerelease_part.and_then(|pre| {
        // Extract number from strings like "rc.10", "alpha.1", etc
        pre.split(|c: char| !c.is_numeric())
            .find(|s| !s.is_empty())
            .and_then(|s| s.parse::<u32>().ok())
    });

    Ok((major, minor, patch, prerelease_num))
}

/// Compare two versions. Returns true if latest > current
fn is_update_available(current: &str, latest: &str) -> Result<bool> {
    let (curr_major, curr_minor, curr_patch, curr_prerelease) = parse_version(current)?;
    let (latest_major, latest_minor, latest_patch, latest_prerelease) = parse_version(latest)?;

    // Compare major.minor.patch first
    let curr_tuple = (curr_major, curr_minor, curr_patch);
    let latest_tuple = (latest_major, latest_minor, latest_patch);

    if latest_tuple > curr_tuple {
        return Ok(true);
    } else if latest_tuple < curr_tuple {
        return Ok(false);
    }

    // If major.minor.patch are equal, compare pre-release versions
    // Release version (None) > pre-release version (Some)
    match (curr_prerelease, latest_prerelease) {
        (None, None) => Ok(false),    // Both release, same version
        (Some(_), None) => Ok(true),  // Current is pre-release, latest is release
        (None, Some(_)) => Ok(false), // Current is release, latest is pre-release
        (Some(curr_pre), Some(latest_pre)) => Ok(latest_pre > curr_pre), // Both pre-release
    }
}

/// Fetch latest release from GitHub API
async fn fetch_latest_release() -> Result<GitHubRelease> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let response = client
        .get("https://api.github.com/repos/autonomous-toaster/voidm/releases/latest")
        .header("User-Agent", "voidm-cli")
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                anyhow!("Could not reach GitHub (timeout after 5s)")
            } else if e.is_connect() {
                anyhow!("Could not reach GitHub (network error)")
            } else {
                anyhow!("Could not reach GitHub: {}", e)
            }
        })?;

    if response.status() == 403 {
        return Err(anyhow!(
            "GitHub API rate limited. Try again later or set GITHUB_TOKEN environment variable"
        ));
    }

    if !response.status().is_success() {
        return Err(anyhow!(
            "GitHub API error: {} {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        ));
    }

    let release: GitHubRelease = response
        .json()
        .await
        .map_err(|e| anyhow!("Could not parse GitHub release info: {}", e))?;

    // Save to cache
    let _ = save_cached_release(&release);
    Ok(release)
}

pub async fn check_update(args: CheckUpdateArgs) -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");

    // Strategy:
    // 1. If cache exists and valid, use it (fast path)
    // 2. If --force, skip cache and fetch fresh
    // 3. Otherwise, try to fetch; if that fails, use stale cache as fallback

    let release = if let Some(cached) = load_cached_release(false) {
        // Cache hit - use it
        if !args.json {
            eprintln!("(cached)");
        }
        GitHubRelease {
            tag_name: cached.tag_name,
            html_url: cached.html_url,
            published_at: cached.published_at,
        }
    } else if args.force {
        // --force: skip cache and fetch fresh
        fetch_latest_release().await?
    } else {
        // Cache miss - try to fetch, with stale cache as fallback
        match fetch_latest_release().await {
            Ok(release) => release,
            Err(e) => {
                // Fetch failed - try to use stale cache
                if let Some(cached) = load_cached_release(true) {
                    if !args.json {
                        eprintln!("(cached - GitHub API unavailable)");
                    }
                    GitHubRelease {
                        tag_name: cached.tag_name,
                        html_url: cached.html_url,
                        published_at: cached.published_at,
                    }
                } else {
                    // No cache available - return the fetch error
                    return Err(e);
                }
            }
        }
    };

    // Extract version from tag (e.g., "v0.8.0" -> "0.8.0")
    let latest_version = release.tag_name.trim_start_matches('v').to_string();

    // Check if update is available
    let update_available = is_update_available(current_version, &latest_version)?;

    let result = UpdateCheckResult {
        current_version: current_version.to_string(),
        latest_version: latest_version.clone(),
        update_available,
        download_url: release.html_url,
        published_at: release.published_at,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("Current version: {}", result.current_version);
        println!("Latest version:  {}", result.latest_version);
        println!();

        if result.update_available {
            println!("✓ Update available!");
            println!();
            println!("Download at:");
            println!("  {}", result.download_url);
            println!();
            println!("Published: {}", result.published_at);
        } else {
            println!("✓ You are running the latest version!");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_standard() {
        assert_eq!(parse_version("0.8.0").unwrap(), (0, 8, 0, None));
        assert_eq!(parse_version("v0.8.0").unwrap(), (0, 8, 0, None));
        assert_eq!(parse_version("1.2.3").unwrap(), (1, 2, 3, None));
    }

    #[test]
    fn test_parse_version_prerelease() {
        assert_eq!(parse_version("0.8.0-rc.10").unwrap(), (0, 8, 0, Some(10)));
        assert_eq!(parse_version("v2.0.0-rc.11").unwrap(), (2, 0, 0, Some(11)));
        assert_eq!(parse_version("1.0.0-alpha").unwrap(), (1, 0, 0, None));
    }

    #[test]
    fn test_parse_version_invalid() {
        assert!(parse_version("invalid").is_err());
        assert!(parse_version("1.2").is_err());
    }

    #[test]
    fn test_is_update_available() {
        // Patch version bump
        assert!(is_update_available("0.8.0", "0.8.1").unwrap());
        assert!(!is_update_available("0.8.1", "0.8.0").unwrap());

        // Minor version bump
        assert!(is_update_available("0.8.0", "0.9.0").unwrap());
        assert!(!is_update_available("0.9.0", "0.8.0").unwrap());

        // Major version bump
        assert!(is_update_available("0.8.0", "1.0.0").unwrap());
        assert!(!is_update_available("1.0.0", "0.8.0").unwrap());

        // Same version
        assert!(!is_update_available("0.8.0", "0.8.0").unwrap());

        // With v prefix
        assert!(is_update_available("v0.8.0", "v0.9.0").unwrap());

        // Pre-release versions
        assert!(is_update_available("0.8.0-rc.10", "0.8.0-rc.11").unwrap());
        assert!(is_update_available("0.8.0-rc.10", "0.8.0").unwrap());
    }

    #[test]
    fn test_is_cache_valid() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Cache from 1 hour ago should be valid
        assert!(is_cache_valid(now - 3600));

        // Cache from 25 hours ago should be invalid
        assert!(!is_cache_valid(now - 90000));

        // Cache from 1 second ago should be valid
        assert!(is_cache_valid(now - 1));
    }
}
