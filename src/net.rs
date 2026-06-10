//! Per-process network throughput.
//!
//! On macOS this shells out to the built-in `nettop`, which reports cumulative
//! per-process byte counters **without root** for the current user's processes. A
//! background thread samples on an interval and diffs consecutive readings to derive
//! per-process rates, pushing them to the UI over a channel.
//!
//! Linux (eBPF / nethogs-style capture) is a planned follow-up; on unsupported
//! platforms the poller simply never emits.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[cfg(all(test, target_os = "macos"))]
mod live_tests {
    use super::*;
    use std::time::Instant;

    /// Exercises the full poller pipeline (nettop -> parse -> diff -> channel) against
    /// live traffic. Ignored by default. Run with:
    /// `cargo test -- --ignored --nocapture live_net`
    #[test]
    #[ignore = "samples live network traffic; run manually"]
    fn live_net_rates() {
        let rx = spawn_poller(Duration::from_secs(1));
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut samples = 0;
        let mut peak = (0u64, 0u64);
        let mut top: Option<(u32, NetRate)> = None;

        while Instant::now() < deadline {
            if let Ok(rates) = rx.recv_timeout(Duration::from_secs(3)) {
                samples += 1;
                let total = rates
                    .values()
                    .fold((0u64, 0u64), |(r, t), n| (r + n.rx_bps, t + n.tx_bps));
                if total.0 + total.1 > peak.0 + peak.1 {
                    peak = total;
                }
                for (&pid, &rate) in &rates {
                    if top.is_none_or(|(_, t)| rate.rx_bps + rate.tx_bps > t.rx_bps + t.tx_bps) {
                        top = Some((pid, rate));
                    }
                }
                println!(
                    "sample {samples}: {} active pids, total down={} B/s up={} B/s",
                    rates.len(),
                    total.0,
                    total.1
                );
            }
        }

        if let Some((pid, rate)) = top {
            println!(
                "top talker: pid {pid} down={} B/s up={} B/s",
                rate.rx_bps, rate.tx_bps
            );
        }
        println!("peak total: down={} B/s up={} B/s", peak.0, peak.1);
        assert!(samples >= 1, "expected at least one sample from the poller");
    }
}

/// Per-process throughput in bytes per second.
#[derive(Clone, Copy, Default)]
pub struct NetRate {
    pub rx_bps: u64,
    pub tx_bps: u64,
}

/// Map of pid -> current throughput.
pub type NetRates = HashMap<u32, NetRate>;

/// Spawn a background thread that samples network counters on an interval and sends
/// derived per-process rates over a channel.
pub fn spawn_poller(interval: Duration) -> Receiver<NetRates> {
    let (tx, rx) = mpsc::channel();
    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(move || macos::run(&tx, interval));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (tx, interval);
    }
    rx
}

#[cfg(target_os = "macos")]
mod macos {
    use std::collections::HashMap;
    use std::process::Command;
    use std::sync::mpsc::Sender;
    use std::time::{Duration, Instant};

    use super::{NetRate, NetRates};

    /// Cumulative (bytes_in, bytes_out) byte counters per pid, as last read from `nettop`.
    type Counters = HashMap<u32, (u64, u64)>;

    pub fn run(tx: &Sender<NetRates>, interval: Duration) {
        let mut prev: Option<(Instant, Counters)> = None;
        loop {
            if let Some(current) = sample() {
                let now = Instant::now();
                if let Some((t0, previous)) = prev.take() {
                    let dt = now.saturating_duration_since(t0).as_secs_f64().max(0.001);
                    let mut rates: NetRates = HashMap::new();
                    for (&pid, &(cin, cout)) in &current {
                        // New pids start at their own value so the first reading is a zero
                        // delta rather than counting the connection's whole lifetime.
                        let (pin, pout) = previous.get(&pid).copied().unwrap_or((cin, cout));
                        let din = cin.saturating_sub(pin);
                        let dout = cout.saturating_sub(pout);
                        if din > 0 || dout > 0 {
                            rates.insert(
                                pid,
                                NetRate {
                                    rx_bps: (din as f64 / dt) as u64,
                                    tx_bps: (dout as f64 / dt) as u64,
                                },
                            );
                        }
                    }
                    if tx.send(rates).is_err() {
                        return; // receiver dropped: app exited
                    }
                }
                prev = Some((now, current));
            }
            std::thread::sleep(interval);
        }
    }

    /// Run `nettop` once and return cumulative (bytes_in, bytes_out) per pid.
    fn sample() -> Option<Counters> {
        let output = Command::new("nettop")
            .args(["-P", "-x", "-l", "1", "-J", "bytes_in,bytes_out"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut map: Counters = HashMap::new();
        for line in text.lines() {
            if let Some((pid, cin, cout)) = parse_line(line) {
                let entry = map.entry(pid).or_insert((0, 0));
                entry.0 = entry.0.saturating_add(cin);
                entry.1 = entry.1.saturating_add(cout);
            }
        }
        Some(map)
    }

    /// Parse one nettop row: `<name>.<pid> ... <bytes_in> <bytes_out>`.
    ///
    /// The two byte counters are the last whitespace-separated tokens, which keeps
    /// process names containing spaces (e.g. "Code Helper.778") intact.
    fn parse_line(line: &str) -> Option<(u32, u64, u64)> {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 3 {
            return None;
        }
        let bytes_out = tokens[tokens.len() - 1].parse::<u64>().ok()?;
        let bytes_in = tokens[tokens.len() - 2].parse::<u64>().ok()?;
        let label = tokens[tokens.len() - 3];
        let pid = label.rsplit('.').next()?.parse::<u32>().ok()?;
        Some((pid, bytes_in, bytes_out))
    }

    #[cfg(test)]
    mod tests {
        use super::parse_line;

        #[test]
        fn parses_simple_row() {
            assert_eq!(
                parse_line("apsd.355                 128957          170344"),
                Some((355, 128957, 170344))
            );
        }

        #[test]
        fn parses_name_with_spaces() {
            assert_eq!(
                parse_line("Code Helper.778           63373            6042"),
                Some((778, 63373, 6042))
            );
        }

        #[test]
        fn parses_multi_space_name() {
            assert_eq!(
                parse_line("Google Chrome H.5370          0               0"),
                Some((5370, 0, 0))
            );
        }

        #[test]
        fn rejects_header_and_blank() {
            assert_eq!(parse_line("            bytes_in       bytes_out"), None);
            assert_eq!(parse_line(""), None);
            assert_eq!(parse_line("   "), None);
        }
    }
}
