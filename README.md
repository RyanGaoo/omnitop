# omnitop

A unified system monitor for the modern stack: **processes, containers, GPU, and per-process network — one pane of glass, one TUI.**

Existing monitors each show a slice: `htop`/`btop` show processes, `docker stats` shows containers, `nvidia-smi`/`asitop` show GPU, `nethogs` shows network. omnitop's goal is all of it, in one fast, beautiful terminal UI.

## Status

**v0.5 — the full stack in one TUI** (working): processes, Docker/Podman containers, GPU, **and per-process network throughput** — the original goal. On macOS, per-process RX/TX comes from the built-in `nettop` (**no `sudo`**); the process table gains `RX/s`/`TX/s` columns and a live network total.

Process monitoring: live table, search/filter, kill (SIGTERM/SIGKILL), tree view, CPU/memory gauges with history sparklines, per-core bars, sorting, pause, config file. **Containers tab** (`Tab`/`2`) and a **GPU panel** read from the IORegistry.

## Install / Run

```sh
cargo run --release
```

## Keybindings

| Key                | Action                                               |
| ------------------ | ---------------------------------------------------- |
| `q` / `Esc`        | Quit                                                 |
| `Tab`              | Switch between Processes and Containers views        |
| `1` / `2`          | Jump to Processes / Containers view                  |
| `↑`/`↓` or `j`/`k` | Navigate the current list                            |
| `/`                | Filter by name or PID (Enter to apply, Esc to clear) |
| `x`                | Kill selected process with SIGTERM (confirmation)    |
| `X`                | Force-kill with SIGKILL (confirmation)               |
| `t`                | Toggle process tree view                             |
| `space`            | Pause/resume refresh                                 |
| `c`                | Sort by CPU                                          |
| `m`                | Sort by memory                                       |
| `p`                | Sort by PID                                          |
| `n`                | Sort by name                                         |

## Roadmap

- **v0.1 — Processes** ✅
  - Live process table (PID, name, CPU%, memory)
  - Global CPU + memory gauges
  - Sorting and keyboard navigation
- **v0.2 — Interactivity** ✅
  - Process search/filter (`/`)
  - Kill processes with confirmation (`x`)
  - Per-core CPU bars, CPU/memory history sparklines
  - Pause/resume (`space`)
- **v0.25 — Polish** ✅
  - Process tree view (`t`)
  - Config file (refresh rate, accent color)
- **v0.3 — Containers** ✅
  - Docker/Podman socket integration (minimal HTTP-over-Unix-socket client, no async deps)
  - Background poller thread keeps the UI responsive
  - Per-container CPU% and memory vs. limit, tabbed view
  - _Follow-up (v0.3.x):_ map host processes to containers (Linux cgroups; not possible on macOS where Docker runs in a VM)
  - _Follow-up:_ stop/restart containers from the UI
- **v0.4 — GPU** ✅
  - Apple Silicon via IOKit/IORegistry (hand-written FFI, no `sudo`)
  - GPU utilization gauge + memory + history sparkline in the header
  - _Follow-up:_ NVIDIA via NVML; per-process GPU attribution
- **v0.5 — Network** ✅
  - Per-process network throughput on macOS via `nettop` (no `sudo`), background poller diffing cumulative counters into live rates
  - `RX/s`/`TX/s` columns + aggregate network total in the process view
  - _Follow-up:_ Linux per-process network (eBPF / nethogs-style capture); sort by throughput
- **v1.0 — Release**
  - Homebrew tap, prebuilt binaries, AUR package
  - Benchmarks: omnitop's own overhead vs. htop/btop

## Configuration

Optional config at `~/.config/omnitop/config.toml` (or `$XDG_CONFIG_HOME/omnitop/config.toml`):

```toml
# Refresh interval in milliseconds (minimum 100)
refresh_ms = 1000

# Accent color: a named color (cyan, green, magenta, ...) or hex like "#7aa2f7"
accent = "cyan"
```

## Architecture

```
src/
  main.rs    — event loop (input + config-driven refresh tick + docker channel drain)
  app.rs     — application state, sampling via sysinfo, sorting, tree ordering, views
  ui.rs      — ratatui rendering (tabs, header gauges, process/container tables, footer)
  config.rs  — TOML config loading (refresh rate, accent color)
  docker.rs  — minimal Docker/Podman client over a Unix socket + background poller
  gpu.rs     — GPU sampling (macOS: hand-written IOKit/CoreFoundation FFI)
  net.rs     — per-process network rates (macOS: nettop poller + cumulative-counter diffing)
```

Per-process network on macOS samples `nettop` on a background thread and diffs consecutive cumulative byte counters into per-second rates, keyed by pid (the parser keeps process names containing spaces intact). Like the container poller, it runs off the UI thread and pushes updates over an `mpsc` channel. Parsing is unit tested; the full pipeline has an ignored live test (`cargo test -- --ignored --nocapture live_net`).

GPU stats on macOS come from the IORegistry's accelerator `PerformanceStatistics` (`Device Utilization %`, `In use system memory`) via hand-written IOKit + CoreFoundation FFI — the same source Activity Monitor uses, readable without elevated privileges. Verified live on Apple Silicon; covered by an ignored hardware test (`cargo test -- --ignored --nocapture live_gpu`).

The Docker client is hand-rolled: it speaks HTTP/1.1 over the runtime's Unix socket (`/var/run/docker.sock` and common Podman/Colima paths), handles chunked transfer encoding, and parses the container list + per-container stats with `serde_json`. Polling runs on a background thread that pushes updates to the UI over an `mpsc` channel, so slow stats calls never block rendering. Pure parsing/CPU-math logic is unit tested (`cargo test`).

The `sysinfo` crate is the initial sampling backend; the plan is to replace hot paths with direct `/proc` (Linux) and `libproc` (macOS) readers as profiling demands.

## License

MIT
