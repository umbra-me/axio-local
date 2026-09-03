//! One builder per harness: the environment and generated configuration that
//! point it at the local server with its configuration home relocated.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde_json::{Value, json};

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    Claude,
    Codex,
    Opencode,
    Pi,
    Axio,
    Shell,
}

impl Harness {
    pub const ALL: [Harness; 6] = [
        Harness::Claude,
        Harness::Codex,
        Harness::Opencode,
        Harness::Pi,
        Harness::Axio,
        Harness::Shell,
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|h| h.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Codex => "codex",
            Harness::Opencode => "opencode",
            Harness::Pi => "pi",
            Harness::Axio => "axio",
            Harness::Shell => "shell",
        }
    }

    /// Real configuration files, relative to the home directory, that this
    /// harness could write. Snapshotted before launch and restored after.
    pub fn protected(self) -> &'static [&'static str] {
        const CLAUDE: &[&str] = &[
            ".claude.json",
            ".claude/settings.json",
            ".claude/settings.local.json",
            ".claude/CLAUDE.md",
            ".claude/keybindings.json",
        ];
        const CODEX: &[&str] = &[".codex/config.toml", ".codex/auth.json", ".codex/AGENTS.md"];
        const OPENCODE: &[&str] = &[
            ".config/opencode/opencode.json",
            ".config/opencode/opencode.jsonc",
            ".local/share/opencode/auth.json",
        ];
        const PI: &[&str] = &[
            ".pi/agent/models.json",
            ".pi/agent/settings.json",
            ".pi/agent/auth.json",
        ];
        const AXIO: &[&str] = &[".axio/config.toml", ".axio/auth.json"];
        const SHELL: &[&str] = &[
            ".axio/auth.json",
            ".axio/config.toml",
            ".claude.json",
            ".claude/CLAUDE.md",
            ".claude/keybindings.json",
            ".claude/settings.json",
            ".claude/settings.local.json",
            ".codex/AGENTS.md",
            ".codex/auth.json",
            ".codex/config.toml",
            ".config/opencode/opencode.json",
            ".config/opencode/opencode.jsonc",
            ".local/share/opencode/auth.json",
            ".pi/agent/auth.json",
            ".pi/agent/models.json",
            ".pi/agent/settings.json",
        ];
        match self {
            Harness::Claude => CLAUDE,
            Harness::Codex => CODEX,
            Harness::Opencode => OPENCODE,
            Harness::Pi => PI,
            Harness::Axio => AXIO,
            Harness::Shell => SHELL,
        }
    }

    /// The executable to run: the harness on `PATH`, or the user's shell.
    pub fn program(self) -> Option<PathBuf> {
        match self {
            Harness::Shell => Some(PathBuf::from(
                std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into()),
            )),
            other => which(other.name()),
        }
    }

    /// Files the builder may generate in the isolated home; shown by `--dry-run`.
    pub fn generated_files(self) -> &'static [&'static str] {
        match self {
            Harness::Codex | Harness::Axio => &["config.toml"],
            Harness::Opencode => &["opencode.json"],
            Harness::Pi => &["models.json"],
            Harness::Claude | Harness::Shell => &[],
        }
    }
}

/// Everything a builder needs to know about this launch.
pub struct Context<'a> {
    pub url: &'a str,
    pub config: &'a Config,
    pub orchestrator: &'a str,
    pub worker: &'a str,
    /// The isolated home for this harness (`~/.config/lmrun/homes/<name>`).
    pub home: &'a Path,
    /// The user's real home directory.
    pub real_home: &'a Path,
    pub shared: bool,
    pub reseed: bool,
}

/// What a builder produces: environment to set, variables to drop, arguments
/// to put before the user's, and notes for stderr.
#[derive(Debug, Default)]
pub struct Launch {
    pub env: Vec<(String, String)>,
    pub unset: Vec<&'static str>,
    pub extra: Vec<String>,
    pub notes: Vec<String>,
}

impl Launch {
    fn set(&mut self, key: &str, value: impl Into<String>) {
        self.env.push((key.to_string(), value.into()));
    }

