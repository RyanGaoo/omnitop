//! Docker/Podman container monitoring over the runtime's API socket.
//!
//! The transport is a Unix domain socket and is therefore `cfg(unix)`-only. On other
//! platforms (e.g. Windows, where Docker is reached over a named pipe) the public API
//! still exists but reports that container monitoring isn't available yet.

/// A single container with its live resource usage.
#[derive(Clone)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub cpu: f64,
    pub mem_used: u64,
    pub mem_limit: u64,
}

/// Latest known state of the container runtime.
#[cfg_attr(not(unix), allow(dead_code))]
pub enum DockerState {
    /// No socket found — Docker/Podman is not installed or not running.
    Disabled,
    /// A socket was found but the request failed.
    Error(String),
    /// Successfully fetched the container list.
    Containers(Vec<Container>),
}

/// A lifecycle action that can be triggered against a container.
#[derive(Clone, Copy)]
pub enum ContainerAction {
    Stop,
    Restart,
}

impl ContainerAction {
    #[cfg_attr(not(unix), allow(dead_code))]
    fn verb(self) -> &'static str {
        match self {
            ContainerAction::Stop => "stop",
            ContainerAction::Restart => "restart",
        }
    }

    /// Present-tense label for the in-progress status message.
    pub fn gerund(self) -> &'static str {
        match self {
            ContainerAction::Stop => "Stopping",
            ContainerAction::Restart => "Restarting",
        }
    }

    /// Past-tense label for the completion status message.
    pub fn past(self) -> &'static str {
        match self {
            ContainerAction::Stop => "stopped",
            ContainerAction::Restart => "restarted",
        }
    }
}

/// Non-Unix stub: report once that container monitoring needs a Unix socket, then idle.
#[cfg(not(unix))]
pub fn spawn_poller(_interval: std::time::Duration) -> std::sync::mpsc::Receiver<DockerState> {
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = tx.send(DockerState::Error(
        "container monitoring requires a Unix socket; Windows (named pipe) support is planned"
            .to_string(),
    ));
    rx
}

#[cfg(not(unix))]
pub fn run_action(_action: ContainerAction, _id: &str) -> Result<(), String> {
    Err("container actions are not supported on this platform yet".to_string())
}

#[cfg(unix)]
pub use unix::{run_action, spawn_poller};

#[cfg(unix)]
mod unix {
    use super::{Container, ContainerAction, DockerState};
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::{self, Receiver};
    use std::thread;
    use std::time::Duration;

    use serde::Deserialize;

    /// Stop or restart a container by id. Blocking (a stop can take ~10s while Docker
    /// waits out the SIGTERM grace period), so callers must run this off the UI thread.
    pub fn run_action(action: ContainerAction, id: &str) -> Result<(), String> {
        let socket = find_socket().ok_or("no Docker/Podman socket found")?;
        let path = format!("/containers/{id}/{}", action.verb());
        match post(&socket, &path)? {
            // 204 = success; 304 = container already in the requested state.
            200..=299 | 304 => Ok(()),
            404 => Err("container not found".into()),
            status => Err(format!("HTTP {status}")),
        }
    }

