//! Finding the model server, asking what it serves, and starting it when it is local.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

use crate::config::Config;

const PORT: u16 = 8000;

/// The base URL to use, in precedence order: `LMRUN_URL`, the config, a
/// server already listening on this machine, then the host of a VM or
/// container this is running inside. Falls back to localhost so an error
/// names a concrete address.
pub fn detect_url(config: &Config) -> String {
    if let Ok(url) = std::env::var("LMRUN_URL")
        && !url.is_empty()
    {
        return url.trim_end_matches('/').to_string();
    }
    if let Some(url) = &config.url {
        return url.trim_end_matches('/').to_string();
    }
    if port_open("127.0.0.1", PORT) {
        return format!("http://127.0.0.1:{PORT}");
    }
    for host in ["host.internal", "host.orb.internal", "host.docker.internal"] {
        if port_open(host, PORT) {
            return format!("http://{host}:{PORT}");
        }
    }
    format!("http://127.0.0.1:{PORT}")
}

fn port_open(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok())
}

#[derive(Deserialize)]
struct ModelList {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    id: String,
}

/// The model IDs the server reports, or `None` when it is unreachable.
pub fn models(url: &str) -> Option<Vec<String>> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .new_agent();
    let list: ModelList = agent
        .get(format!("{url}/v1/models"))
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;
    Some(list.data.into_iter().map(|m| m.id).collect())
}

/// Make sure a server answers at `url`. When it does not, and the URL is local,
/// and this is macOS with an oMLX start script in place, start it and wait up
/// to a minute for `/v1/models`.
pub fn ensure_running(url: &str, home: &Path) -> bool {
    if models(url).is_some() {
        return true;
    }
    let script = home.join(".omlx").join("start.sh");
    if !cfg!(target_os = "macos") || !script.exists() || !url.contains("127.0.0.1") {
        return false;
    }
    eprintln!(
        "axio-local: oMLX not running, starting it via {}",
        script.display()
    );
    let log = home.join(".omlx").join("server.log");
    let Ok(log) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
    else {
        return false;
    };
    let Ok(err) = log.try_clone() else {
        return false;
    };
    let spawned = std::process::Command::new(&script)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(err)
        .spawn();
    if spawned.is_err() {
        return false;
    }
    for _ in 0..60 {
        std::thread::sleep(Duration::from_secs(1));
        if models(url).is_some() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_config_url_wins_and_loses_its_trailing_slash() {
        let config = Config {
            url: Some("http://box:9000/".into()),
            ..Config::default()
        };
        // LMRUN_URL is not set in the test environment.
        assert_eq!(detect_url(&config), "http://box:9000");
    }

    #[test]
    fn closed_port_is_reported_closed() {
        // Port 1 is reserved and nothing listens on it in any sane environment.
        assert!(!port_open("127.0.0.1", 1));
    }
}
