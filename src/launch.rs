//! Run the harness as a child process and survive it: Ctrl+C reaches only the
//! child, SIGTERM and SIGHUP are forwarded, and whatever happens the caller
//! gets control back to restore configuration.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};

use crate::harness::Launch;

pub fn run(
    program: &Path,
    extra: &[String],
    args: &[OsString],
    launch: &Launch,
) -> Result<ExitStatus> {
    let mut cmd = Command::new(program);
    cmd.args(extra).args(args);
    for key in &launch.unset {
        cmd.env_remove(key);
    }
    for (key, value) in &launch.env {
        cmd.env(key, value);
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("launching {}", program.display()))?;

    // Installing a handler for SIGINT is what keeps this process alive when the
    // terminal sends Ctrl+C to the whole foreground group; the child still gets
    // its own copy and decides for itself.
    let interrupted = Arc::new(AtomicBool::new(false));
    let terminate = Arc::new(AtomicBool::new(false));
    let hangup = Arc::new(AtomicBool::new(false));
    let ids = [
        signal_hook::flag::register(SIGINT, Arc::clone(&interrupted))?,
        signal_hook::flag::register(SIGTERM, Arc::clone(&terminate))?,
        signal_hook::flag::register(SIGHUP, Arc::clone(&hangup))?,
    ];

    let pid = child.id() as libc::pid_t;
    let mut forwarded = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if !forwarded {
            let signal = if terminate.load(Ordering::Relaxed) {
                Some(SIGTERM)
            } else if hangup.load(Ordering::Relaxed) {
                Some(SIGHUP)
            } else {
                None
            };
            if let Some(signal) = signal {
                // SAFETY: plain syscall on a pid we own; the child has not been reaped.
                unsafe {
                    libc::kill(pid, signal);
                }
                forwarded = true;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    for id in ids {
        signal_hook::low_level::unregister(id);
    }
    Ok(status)
}