    /// Spawn a background thread that polls the container runtime on an interval and
    /// sends updates over a channel. Keeps the UI responsive since the Docker stats
    /// endpoint can block for ~1s per container.
    pub fn spawn_poller(interval: Duration) -> Receiver<DockerState> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || loop {
            let state = match find_socket() {
                Some(sock) => match poll(&sock) {
                    Ok(containers) => DockerState::Containers(containers),
                    Err(e) => DockerState::Error(e),
                },
                None => DockerState::Disabled,
            };
            if tx.send(state).is_err() {
                break; // receiver dropped: app exited
            }
            thread::sleep(interval);
        });
        rx
    }

    /// Locate a Docker or Podman Unix socket across common install locations.
    fn find_socket() -> Option<PathBuf> {
        if let Some(host) = std::env::var_os("DOCKER_HOST") {
            if let Some(path) = host.to_string_lossy().strip_prefix("unix://") {
                let pb = PathBuf::from(path);
                if pb.exists() {
                    return Some(pb);
                }
            }
        }

        let mut candidates = vec![PathBuf::from("/var/run/docker.sock")];
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            candidates.push(home.join(".docker/run/docker.sock"));
            candidates.push(home.join(".colima/default/docker.sock"));
        }
        if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
            candidates.push(PathBuf::from(xdg).join("podman/podman.sock"));
        }
        candidates.push(PathBuf::from("/run/podman/podman.sock"));

        candidates.into_iter().find(|p| p.exists())
    }

    fn poll(socket: &Path) -> Result<Vec<Container>, String> {
        let body = request(socket, "/containers/json")?;
        let summaries: Vec<ContainerSummary> =
            serde_json::from_slice(&body).map_err(|e| format!("parse containers: {e}"))?;

        let mut containers = Vec::with_capacity(summaries.len());
        for s in summaries {
            let name = s
                .names
                .first()
                .map(|n| n.trim_start_matches('/').to_string())
                .unwrap_or_else(|| short_id(&s.id));

            let (cpu, mem_used, mem_limit) =
                match request(socket, &format!("/containers/{}/stats?stream=false", s.id)) {
                    Ok(b) => serde_json::from_slice::<Stats>(&b)
                        .map(|st| st.compute())
                        .unwrap_or((0.0, 0, 0)),
                    Err(_) => (0.0, 0, 0),
                };

            containers.push(Container {
                id: short_id(&s.id),
                name,
                image: s.image,
                status: s.status,
                cpu,
                mem_used,
                mem_limit,
            });
        }
        Ok(containers)
    }

    fn short_id(id: &str) -> String {
        id.chars().take(12).collect()
    }

    /// Perform a minimal HTTP/1.1 GET over the Unix socket and return the response body.
    /// Handles both `Content-Length`/connection-close bodies and `Transfer-Encoding: chunked`.
    fn request(socket: &Path, path: &str) -> Result<Vec<u8>, String> {
        let mut stream = UnixStream::connect(socket).map_err(|e| format!("connect: {e}"))?;
        stream.set_read_timeout(Some(Duration::from_secs(6))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(6))).ok();

        let req = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
        stream
            .write_all(req.as_bytes())
            .map_err(|e| format!("write: {e}"))?;

        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .map_err(|e| format!("read: {e}"))?;

        let sep = find(&raw, b"\r\n\r\n").ok_or("malformed response: no header terminator")?;
        let header = String::from_utf8_lossy(&raw[..sep]).to_string();
        let body_raw = &raw[sep + 4..];

        let status = header
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .unwrap_or(0);

        let chunked = header.to_lowercase().contains("transfer-encoding: chunked");
        let body = if chunked {
            dechunk(body_raw)?
        } else {
            body_raw.to_vec()
        };

        if !(200..300).contains(&status) {
            let msg = String::from_utf8_lossy(&body);
            return Err(format!("HTTP {status}: {}", msg.trim()));
        }
        Ok(body)
    }

    /// Perform a minimal HTTP/1.1 POST (no body) over the Unix socket and return the
    /// response status code. Used for container lifecycle actions, which return 204/304.
    fn post(socket: &Path, path: &str) -> Result<u16, String> {
        let mut stream = UnixStream::connect(socket).map_err(|e| format!("connect: {e}"))?;
        // A stop waits out the SIGTERM grace period (~10s), so allow generous read time.
        stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(6))).ok();

        let req = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
        stream
            .write_all(req.as_bytes())
            .map_err(|e| format!("write: {e}"))?;

        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .map_err(|e| format!("read: {e}"))?;

        let sep = find(&raw, b"\r\n\r\n").ok_or("malformed response: no header terminator")?;
        let header = String::from_utf8_lossy(&raw[..sep]);
        header
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| "malformed status line".to_string())
    }

    /// Decode an HTTP chunked-transfer-encoded body.
    fn dechunk(mut body: &[u8]) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        loop {
            let nl = find(body, b"\r\n").ok_or("malformed chunk: missing size line")?;
            let size_line = std::str::from_utf8(&body[..nl]).map_err(|_| "invalid chunk size")?;
            let size_hex = size_line.split(';').next().unwrap_or("").trim();
            let size = usize::from_str_radix(size_hex, 16).map_err(|_| "invalid chunk size hex")?;
            body = &body[nl + 2..];
            if size == 0 {
                break;
            }
            if body.len() < size {
                return Err("truncated chunk body".into());
            }
            out.extend_from_slice(&body[..size]);
            body = &body[size..];
            if body.len() >= 2 {
                body = &body[2..]; // trailing CRLF after chunk data
            }
        }
        Ok(out)
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    // ---- Docker API response shapes (only the fields we use) ----

    #[derive(Deserialize)]
    struct ContainerSummary {
        #[serde(rename = "Id")]
        id: String,
        #[serde(rename = "Names", default)]
        names: Vec<String>,
        #[serde(rename = "Image", default)]
        image: String,
        #[serde(rename = "Status", default)]
        status: String,
    }

    #[derive(Deserialize, Default)]
    struct Stats {
        #[serde(default)]
        cpu_stats: CpuStats,
        #[serde(default)]
        precpu_stats: CpuStats,
        #[serde(default)]
        memory_stats: MemStats,
    }

    impl Stats {
        /// Compute CPU percentage (Docker's formula) plus memory usage/limit.
        fn compute(&self) -> (f64, u64, u64) {
            let cpu_delta =
                self.cpu_stats
                    .cpu_usage
                    .total_usage
                    .saturating_sub(self.precpu_stats.cpu_usage.total_usage) as f64;
            let sys_delta =
                self.cpu_stats
                    .system_cpu_usage
                    .saturating_sub(self.precpu_stats.system_cpu_usage) as f64;
            let ncpu = if self.cpu_stats.online_cpus > 0 {
                self.cpu_stats.online_cpus
            } else {
                self.cpu_stats.cpu_usage.percpu_usage.len() as u64
            }
            .max(1);

            let cpu = if sys_delta > 0.0 && cpu_delta > 0.0 {
                (cpu_delta / sys_delta) * ncpu as f64 * 100.0
            } else {
                0.0
            };
            (cpu, self.memory_stats.usage, self.memory_stats.limit)
        }
    }

    #[derive(Deserialize, Default)]
    struct CpuStats {
        #[serde(default)]
        cpu_usage: CpuUsage,
        #[serde(default)]
        system_cpu_usage: u64,
        #[serde(default)]
        online_cpus: u64,
    }

    #[derive(Deserialize, Default)]
    struct CpuUsage {
        #[serde(default)]
        total_usage: u64,
        #[serde(default)]
        percpu_usage: Vec<u64>,
    }

    #[derive(Deserialize, Default)]
    struct MemStats {
        #[serde(default)]
        usage: u64,
        #[serde(default)]
        limit: u64,
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn find_locates_subsequence() {
            assert_eq!(find(b"abc\r\n\r\ndef", b"\r\n\r\n"), Some(3));
            assert_eq!(find(b"no terminator", b"\r\n\r\n"), None);
        }

        #[test]
        fn dechunk_decodes_multiple_chunks() {
            // "Wiki" (4) + "pedia" (5) + terminating 0-chunk
            let body = b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
            assert_eq!(dechunk(body).unwrap(), b"Wikipedia");
        }

        #[test]
        fn dechunk_handles_empty_body() {
            assert_eq!(dechunk(b"0\r\n\r\n").unwrap(), b"");
        }

        #[test]
        fn dechunk_reports_truncation() {
            // Declares 10 bytes but only provides 3.
            assert!(dechunk(b"a\r\nabc").is_err());
        }

        #[test]
        fn compute_cpu_uses_docker_formula() {
            let stats = Stats {
                cpu_stats: CpuStats {
                    cpu_usage: CpuUsage {
                        total_usage: 2_000,
                        percpu_usage: vec![],
                    },
                    system_cpu_usage: 20_000,
                    online_cpus: 4,
                },
                precpu_stats: CpuStats {
                    cpu_usage: CpuUsage {
                        total_usage: 1_000,
                        percpu_usage: vec![],
                    },
                    system_cpu_usage: 10_000,
                    online_cpus: 4,
                },
                memory_stats: MemStats {
                    usage: 1_048_576,
                    limit: 4_194_304,
                },
            };
            // (1000/10000) * 4 * 100 = 40%
            let (cpu, used, limit) = stats.compute();
            assert!((cpu - 40.0).abs() < 1e-9);
            assert_eq!(used, 1_048_576);
            assert_eq!(limit, 4_194_304);
        }

        #[test]
        fn compute_cpu_falls_back_to_percpu_count() {
            let stats = Stats {
                cpu_stats: CpuStats {
                    cpu_usage: CpuUsage {
                        total_usage: 1_500,
                        percpu_usage: vec![0, 0],
                    },
                    system_cpu_usage: 10_000,
                    online_cpus: 0,
                },
                precpu_stats: CpuStats {
                    cpu_usage: CpuUsage {
                        total_usage: 1_000,
                        percpu_usage: vec![0, 0],
                    },
                    system_cpu_usage: 5_000,
                    online_cpus: 0,
                },
                memory_stats: MemStats::default(),
            };
            // (500/5000) * 2 * 100 = 20%
            let (cpu, _, _) = stats.compute();
            assert!((cpu - 20.0).abs() < 1e-9);
        }

        #[test]
        fn compute_cpu_zero_when_no_system_delta() {
            let stats = Stats::default();
            assert_eq!(stats.compute().0, 0.0);
        }

        /// End-to-end check against a real daemon. Ignored by default (needs Docker/Podman
        /// running). Run with: `cargo test -- --ignored --nocapture live_poll`
        #[test]
        #[ignore = "requires a running Docker/Podman daemon"]
        fn live_poll_finds_containers() {
            let socket = find_socket().expect("no Docker/Podman socket found");
            println!("using socket: {}", socket.display());
            let containers = poll(&socket).expect("poll failed");
            println!("found {} containers:", containers.len());
            for c in &containers {
                println!(
                    "  {:12}  {:16}  {:20}  cpu={:6.2}%  mem={}/{}",
                    c.id, c.name, c.image, c.cpu, c.mem_used, c.mem_limit
                );
            }
            assert!(
                !containers.is_empty(),
                "expected at least one running container"
            );
        }

        /// Exercises the real POST action path. Restart is reversible (the container comes
        /// back up), so it's safe to run against a demo container. Ignored by default.
        /// Run with: `cargo test -- --ignored --nocapture live_restart`
        #[test]
        #[ignore = "requires a running Docker daemon with a container named omnitop-redis"]
        fn live_restart_action() {
            // Docker's lifecycle endpoints accept a name in place of an id.
            run_action(ContainerAction::Restart, "omnitop-redis").expect("restart action failed");
            println!("restart action succeeded");
        }
    }
}
