//! `~/.config/lmrun/config.json` and the directory layout around it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Where everything axio-local owns lives. Nothing is ever written outside `base`
/// except the snapshot restore, which writes back to the files it copied.
#[derive(Debug, Clone)]
pub struct Paths {
    pub home: PathBuf,
    pub homes: PathBuf,
    pub snapshots: PathBuf,
    pub config: PathBuf,
}

impl Paths {
    pub fn from_home(home: PathBuf) -> Self {
        let base = home.join(".config").join("lmrun");
        Self {
            homes: base.join("homes"),
            snapshots: base.join("snapshots"),
            config: base.join("config.json"),
            home,
        }
    }

    pub fn discover() -> Result<Self> {
        let home = std::env::home_dir().context("cannot determine the home directory")?;
        Ok(Self::from_home(home))
    }

    pub fn harness_home(&self, harness: &str) -> PathBuf {
        self.homes.join(harness)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Server base URL, or `None` to auto-detect.
    pub url: Option<String>,
    /// Bearer token sent to the server; local servers accept anything non-empty.
    pub token: String,
    /// The main model: planning, the top of a Claude Code session, Axio, Pi.
    pub orchestrator: String,
    /// The subagent model: Claude Code's `haiku` and every subagent, OpenCode's `small_model`.
    pub worker: String,
    /// Codex is single-model and speaks the Responses API; this is what it runs on.
    pub codex_model: String,
    pub context_window: u64,
    pub max_output: u64,
    pub keep_snapshots: usize,
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub url: String,
    pub orchestrator: String,
    pub worker: String,
}

impl Config {
    pub fn apply_profile(&mut self, name: &str) -> Result<()> {
        let profile = self.profiles.get(name).with_context(|| {
            format!("unknown local profile {name:?}; define it in profiles in config.json")
        })?;
        anyhow::ensure!(
            profile.url.starts_with("http://")
                && !profile.orchestrator.trim().is_empty()
                && !profile.worker.trim().is_empty(),
            "profile requires an HTTP URL and both model roles"
        );
        self.url = Some(profile.url.clone());
        self.orchestrator = profile.orchestrator.clone();
        self.worker = profile.worker.clone();
        self.codex_model = profile.worker.clone();
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            url: None,
            token: "local".into(),
            orchestrator: "Jundot--Qwen3.8-27B-oQ4e-mtp".into(),
            worker: "gpt-oss-120b-MXFP4-Q8".into(),
            codex_model: "gpt-oss-120b-MXFP4-Q8".into(),
            context_window: 131_072,
            max_output: 32_768,
            keep_snapshots: 10,
            profiles: BTreeMap::new(),
        }
    }
}

impl Config {
    /// Load the config, writing the defaults first if the file does not exist.
    pub fn load_or_init(path: &Path) -> Result<Self> {
        if path.exists() {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            return serde_json::from_str(&text)
                .with_context(|| format!("parsing {}", path.display()));
        }
        let config = Self::default();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&config)? + "\n")
            .with_context(|| format!("writing {}", path.display()))?;
        eprintln!("axio-local: wrote default config to {}", path.display());
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_profiles_select_all_roles_and_unknown_names_fail() {
        let mut config = Config::default();
        config.profiles.insert(
            "workstation".into(),
            Profile {
                url: "http://localhost:9000".into(),
                orchestrator: "planner".into(),
                worker: "worker".into(),
            },
        );
        config.apply_profile("workstation").unwrap();
        assert_eq!(config.url.as_deref(), Some("http://localhost:9000"));
        assert_eq!(config.orchestrator, "planner");
        assert_eq!(config.worker, "worker");
        assert_eq!(config.codex_model, "worker");
        assert!(config.apply_profile("missing").is_err());
    }

    #[test]
    fn missing_config_is_written_with_defaults_and_reloads_identically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.json");
        let first = Config::load_or_init(&path).unwrap();
        assert!(path.exists());
        let second = Config::load_or_init(&path).unwrap();
        assert_eq!(first.orchestrator, second.orchestrator);
        assert_eq!(second.keep_snapshots, 10);
        assert!(second.url.is_none());
    }

    #[test]
    fn partial_config_keeps_defaults_for_missing_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"worker": "tiny"}"#).unwrap();
        let config = Config::load_or_init(&path).unwrap();
        assert_eq!(config.worker, "tiny");
        assert_eq!(config.token, "local");
    }

    #[test]
    fn paths_hang_off_the_home_directory() {
        let paths = Paths::from_home(PathBuf::from("/h"));
        assert_eq!(paths.config, PathBuf::from("/h/.config/lmrun/config.json"));
        assert_eq!(
            paths.harness_home("codex"),
            PathBuf::from("/h/.config/lmrun/homes/codex")
        );
    }
}