    #[cfg(test)]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

fn base(ctx: &Context) -> Launch {
    let mut l = Launch::default();
    l.unset.push("ANTHROPIC_API_KEY");
    let v1 = format!("{}/v1", ctx.url);
    l.set("LMRUN", "1");
    l.set("LMRUN_URL", ctx.url);
    l.set("LMRUN_ORCHESTRATOR", ctx.orchestrator);
    l.set("LMRUN_WORKER", ctx.worker);
    l.set("OMLX_API_KEY", &ctx.config.token);
    // Generic OpenAI-compatible clients (aider, llm, scripts).
    l.set("OPENAI_BASE_URL", &v1);
    l.set("OPENAI_API_BASE", &v1);
    l.set("OPENAI_API_KEY", &ctx.config.token);
    // Generic Anthropic-compatible clients.
    l.set("ANTHROPIC_BASE_URL", ctx.url);
    l.set("ANTHROPIC_AUTH_TOKEN", &ctx.config.token);
    l
}

pub fn build(harness: Harness, ctx: &Context) -> Result<Launch> {
    match harness {
        Harness::Claude => claude(ctx),
        Harness::Codex => codex(ctx),
        Harness::Opencode => opencode(ctx),
        Harness::Pi => pi(ctx),
        Harness::Axio => axio(ctx),
        Harness::Shell => shell(ctx),
    }
}

fn claude(ctx: &Context) -> Result<Launch> {
    let mut l = base(ctx);
    for key in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_FABLE_MODEL",
    ] {
        l.set(key, ctx.orchestrator);
    }
    l.set("ANTHROPIC_DEFAULT_HAIKU_MODEL", ctx.worker);
    l.set("CLAUDE_CODE_SUBAGENT_MODEL", ctx.worker);
    l.set("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1");
    l.set("DISABLE_TELEMETRY", "1");
    l.set("DISABLE_AUTOUPDATER", "1");
    if ctx.shared {
        return Ok(l);
    }
    l.set("CLAUDE_CONFIG_DIR", ctx.home.display().to_string());
    std::fs::create_dir_all(ctx.home)?;
    let real = ctx.real_home.join(".claude");
    const SEED: [&str; 5] = ["CLAUDE.md", "settings.json", "skills", "agents", "commands"];
    if ctx.reseed {
        for name in SEED {
            remove(&ctx.home.join(name));
        }
    }
    let mut seeded: Vec<&str> = SEED
        .into_iter()
        .filter(|name| copy_if_missing(&real.join(name), &ctx.home.join(name)).unwrap_or(false))
        .collect();
    let state_path = ctx.home.join(".claude.json");
    if !state_path.exists() {
        let mut state = json!({ "hasCompletedOnboarding": true });
        if let Ok(text) = std::fs::read_to_string(ctx.real_home.join(".claude.json"))
            && let Ok(Value::Object(real_state)) = serde_json::from_str::<Value>(&text)
        {
            for key in [
                "theme",
                "preferredNotifChannel",
                "editorMode",
                "autoUpdates",
            ] {
                if let Some(v) = real_state.get(key) {
                    state[key] = v.clone();
                }
            }
        }
        std::fs::write(&state_path, serde_json::to_string_pretty(&state)? + "\n")?;
        seeded.push(".claude.json");
    }
    if !seeded.is_empty() {
        l.notes.push(format!(
            "seeded isolated Claude home with: {}",
            seeded.join(", ")
        ));
    }
    Ok(l)
}

fn codex(ctx: &Context) -> Result<Launch> {
    let mut l = base(ctx);
    let model = if ctx.config.codex_model.is_empty() {
        ctx.worker
    } else {
        ctx.config.codex_model.as_str()
    };
    if ctx.shared {
        // Override on the command line only; never write ~/.codex/config.toml.
        for (k, v) in [
            ("model_provider", "\"omlx\"".to_string()),
            ("model", format!("\"{model}\"")),
            ("model_providers.omlx.name", "\"oMLX (local)\"".to_string()),
            (
                "model_providers.omlx.base_url",
                format!("\"{}/v1\"", ctx.url),
            ),
            (
                "model_providers.omlx.env_key",
                "\"OMLX_API_KEY\"".to_string(),
            ),
            ("model_providers.omlx.wire_api", "\"responses\"".to_string()),
        ] {
            l.extra.push("-c".into());
            l.extra.push(format!("{k}={v}"));
        }
        return Ok(l);
    }
    l.set("CODEX_HOME", ctx.home.display().to_string());
    std::fs::create_dir_all(ctx.home)?;
    let toml = format!(
        "# generated by axio-local - safe to delete\n\
         model = \"{model}\"\n\
         model_provider = \"omlx\"\n\
         model_context_window = {ctx_window}\n\
         model_reasoning_effort = \"medium\"\n\
         \n\
         [model_providers.omlx]\n\
         name = \"oMLX (local)\"\n\
         base_url = \"{url}/v1\"\n\
         env_key = \"OMLX_API_KEY\"\n\
         wire_api = \"responses\"\n\
         request_max_retries = 2\n\
         stream_idle_timeout_ms = 600000\n",
        ctx_window = ctx.config.context_window,
        url = ctx.url,
    );
    std::fs::write(ctx.home.join("config.toml"), toml)?;
    let agents = ctx.home.join("AGENTS.md");
    if ctx.reseed {
        remove(&agents);
    }
    if copy_if_missing(&ctx.real_home.join(".codex").join("AGENTS.md"), &agents)? {
        l.notes
            .push("seeded isolated Codex home with AGENTS.md".into());
    }
    Ok(l)
}

