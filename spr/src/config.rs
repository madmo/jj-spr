/*
 * Copyright (c) Radical HQ Limited
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::collections::HashSet;

use crate::{error::Result, github::GitHubBranch, utils::slugify};

#[derive(Clone, Debug)]
pub struct Config {
    pub owner: String,
    pub repo: String,
    pub remote_name: String,
    pub master_ref: GitHubBranch,
    pub branch_prefix: String,
    pub require_approval: bool,
    pub github_host: Option<String>,
}

impl Config {
    pub fn new(
        owner: String,
        repo: String,
        remote_name: String,
        master_branch: String,
        branch_prefix: String,
        require_approval: bool,
        github_host: Option<String>,
    ) -> Self {
        let master_ref =
            GitHubBranch::new_from_branch_name(&master_branch, &remote_name, &master_branch);
        Self {
            owner,
            repo,
            remote_name,
            master_ref,
            branch_prefix,
            require_approval,
            github_host,
        }
    }

    /// Normalize and validate a GitHub host configuration value.
    /// Returns the base URL with protocol (https:// by default) and strips trailing slashes.
    fn normalize_github_host(host: &str) -> String {
        let trimmed = host.trim_end_matches('/');
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else {
            format!("https://{}", trimmed)
        }
    }

    /// Extract hostname from a normalized GitHub host URL.
    /// Returns just the hostname portion without protocol, path, or port.
    fn extract_hostname(base_url: &str) -> String {
        base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .split(':')
            .next()
            .unwrap_or("")
            .to_string()
    }

    /// Check if a normalized base URL is for github.com (not GHE).
    fn is_github_com(base_url: &str) -> bool {
        let hostname = Self::extract_hostname(base_url);
        hostname == "github.com" || hostname == "api.github.com"
    }

    /// Get the API base URL for REST API calls.
    /// Returns the base URL with /api/v3 suffix for GHE, or api.github.com for GitHub.com.
    pub fn api_base_url(&self) -> String {
        if let Some(host) = &self.github_host {
            let base = Self::normalize_github_host(host);
            if Self::is_github_com(&base) {
                "https://api.github.com".to_string()
            } else {
                // Strip any existing /api or /api/v3 suffix to avoid duplication
                let base_clean = base
                    .trim_end_matches("/api/v3")
                    .trim_end_matches("/api");
                format!("{}/api/v3", base_clean)
            }
        } else {
            "https://api.github.com".to_string()
        }
    }

    /// Get the web base URL for constructing PR links.
    fn web_base_url(&self) -> String {
        if let Some(host) = &self.github_host {
            let base = Self::normalize_github_host(host);
            if Self::is_github_com(&base) {
                "https://github.com".to_string()
            } else {
                // Strip any /api or /api/v3 suffix for web URLs
                base.trim_end_matches("/api/v3")
                    .trim_end_matches("/api")
                    .to_string()
            }
        } else {
            "https://github.com".to_string()
        }
    }

    pub fn pull_request_url(&self, number: u64) -> String {
        let web_base = self.web_base_url();
        format!(
            "{web_base}/{owner}/{repo}/pull/{number}",
            owner = &self.owner,
            repo = &self.repo
        )
    }

    pub fn parse_pull_request_field(&self, text: &str) -> Option<u64> {
        if text.is_empty() {
            return None;
        }

        let regex = lazy_regex::regex!(r#"^\s*#?\s*(\d+)\s*$"#);
        let m = regex.captures(text);
        if let Some(caps) = m {
            return Some(caps.get(1).unwrap().as_str().parse().unwrap());
        }

        let regex = lazy_regex::regex!(
            r#"^\s*https?://([^/]+)/([\w\-\.]+)/([\w\-\.]+)/pull/(\d+)([/?#].*)?\s*$"#
        );
        let m = regex.captures(text);
        if let Some(caps) = m {
            let host = caps.get(1).unwrap().as_str();
            let owner = caps.get(2).unwrap().as_str();
            let repo = caps.get(3).unwrap().as_str();
            let number_str = caps.get(4).unwrap().as_str();
            
            // Extract expected hostname from config
            let allowed_host = if let Some(cfg_host) = &self.github_host {
                let base = Self::normalize_github_host(cfg_host);
                Self::extract_hostname(&base)
            } else {
                "github.com".to_string()
            };

            if owner == self.owner && repo == self.repo && host == allowed_host {
                return Some(number_str.parse().unwrap());
            }
        }

        None
    }

    pub fn get_new_branch_name(&self, existing_ref_names: &HashSet<String>, title: &str) -> String {
        self.find_unused_branch_name(existing_ref_names, &slugify(title))
    }

    pub fn get_base_branch_name(
        &self,
        existing_ref_names: &HashSet<String>,
        title: &str,
    ) -> String {
        self.find_unused_branch_name(
            existing_ref_names,
            &format!("{}.{}", self.master_ref.branch_name(), &slugify(title)),
        )
    }

    fn find_unused_branch_name(&self, existing_ref_names: &HashSet<String>, slug: &str) -> String {
        let remote_name = &self.remote_name;
        let branch_prefix = &self.branch_prefix;
        let mut branch_name = format!("{branch_prefix}{slug}");
        let mut suffix = 0;

        loop {
            let remote_ref = format!("refs/remotes/{remote_name}/{branch_name}");

            if !existing_ref_names.contains(&remote_ref) {
                return branch_name;
            }

            suffix += 1;
            branch_name = format!("{branch_prefix}{slug}-{suffix}");
        }
    }

    pub fn new_github_branch_from_ref(&self, ghref: &str) -> Result<GitHubBranch> {
        GitHubBranch::new_from_ref(ghref, &self.remote_name, self.master_ref.branch_name())
    }

    pub fn graphql_endpoint(&self) -> String {
        if let Some(host) = &self.github_host {
            let base = Self::normalize_github_host(host);
            if Self::is_github_com(&base) {
                "https://api.github.com/graphql".to_string()
            } else {
                // Strip any existing /api or /api/v3 suffix to avoid duplication
                let base_clean = base
                    .trim_end_matches("/api/v3")
                    .trim_end_matches("/api");
                format!("{}/api/graphql", base_clean)
            }
        } else {
            "https://api.github.com/graphql".to_string()
        }
    }

    pub fn new_github_branch(&self, branch_name: &str) -> GitHubBranch {
        GitHubBranch::new_from_branch_name(
            branch_name,
            &self.remote_name,
            self.master_ref.branch_name(),
        )
    }
}

pub enum AuthTokenSource {
    Config(String),
    GitHubCLI(String),
}

impl AuthTokenSource {
    pub fn token(&self) -> &String {
        match self {
            AuthTokenSource::Config(token) | AuthTokenSource::GitHubCLI(token) => token,
        }
    }
}

pub fn get_auth_token(git_config: &git2::Config) -> Option<String> {
    get_auth_token_with_source(git_config).map(|v| v.token().to_owned())
}

pub fn get_auth_token_with_source(git_config: &git2::Config) -> Option<AuthTokenSource> {
    // Prefer the configured token if it exists
    if let Some(token) = get_config_value("spr.githubAuthToken", git_config) {
        return Some(AuthTokenSource::Config(token));
    }

    // Try to get a token from the gh CLI
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .stdout(std::process::Stdio::piped())
        .output()
        .ok()?;

    if output.status.success() {
        Some(AuthTokenSource::GitHubCLI(
            String::from_utf8(output.stdout).ok()?.trim().to_owned(),
        ))
    } else {
        None
    }
}

// Helper function to get config value from jj first, then git
pub fn get_config_value(key: &str, git_config: &git2::Config) -> Option<String> {
    // Try jj config first
    if let Ok(output) = std::process::Command::new("jj")
        .args(["config", "get", key])
        .output()
        && output.status.success()
        && let Ok(value) = String::from_utf8(output.stdout)
    {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    // Fall back to git config
    git_config.get_string(key).ok()
}

pub fn get_config_bool(key: &str, git_config: &git2::Config) -> Option<bool> {
    // Try jj config first
    if let Ok(output) = std::process::Command::new("jj")
        .args(["config", "get", key])
        .output()
        && output.status.success()
        && let Ok(value) = String::from_utf8(output.stdout)
    {
        let trimmed = value.trim().to_lowercase();
        if trimmed == "true" {
            return Some(true);
        } else if trimmed == "false" {
            return Some(false);
        }
    }

    // Fall back to git config
    git_config.get_bool(key).ok()
}

/// Helper function to set config value in jj (repo-level)
pub fn set_jj_config(key: &str, value: &str, repo_path: &std::path::Path) -> Result<()> {
    let output = std::process::Command::new("jj")
        .args(["config", "set", "--repo", key, value])
        .current_dir(repo_path)
        .output()
        .map_err(|e| crate::error::Error::new(format!("Failed to execute jj config set: {}", e)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(crate::error::Error::new(format!(
            "jj config set failed for key '{}': {}",
            key, stderr
        )))
    }
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    fn config_factory() -> Config {
        crate::config::Config::new(
            "acme".into(),
            "codez".into(),
            "origin".into(),
            "master".into(),
            "spr/foo/".into(),
            false,
            None,
        )
    }

    #[test]
    fn test_set_jj_config_success() {
        // Create a temporary jj repo for testing
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let path = temp_dir.path();

        // Initialize git repo first
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("Failed to init git repo");

        // Initialize jj repo (colocated)
        let jj_init = std::process::Command::new("jj")
            .args(["git", "init", "--colocate"])
            .current_dir(path)
            .output()
            .expect("Failed to init jj repo");

        if !jj_init.status.success() {
            // Skip test if jj is not available
            return;
        }

        // Test setting a config value
        let result = set_jj_config("spr.githubRepository", "test/repo", path);
        assert!(result.is_ok(), "Should successfully set config");

        // Verify the config was set
        let output = std::process::Command::new("jj")
            .args(["config", "get", "spr.githubRepository"])
            .current_dir(path)
            .output()
            .expect("Failed to get config");

        assert!(output.status.success());
        let value = String::from_utf8(output.stdout).unwrap();
        assert_eq!(value.trim(), "test/repo");
    }

    #[test]
    fn test_set_jj_config_multiple_values() {
        // Create a temporary jj repo for testing
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let path = temp_dir.path();

        // Initialize git repo first
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .expect("Failed to init git repo");

        // Initialize jj repo (colocated)
        let jj_init = std::process::Command::new("jj")
            .args(["git", "init", "--colocate"])
            .current_dir(path)
            .output()
            .expect("Failed to init jj repo");

        if !jj_init.status.success() {
            // Skip test if jj is not available
            return;
        }

        // Set multiple config values
        assert!(set_jj_config("spr.githubRepository", "owner/repo", path).is_ok());
        assert!(set_jj_config("spr.branchPrefix", "spr/test/", path).is_ok());
        assert!(set_jj_config("spr.requireApproval", "false", path).is_ok());

        // Verify all configs were set correctly
        let output = std::process::Command::new("jj")
            .args(["config", "get", "spr.githubRepository"])
            .current_dir(path)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "owner/repo"
        );

        let output = std::process::Command::new("jj")
            .args(["config", "get", "spr.branchPrefix"])
            .current_dir(path)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "spr/test/"
        );

        let output = std::process::Command::new("jj")
            .args(["config", "get", "spr.requireApproval"])
            .current_dir(path)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "false");
    }

    #[test]
    fn test_set_jj_config_invalid_repo() {
        // Try to set config in a non-existent directory
        let result = set_jj_config(
            "spr.test",
            "value",
            std::path::Path::new("/nonexistent/path"),
        );
        assert!(result.is_err(), "Should fail for invalid repo path");
    }

    #[test]
    fn test_pull_request_url() {
        let gh = config_factory();

        assert_eq!(
            &gh.pull_request_url(123),
            "https://github.com/acme/codez/pull/123"
        );
    }

    #[test]
    fn test_parse_pull_request_field_empty() {
        let gh = config_factory();

        assert_eq!(gh.parse_pull_request_field(""), None);
        assert_eq!(gh.parse_pull_request_field("   "), None);
        assert_eq!(gh.parse_pull_request_field("\n"), None);
    }

    #[test]
    fn test_parse_pull_request_field_number() {
        let gh = config_factory();

        assert_eq!(gh.parse_pull_request_field("123"), Some(123));
        assert_eq!(gh.parse_pull_request_field("   123 "), Some(123));
        assert_eq!(gh.parse_pull_request_field("#123"), Some(123));
        assert_eq!(gh.parse_pull_request_field(" # 123"), Some(123));
    }

    #[test]
    fn test_parse_pull_request_field_url() {
        let gh = config_factory();

        assert_eq!(
            gh.parse_pull_request_field("https://github.com/acme/codez/pull/123"),
            Some(123)
        );
        assert_eq!(
            gh.parse_pull_request_field("  https://github.com/acme/codez/pull/123  "),
            Some(123)
        );
        assert_eq!(
            gh.parse_pull_request_field("https://github.com/acme/codez/pull/123/"),
            Some(123)
        );
        assert_eq!(
            gh.parse_pull_request_field("https://github.com/acme/codez/pull/123?x=a"),
            Some(123)
        );
        assert_eq!(
            gh.parse_pull_request_field("https://github.com/acme/codez/pull/123/foo"),
            Some(123)
        );
        assert_eq!(
            gh.parse_pull_request_field("https://github.com/acme/codez/pull/123#abc"),
            Some(123)
        );
    }

    #[test]
    fn test_hostname_detection_github_com() {
        let config = crate::config::Config::new(
            "owner".into(),
            "repo".into(),
            "origin".into(),
            "main".into(),
            "spr/".into(),
            false,
            Some("github.com".into()),
        );
        
        // Should use api.github.com for github.com
        assert_eq!(config.api_base_url(), "https://api.github.com");
        assert_eq!(config.graphql_endpoint(), "https://api.github.com/graphql");
    }

    #[test]
    fn test_hostname_detection_ghe() {
        let config = crate::config::Config::new(
            "owner".into(),
            "repo".into(),
            "origin".into(),
            "main".into(),
            "spr/".into(),
            false,
            Some("ghe.example.com".into()),
        );
        
        // Should use /api/v3 suffix for GHE
        assert_eq!(config.api_base_url(), "https://ghe.example.com/api/v3");
        assert_eq!(config.graphql_endpoint(), "https://ghe.example.com/api/graphql");
    }

    #[test]
    fn test_hostname_detection_substring_attack() {
        // Test that "mygithub.com" is NOT treated as github.com
        let config = crate::config::Config::new(
            "owner".into(),
            "repo".into(),
            "origin".into(),
            "main".into(),
            "spr/".into(),
            false,
            Some("mygithub.com".into()),
        );
        
        // Should treat as GHE, not as github.com
        assert_eq!(config.api_base_url(), "https://mygithub.com/api/v3");
        assert_eq!(config.graphql_endpoint(), "https://mygithub.com/api/graphql");
    }

    #[test]
    fn test_api_suffix_stripping() {
        // Test that /api and /api/v3 suffixes are properly stripped
        let config1 = crate::config::Config::new(
            "owner".into(),
            "repo".into(),
            "origin".into(),
            "main".into(),
            "spr/".into(),
            false,
            Some("ghe.example.com/api".into()),
        );
        assert_eq!(config1.api_base_url(), "https://ghe.example.com/api/v3");
        assert_eq!(config1.graphql_endpoint(), "https://ghe.example.com/api/graphql");

        let config2 = crate::config::Config::new(
            "owner".into(),
            "repo".into(),
            "origin".into(),
            "main".into(),
            "spr/".into(),
            false,
            Some("ghe.example.com/api/v3".into()),
        );
        assert_eq!(config2.api_base_url(), "https://ghe.example.com/api/v3");
        assert_eq!(config2.graphql_endpoint(), "https://ghe.example.com/api/graphql");
    }

    #[test]
    fn test_pr_url_parsing_ghe() {
        let config = crate::config::Config::new(
            "owner".into(),
            "repo".into(),
            "origin".into(),
            "main".into(),
            "spr/".into(),
            false,
            Some("ghe.example.com".into()),
        );
        
        // Should parse GHE URLs correctly
        assert_eq!(
            config.parse_pull_request_field("https://ghe.example.com/owner/repo/pull/123"),
            Some(123)
        );
        
        // Should reject github.com URLs when configured for GHE
        assert_eq!(
            config.parse_pull_request_field("https://github.com/owner/repo/pull/123"),
            None
        );
    }
}
