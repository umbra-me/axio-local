//! axio-local: launch coding agents against local models without touching their real configs.

mod config;
mod guard;
mod harness;
mod launch;
mod server;

use std::ffi::OsString;
use std::process::ExitCode;

use anyhow::{Result, bail};
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
    let config = Config::load_or_init(&paths.config)?;
    let opts = Opts {
        orchestrator: cli
            .orchestrator
            .clone()
            .unwrap_or_else(|| config.orchestrator.clone()),
        worker: cli.worker.clone().unwrap_or_else(|| config.worker.clone()),
        url: cli
            .url
            .as_deref()
            .map(|u| u.trim_end_matches('/').to_string()),
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
    report(&guard);
    let status = status?;
    Ok(status
        .code()
        .map(|c| ExitCode::from(c.clamp(0, 255) as u8))
        .unwrap_or(ExitCode::from(1)))
}

fn report(guard: &Guard) {
    let outcome = guard.restore();
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
    let mut env = launch.env.clone();
    env.sort();
    for (key, value) in env {
        if ENV_PREFIXES.iter().any(|p| key.starts_with(p)) {
            println!("export {key}={}", serde_json::to_string(&value)?);
        }
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
    let mut env = launch.env.clone();
    env.sort();
    for (key, value) in env {
        if [
            "LMRUN",
            "ANTHROPIC",
            "CLAUDE_CODE",
            "OPENAI",
            "OMLX",
            "DISABLE",
        ]
        .iter()
        .any(|p| key.starts_with(p))
        {
            println!("export {key}={}", serde_json::to_string(&value)?);
        }
    }
    for key in &launch.unset {
        println!("unset {key}");
    }
    Ok(ExitCode::SUCCESS)
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
    if home.is_dir() {
        std::fs::remove_dir_all(&home)?;
        println!("removed {}", home.display());
    } else {
        println!("nothing to remove at {}", home.display());
    }
    Ok(ExitCode::SUCCESS)
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
