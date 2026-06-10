# Launch notes

Draft copy for launching omnitop. Edit freely before posting.

## Publishing to crates.io

```sh
# One-time: get a token from https://crates.io/settings/tokens
cargo login <YOUR_TOKEN>

# From the repo root (working tree must be committed):
cargo publish
```

The package is already verified with `cargo publish --dry-run` (13 files, ~26 KiB
compressed; the demo GIF is excluded via `exclude` in Cargo.toml).

After publishing, `cargo install omnitop` will work. Add the badge to the README:

```md
[![crates.io](https://img.shields.io/crates/v/omnitop.svg)](https://crates.io/crates/omnitop)
```

---

## Show HN

**Title:**
Show HN: omnitop – processes, containers, GPU and per-process network in one TUI

**Body:**
I kept juggling four terminals to watch one machine: htop for processes, `docker
stats` for containers, a GPU monitor, and something like nethogs for per-process
network. omnitop puts all of it in a single terminal UI.

It's written in Rust (ratatui). A few things I'm happy with:

- The Docker/Podman view talks to the runtime socket directly — a small hand-rolled
  HTTP/1.1-over-Unix-socket client (chunked decoding + serde_json), no Docker CLI or
  async runtime needed.
- On Apple Silicon, GPU utilization and memory come straight from the IORegistry via
  hand-written IOKit/CoreFoundation FFI — the same source Activity Monitor uses, and
  it works **without sudo** (unlike powermetrics-based tools).
- Per-process network on macOS is derived from `nettop`, also without root, by diffing
  cumulative byte counters on a background thread.
- Containers and network polling run off the UI thread over channels, so the UI never
  blocks on a slow stats call.

Status: it's early (v0.5). macOS is the most complete target today — GPU and
per-process network are macOS-only so far; Linux equivalents (NVML, eBPF) are on the
roadmap. Processes and containers work on both.

Repo: https://github.com/RyanGaoo/omnitop

Would love feedback on the architecture and what you'd want from a unified monitor.

---

## r/rust

**Title:**
omnitop: a unified system monitor TUI (processes + containers + GPU + network), with
hand-written IOKit FFI and a from-scratch Docker socket client

**Body:**
I've been building omnitop, a terminal system monitor in Rust (ratatui + crossterm)
that unifies what htop / docker stats / a GPU monitor / nethogs each do separately.

Rust bits that might interest this sub:

- **No async runtime.** Container and network sampling run on plain `std::thread`s and
  push updates to the UI over `mpsc` channels.
- **Hand-rolled Docker/Podman client:** HTTP/1.1 over a `UnixStream`, including chunked
  transfer-encoding decoding, then `serde_json` for the container list + stats. No
  bollard/hyper.
- **Hand-written IOKit + CoreFoundation FFI** for Apple Silicon GPU stats (reading the
  IORegistry `PerformanceStatistics` dict). Manual `CFRelease` discipline; no `sudo`.
- **macOS `nettop` parser** that handles process names containing spaces by treating
  the last two whitespace tokens as the byte counters.
- Pure logic (chunked decode, CPU% math, nettop parsing) is unit-tested; the live
  paths have `#[ignore]`d integration tests that run against real Docker/GPU/network.

Repo (MIT): https://github.com/RyanGaoo/omnitop

Critiques on the FFI safety and the poller design especially welcome.

---

## r/selfhosted

**Title:**
omnitop – one TUI to watch processes, Docker/Podman containers, GPU and per-process
network on your box

**Body:**
If you run a home server or a few containers, you've probably bounced between htop and
`docker stats`. omnitop is a single terminal dashboard that shows host processes
(CPU/mem, tree, kill) and a live Docker/Podman container view (per-container CPU and
memory vs. limits) side by side, plus GPU and per-process network where supported.

It talks to the container socket directly (Docker or Podman, including Colima), needs
no agent, and runs without root. It's free and open source (Rust, MIT).

Heads-up: it's early and macOS is the most complete target right now (GPU + per-process
network are macOS-only so far); processes and containers work on Linux too, with the
rest on the roadmap.

Repo: https://github.com/RyanGaoo/omnitop

Happy to take feature requests.