fn opencode(ctx: &Context) -> Result<Launch> {
    let mut l = base(ctx);
    if ctx.shared {
        l.extra.push("--model".into());
        l.extra.push(format!("omlx/{}", ctx.orchestrator));
        l.notes.push(
            "shared mode: provider 'omlx' must already exist in your real opencode.json".into(),
        );
        return Ok(l);
    }
    std::fs::create_dir_all(ctx.home)?;
    let mut models = serde_json::Map::new();
    for m in distinct([ctx.orchestrator, ctx.worker]) {
        models.insert(
            m.to_string(),
            json!({ "name": m, "limit": { "context": ctx.config.context_window, "output": ctx.config.max_output } }),
        );
    }
    let conf = json!({
        "$schema": "https://opencode.ai/config.json",
        "model": format!("omlx/{}", ctx.orchestrator),
        "small_model": format!("omlx/{}", ctx.worker),
        "provider": {
            "omlx": {
                "npm": "@ai-sdk/openai-compatible",
                "name": "oMLX (local)",
                "options": { "baseURL": format!("{}/v1", ctx.url), "apiKey": "{env:OMLX_API_KEY}" },
                "models": models,
            }
        }
    });
    let path = ctx.home.join("opencode.json");
    std::fs::write(&path, serde_json::to_string_pretty(&conf)? + "\n")?;
    l.set("OPENCODE_CONFIG", path.display().to_string());
    l.notes.push(
        "note: OpenCode still merges ~/.config/opencode/opencode.json underneath (read-only for us)".into(),
    );
    Ok(l)
}

fn pi(ctx: &Context) -> Result<Launch> {
    let mut l = base(ctx);
    l.extra.extend([
        "--provider".into(),
        "omlx".into(),
        "--model".into(),
        ctx.orchestrator.into(),
    ]);
    if ctx.shared {
        l.notes.push(
            "shared mode: provider 'omlx' must already exist in ~/.pi/agent/models.json".into(),
        );
        return Ok(l);
    }
    std::fs::create_dir_all(ctx.home)?;
    let models: Vec<Value> = distinct([ctx.orchestrator, ctx.worker])
        .into_iter()
        .map(|m| {
            json!({
                "id": m, "name": m,
                "contextWindow": ctx.config.context_window, "maxTokens": ctx.config.max_output,
                "input": ["text"],
                "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 }
            })
        })
        .collect();
    let conf = json!({
        "providers": {
            "omlx": {
                "baseUrl": format!("{}/v1", ctx.url),
                "api": "openai-completions",
                "apiKey": "$OMLX_API_KEY",
                "models": models
            }
        }
    });
    std::fs::write(
        ctx.home.join("models.json"),
        serde_json::to_string_pretty(&conf)? + "\n",
    )?;
    l.set("PI_CODING_AGENT_DIR", ctx.home.display().to_string());
    Ok(l)
}

fn axio(ctx: &Context) -> Result<Launch> {
    // axio: [model] provider/base_url/model in $AXIO_HOME/config.toml, or AXIO_* variables (env outranks file).
    let mut l = base(ctx);
    l.set("AXIO_PROVIDER", "openai-compatible");
    l.set("AXIO_BASE_URL", format!("{}/v1", ctx.url));
    l.set("AXIO_MODEL", ctx.orchestrator);
    l.set("OLLAMA_API_KEY", &ctx.config.token);
    if ctx.shared {
        l.notes.push(
            "shared mode: AXIO_* variables override ~/.axio/config.toml for this run only".into(),
        );
        return Ok(l);
    }
    l.set("AXIO_HOME", ctx.home.display().to_string());
    std::fs::create_dir_all(ctx.home)?;
    let toml = format!(
        "# generated by axio-local - safe to delete\n\
         [model]\n\
         provider = \"openai-compatible\"\n\
         base_url = \"{url}/v1\"\n\
         model = \"{model}\"\n\
         effort = \"medium\"\n\
         \n\
         [worktree]\n\
         enabled = false\n",
        url = ctx.url,
        model = ctx.orchestrator,
    );
    std::fs::write(ctx.home.join("config.toml"), toml)?;
    Ok(l)
}

fn shell(ctx: &Context) -> Result<Launch> {
    let mut l = base(ctx);
    let ps1 = std::env::var("PS1").unwrap_or_else(|_| "\\$ ".into());
    l.set("PS1", format!("(axio-local) {ps1}"));
    l.notes.push(
        "subshell: any tool you start here inherits the local-model env; exit to restore".into(),
    );
    Ok(l)
}

fn distinct(names: [&str; 2]) -> Vec<&str> {
    let mut v: Vec<&str> = names.to_vec();
    v.sort_unstable();
    v.dedup();
    v
}

