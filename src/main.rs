//! axio-local: launch coding agents against local models without touching their real configs.

mod config;
mod guard;
mod harness;
mod launch;
mod locks;
mod server;

use std::ffi::OsString;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use config::{Config, Paths};
use guard::Guard;
use harness::{Harness, Launch};

const ENV_PREFIXES: [&str; 12] = [
    "LMRUN",
    "ANTHROPIC",
    "CLAUDE",
    "OPENAI",
    "OMLX",
    "CODEX",
    "OPENCODE",
    "PI_",
    "AXIO",
    "OLLAMA",
    "DISABLE",
    "PS1",
];

#[derive(Parser)]
#[command(
    name = "axio-local",
    version,
    about = "Run coding agents on local models without touching their real configs.",
    long_about = "Each harness runs with an isolated configuration home plus process-scoped \
environment variables, so its real configuration is never written. As a safety net the real \
configuration files are snapshotted before launch and restored on exit if anything changed them.\n\n\
Options go before the harness name; everything after it is passed through.",
    after_help = "Config: ~/.config/lmrun/config.json    Env override: LMRUN_URL"
)]
struct Cli {
    /// Named local server/model profile from config.json
    #[arg(long, global = true)]
    profile: Option<String>,
    /// The main model for this launch (default from config)
    #[arg(short = 'o', long, global = true, value_name = "MODEL")]
    orchestrator: Option<String>,
    /// The model subagents and light tasks use (default from config)
    #[arg(short = 'w', long, global = true, value_name = "MODEL")]
    worker: Option<String>,
    /// Server base URL (default: auto-detect)
    #[arg(long, global = true, value_name = "URL")]
    url: Option<String>,
    /// Use the REAL config dir instead of an isolated one; restore on exit still applies
    #[arg(long, global = true)]
    shared: bool,
    /// Re-copy CLAUDE.md, settings, skills, agents and AGENTS.md into the isolated home
    #[arg(long, global = true)]
    reseed: bool,
    /// Show the environment and generated config, launch nothing
    #[arg(long, global = true)]
    dry_run: bool,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Claude Code (CLAUDE_CONFIG_DIR isolated)
    Claude {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Codex CLI (CODEX_HOME isolated)
    Codex {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// OpenCode (OPENCODE_CONFIG isolated)
    Opencode {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Pi agent (PI_CODING_AGENT_DIR isolated)
    Pi {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Axio (AXIO_HOME isolated)
    Axio {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// A subshell with the local-model env exported; pass a command to run it instead
    Shell {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    /// Print export lines: eval "$(axio-local env)"
    Env,
    /// Server reachability and which models it serves
    Status,
    /// Start oMLX if it is not running (macOS)
    Serve,
    /// Harnesses found on PATH and their isolated homes
    List,
    /// Delete a harness's isolated home
    Reset { harness: String },
    /// List the snapshots the safety net has taken
    Snapshots,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("axio-local: {err:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let paths = Paths::discover()?;
    let mut config = Config::load_or_init(&paths.config)?;
    if let Some(name) = &cli.profile {
        config.apply_profile(name)?;
    }
    let opts = Opts {
        orchestrator: cli
            .orchestrator
            .clone()
            .unwrap_or_else(|| config.orchestrator.clone()),
        worker: cli.worker.clone().unwrap_or_else(|| config.worker.clone()),
        url: cli
            .url
            .as_deref()
            .map(|u| u.trim_end_matches('/').to_string())
            .or_else(|| cli.profile.as_ref().and(config.url.clone())),
        shared: cli.shared,
        reseed: cli.reseed,
        dry_run: cli.dry_run,
    };
    match cli.command {
        Cmd::Claude { args } => launch_harness(Harness::Claude, args, &opts, &config, &paths),
        Cmd::Codex { args } => launch_harness(Harness::Codex, args, &opts, &config, &paths),
        Cmd::Opencode { args } => launch_harness(Harness::Opencode, args, &opts, &config, &paths),
        Cmd::Pi { args } => launch_harness(Harness::Pi, args, &opts, &config, &paths),
        Cmd::Axio { args } => launch_harness(Harness::Axio, args, &opts, &config, &paths),
        Cmd::Shell { args } => launch_harness(Harness::Shell, args, &opts, &config, &paths),
        Cmd::Env => env(&opts, &config, &paths),
        Cmd::Status => status(&opts, &config, &paths),
        Cmd::Serve => serve(&opts, &config, &paths),
        Cmd::List => list(&paths),
        Cmd::Reset { harness } => reset(&harness, &paths),
        Cmd::Snapshots => snapshots(&paths),
    }
}

struct Opts {
    orchestrator: String,
    worker: String,
    url: Option<String>,
    shared: bool,
    reseed: bool,
    dry_run: bool,
}

impl Opts {
    fn url(&self, config: &Config) -> String {
        self.url
            .clone()
            .unwrap_or_else(|| server::detect_url(config))
            .trim_end_matches('/')
            .to_string()
    }
}

fn launch_harness(
    harness: Harness,
    args: Vec<OsString>,
    opts: &Opts,
    config: &Config,
    paths: &Paths,
) -> Result<ExitCode> {
    let url = opts.url(config);
    if !opts.dry_run {
        if !server::ensure_running(&url, &paths.home) {
            bail!(
                "no model server reachable at {url} (start it with: axio-local serve, or set --url / LMRUN_URL)"
            );
        }
        let served = server::models(&url).unwrap_or_default();
        for (role, model) in [
            ("orchestrator", &opts.orchestrator),
            ("worker", &opts.worker),
        ] {
            if !served.iter().any(|m| m == model) {
                eprintln!(
                    "axio-local: WARNING: {role} model {model:?} is not served by {url} (available: {})",
                    if served.is_empty() {
                        "none".to_string()
                    } else {
                        served.join(", ")
                    }
                );
            }
        }
    }
    let Some(program) = harness.program() else {
        bail!("{} is not on PATH", harness.name());
    };
    let home = paths.harness_home(harness.name());
    let _locks =
        locks::LaunchLocks::acquire(&paths.home, &paths.snapshots, &home, harness.protected())?;
    let ctx = harness::Context {
        url: &url,
        config,
        orchestrator: &opts.orchestrator,
        worker: &opts.worker,
        home: &home,
        real_home: &paths.home,
        shared: opts.shared,
        reseed: opts.reseed,
    };
    let launch = harness::build(harness, &ctx)?;
    for note in &launch.notes {
        eprintln!("axio-local: {note}");
    }
    eprintln!(
        "axio-local: server={url}  orchestrator={}  worker={}  mode={}",
        opts.orchestrator,
        opts.worker,
        if opts.shared {
            "SHARED real config".to_string()
        } else {
            format!("isolated home {}", home.display())
        }
    );

    // `axio-local shell 'cmd'` runs the command through the shell instead of opening one.
    let (extra, args): (Vec<String>, Vec<OsString>) =
        if harness == Harness::Shell && !args.is_empty() {
            let joined = args
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ");
            (vec!["-c".into(), joined], Vec::new())
        } else {
            (launch.extra.clone(), args)
        };

    if opts.dry_run {
        dry_run(
            &program,
            &extra,
            &args,
            &launch,
            harness,
            &home,
            opts.shared,
        )?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut guard = Guard::new(
        &paths.home,
        &paths.snapshots,
        harness.name(),
        harness.protected(),
        config.keep_snapshots,
    );
    guard.take()?;
    if opts.shared {
        eprintln!(
            "axio-local: SHARED mode: the real config dir is in use; it will be restored from {} on exit",
            guard.snapshot.display()
        );
    }
    let status = launch::run(&program, &extra, &args, &launch);
    report(&guard)?;
    let status = status?;
    Ok(status
        .code()
        .map(|c| ExitCode::from(c.clamp(0, 255) as u8))
        .unwrap_or(ExitCode::from(1)))
}

fn report(guard: &Guard) -> Result<()> {
    let outcome = guard.restore();
    if !outcome.failed.is_empty() {
        bail!(
            "recovery failed for {} file(s); snapshot retained at {}",
            outcome.failed.len(),
            guard.snapshot.display()
        );
    }
    if !outcome.restored.is_empty() {
        eprintln!(
            "axio-local: RESTORED {} real config file(s) the harness modified:",
            outcome.restored.len()
        );
        for p in &outcome.restored {
            eprintln!(
                "axio-local:   {}  (snapshot kept in {})",
                p.display(),
                guard.snapshot.display()
            );
        }
    }
    if !outcome.created.is_empty() {
        let names: Vec<String> = outcome
            .created
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        eprintln!(
            "axio-local: note: new file(s) appeared in real config dirs (left in place): {}",
            names.join(", ")
        );
    }
    if outcome.restored.is_empty() && outcome.created.is_empty() {
        eprintln!(
            "axio-local: real configs untouched ({} file(s) verified)",
            outcome.verified
        );
    }
    Ok(())
}

fn dry_run(
    program: &std::path::Path,
    extra: &[String],
    args: &[OsString],
    launch: &Launch,
    harness: Harness,
    home: &std::path::Path,
    shared: bool,
) -> Result<()> {
    let mut argv = vec![program.display().to_string()];
    argv.extend(extra.iter().cloned());
    argv.extend(args.iter().map(|a| a.to_string_lossy().into_owned()));
    println!("# argv: {}", argv.join(" "));
    for key in &launch.unset {
        println!("unset {key}");
    }
    for line in export_lines(launch, &ENV_PREFIXES) {
        println!("{line}");
    }
    if !shared {
        for name in harness.generated_files() {
            let path = home.join(name);
            if let Ok(text) = std::fs::read_to_string(&path) {
                println!("\n# {}\n{text}", path.display());
            }
        }
    }
    Ok(())
}

fn env(opts: &Opts, config: &Config, paths: &Paths) -> Result<ExitCode> {
    let url = opts.url(config);
    let home = paths.harness_home("claude");
    let ctx = harness::Context {
        url: &url,
        config,
        orchestrator: &opts.orchestrator,
        worker: &opts.worker,
        home: &home,
        real_home: &paths.home,
        shared: true,
        reseed: false,
    };
    let launch = harness::build(Harness::Claude, &ctx)?;
    for line in export_lines(
        &launch,
        &[
            "LMRUN",
            "ANTHROPIC",
            "CLAUDE_CODE",
            "OPENAI",
            "OMLX",
            "DISABLE",
        ],
    ) {
        println!("{line}");
    }
    for key in &launch.unset {
        println!("unset {key}");
    }
    Ok(ExitCode::SUCCESS)
}

/// `export` lines, sorted, for the variables whose names start with one of
/// `prefixes`. The values come from config.json and the command line, and
/// the output is meant for `eval`, so each one is a single quoted word.
fn export_lines(launch: &Launch, prefixes: &[&str]) -> Vec<String> {
    let mut env = launch.env.clone();
    env.sort();
    env.into_iter()
        .filter(|(key, _)| prefixes.iter().any(|p| key.starts_with(p)))
        .map(|(key, value)| format!("export {key}={}", shell_quote(&value)))
        .collect()
}

/// `value` as one POSIX shell word that expands to exactly `value`: nothing
/// inside single quotes is expanded, and a quote in the value closes the
/// run, adds an escaped quote and opens a new one.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn status(opts: &Opts, config: &Config, paths: &Paths) -> Result<ExitCode> {
    let url = opts.url(config);
    let served = server::models(&url);
    let up = served.is_some();
    let flag = |model: &str| match &served {
        Some(list) if !list.iter().any(|m| m == model) => "  [NOT SERVED]",
        _ => "",
    };
    println!("server:       {url}  ({})", if up { "UP" } else { "DOWN" });
    println!(
        "orchestrator: {}{}",
        opts.orchestrator,
        flag(&opts.orchestrator)
    );
    println!("worker:       {}{}", opts.worker, flag(&opts.worker));
    if let Some(list) = &served
        && !list.is_empty()
    {
        println!("served:       {}", list.join(", "));
    }
    println!("config:       {}", paths.config.display());
    Ok(if up {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn serve(opts: &Opts, config: &Config, paths: &Paths) -> Result<ExitCode> {
    let url = opts.url(config);
    if server::ensure_running(&url, &paths.home) {
        println!(
            "server up at {url}: {}",
            server::models(&url).unwrap_or_default().join(", ")
        );
        return Ok(ExitCode::SUCCESS);
    }
    bail!("could not start a server for {url}");
}

fn list(paths: &Paths) -> Result<ExitCode> {
    for h in Harness::ALL {
        if h == Harness::Shell {
            continue;
        }
        let exe = h
            .program()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(not on PATH)".into());
        let home = paths.harness_home(h.name());
        println!(
            "{:<9} {:<50} isolated home: {}",
            h.name(),
            exe,
            if home.is_dir() { "yes" } else { "not yet" }
        );
    }
    println!("shell     subshell with env exported");
    Ok(ExitCode::SUCCESS)
}

fn reset(name: &str, paths: &Paths) -> Result<ExitCode> {
    let Some(h) = Harness::from_name(name) else {
        bail!("unknown harness {name:?}");
    };
    let home = paths.harness_home(h.name());
    let _lock = reset_lock(&home, paths)
        .with_context(|| format!("{} is running; close it before a reset", h.name()))?;
    if home.is_dir() {
        std::fs::remove_dir_all(&home)?;
        println!("removed {}", home.display());
    } else {
        println!("nothing to remove at {}", home.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// The lock a launch holds on its home for as long as it runs, so a reset
/// never deletes a home in use and no launch starts in one being deleted.
fn reset_lock(home: &std::path::Path, paths: &Paths) -> Result<locks::LaunchLocks> {
    locks::LaunchLocks::acquire(&paths.home, &paths.snapshots, home, &[])
}

fn snapshots(paths: &Paths) -> Result<ExitCode> {
    let Ok(entries) = std::fs::read_dir(&paths.snapshots) else {
        println!("no snapshots");
        return Ok(ExitCode::SUCCESS);
    };
    let mut dirs: Vec<_> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    dirs.sort();
    for d in dirs {
        println!("{}", d.display());
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn profile_endpoint_is_normalized_before_appending_api_paths() {
        let mut config = Config::default();
        config.profiles.insert(
            "workstation".into(),
            config::Profile {
                url: "http://localhost:9000/proxy/".into(),
                orchestrator: "planner".into(),
                worker: "worker".into(),
            },
        );
        config.apply_profile("workstation").unwrap();
        let mut opts = Opts {
            orchestrator: config.orchestrator.clone(),
            worker: config.worker.clone(),
            url: config.url.clone(),
            shared: false,
            reseed: false,
            dry_run: true,
        };
        assert_eq!(
            format!("{}/v1/models", opts.url(&config)),
            "http://localhost:9000/proxy/v1/models"
        );
        opts.url = Some("http://localhost:8000/".into());
        assert_eq!(opts.url(&config), "http://localhost:8000");
    }

    #[test]
    fn failed_recovery_returns_an_error_to_the_exit_path() {
        let d = tempfile::tempdir().unwrap();
        let config = d.path().join("config");
        std::fs::write(&config, "original").unwrap();
        let mut guard = Guard::new(
            d.path(),
            &d.path().join("snapshots"),
            "test",
            &["config"],
            10,
        );
        guard.take().unwrap();
        std::fs::write(&config, "changed").unwrap();
        std::fs::remove_file(guard.snapshot.join("config")).unwrap();
        assert!(report(&guard).is_err());
    }

    /// What a POSIX shell makes of a word built only from single-quoted runs
    /// and `\'`. Anything else in the word would be open to expansion, so it
    /// is an error here. Nothing is ever handed to a real shell.
    fn unquote(word: &str) -> std::result::Result<String, String> {
        let mut out = String::new();
        let mut chars = word.chars();
        while let Some(c) = chars.next() {
            match c {
                '\'' => loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(c) => out.push(c),
                        None => return Err(format!("unterminated quote in {word:?}")),
                    }
                },
                '\\' => match chars.next() {
                    Some('\'') => out.push('\''),
                    other => return Err(format!("unexpected escape {other:?} in {word:?}")),
                },
                other => return Err(format!("unquoted {other:?} in {word:?}")),
            }
        }
        Ok(out)
    }

    const HOSTILE: [&str; 12] = [
        "",
        "plain",
        "http://localhost:8000",
        "$(touch /tmp/axio-local-pwned)",
        "`touch /tmp/axio-local-pwned`",
        "${HOME}",
        "it's",
        "'; touch /tmp/axio-local-pwned; '",
        "\"double\" and \\backslash\\",
        "line one\nline two",
        "tab\tand !bang and *glob*",
        "\u{e9}t\u{e9} \u{1f600}",
    ];

    #[test]
    fn shell_quoting_round_trips_every_value_literally() {
        for value in HOSTILE {
            assert_eq!(unquote(&shell_quote(value)).unwrap(), value);
        }
    }

    #[test]
    fn env_output_keeps_config_values_out_of_the_shell() {
        let launch = Launch {
            env: HOSTILE
                .iter()
                .enumerate()
                .map(|(index, value)| (format!("LMRUN_TEST_{index:02}"), value.to_string()))
                .collect(),
            ..Launch::default()
        };
        let lines = export_lines(&launch, &["LMRUN"]);
        assert_eq!(lines.len(), HOSTILE.len());
        for (line, (key, value)) in lines.iter().zip(&launch.env) {
            let word = line
                .strip_prefix(&format!("export {key}="))
                .unwrap_or_else(|| panic!("{line:?}"));
            assert_eq!(&unquote(word).unwrap(), value);
        }
    }

    #[test]
    fn export_lines_only_print_the_selected_prefixes() {
        let launch = Launch {
            env: vec![
                ("LMRUN_URL".into(), "http://localhost".into()),
                ("PATH".into(), "/usr/bin".into()),
            ],
            ..Launch::default()
        };
        assert_eq!(
            export_lines(&launch, &["LMRUN"]),
            ["export LMRUN_URL='http://localhost'"]
        );
    }

    #[test]
    fn reset_refuses_a_home_that_a_launch_holds() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::from_home(d.path().to_path_buf());
        let home = paths.harness_home("claude");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("settings.json"), "{}").unwrap();
        let held = locks::LaunchLocks::acquire(
            &paths.home,
            &paths.snapshots,
            &home,
            Harness::Claude.protected(),
        )
        .unwrap();
        assert!(reset("claude", &paths).is_err());
        assert!(home.join("settings.json").exists());
        drop(held);
        reset("claude", &paths).unwrap();
        assert!(!home.exists());
    }

    #[test]
    fn a_launch_cannot_start_while_reset_holds_the_home() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::from_home(d.path().to_path_buf());
        let home = paths.harness_home("claude");
        let held = reset_lock(&home, &paths).unwrap();
        assert!(
            locks::LaunchLocks::acquire(
                &paths.home,
                &paths.snapshots,
                &home,
                Harness::Claude.protected()
            )
            .is_err()
        );
        drop(held);
    }
}