fn remove(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

/// Copy `src` to `dst` (files or whole directories) when `src` exists and
/// `dst` does not. Returns whether anything was copied.
fn copy_if_missing(src: &Path, dst: &Path) -> Result<bool> {
    if !src.exists() || dst.exists() {
        return Ok(false);
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if src.is_dir() {
        copy_tree(src, dst)?;
    } else {
        std::fs::copy(src, dst).with_context(|| format!("copying {}", src.display()))?;
    }
    Ok(true)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_symlink() {
            let link = std::fs::read_link(entry.path())?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(link, &target)?;
            #[cfg(not(unix))]
            let _ = link;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Look a program up on `PATH`. A dependency-free `which`.
pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        config: &'a Config,
        home: &'a Path,
        real_home: &'a Path,
        shared: bool,
    ) -> Context<'a> {
        Context {
            url: "http://127.0.0.1:8000",
            config,
            orchestrator: "big",
            worker: "small",
            home,
            real_home,
            shared,
            reseed: false,
        }
    }

    #[test]
    fn claude_maps_aliases_to_the_two_roles_and_relocates_its_home() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join(".claude")).unwrap();
        std::fs::write(real.join(".claude").join("CLAUDE.md"), "rules").unwrap();
        std::fs::write(
            real.join(".claude.json"),
            r#"{"theme":"dark","secret":"no"}"#,
        )
        .unwrap();
        let home = dir.path().join("home");
        let config = Config::default();
        let l = build(Harness::Claude, &ctx(&config, &home, &real, false)).unwrap();
        assert_eq!(l.get("ANTHROPIC_DEFAULT_OPUS_MODEL"), Some("big"));
        assert_eq!(l.get("ANTHROPIC_DEFAULT_HAIKU_MODEL"), Some("small"));
        assert_eq!(l.get("CLAUDE_CODE_SUBAGENT_MODEL"), Some("small"));
        assert_eq!(l.get("CLAUDE_CONFIG_DIR"), Some(home.to_str().unwrap()));
        assert!(l.unset.contains(&"ANTHROPIC_API_KEY"));
        assert_eq!(
            std::fs::read_to_string(home.join("CLAUDE.md")).unwrap(),
            "rules"
        );
        let state: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join(".claude.json")).unwrap())
                .unwrap();
        assert_eq!(state["hasCompletedOnboarding"], true);
        assert_eq!(state["theme"], "dark");
        assert!(
            state.get("secret").is_none(),
            "only known keys are carried over"
        );
        // The real home was only read.
        assert_eq!(
            std::fs::read_to_string(real.join(".claude.json")).unwrap(),
            r#"{"theme":"dark","secret":"no"}"#
        );
    }

    #[test]
    fn codex_shared_mode_overrides_on_the_command_line_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let l = build(
            Harness::Codex,
            &ctx(&config, &dir.path().join("h"), dir.path(), true),
        )
        .unwrap();
        assert!(
            l.extra
                .iter()
                .any(|a| a == "model_providers.omlx.wire_api=\"responses\"")
        );
        assert!(l.get("CODEX_HOME").is_none());
        assert!(!dir.path().join("h").exists());
    }

    #[test]
    fn opencode_names_both_roles_once_each() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let home = dir.path().join("oc");
        let l = build(Harness::Opencode, &ctx(&config, &home, dir.path(), false)).unwrap();
        let conf: Value =
            serde_json::from_str(&std::fs::read_to_string(home.join("opencode.json")).unwrap())
                .unwrap();
        assert_eq!(conf["model"], "omlx/big");
        assert_eq!(conf["small_model"], "omlx/small");
        assert_eq!(
            conf["provider"]["omlx"]["models"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            l.get("OPENCODE_CONFIG"),
            Some(home.join("opencode.json").to_str().unwrap())
        );
    }

    #[test]
    fn axio_and_pi_generate_their_provider_files() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let ax = dir.path().join("axio");
        let l = build(Harness::Axio, &ctx(&config, &ax, dir.path(), false)).unwrap();
        assert_eq!(l.get("AXIO_HOME"), Some(ax.to_str().unwrap()));
        let toml = std::fs::read_to_string(ax.join("config.toml")).unwrap();
        assert!(toml.contains("provider = \"openai-compatible\""));
        assert!(toml.contains("model = \"big\""));
        let pi_home = dir.path().join("pi");
        let l = build(Harness::Pi, &ctx(&config, &pi_home, dir.path(), false)).unwrap();
        assert_eq!(l.extra, vec!["--provider", "omlx", "--model", "big"]);
        assert!(pi_home.join("models.json").exists());
    }

    #[test]
    fn every_harness_has_a_name_that_round_trips() {
        for h in Harness::ALL {
            assert_eq!(Harness::from_name(h.name()), Some(h));
        }
        assert_eq!(Harness::from_name("emacs"), None);
    }
}
