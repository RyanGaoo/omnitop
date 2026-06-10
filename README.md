# omnitop

A unified system monitor for the modern stack: **processes, containers, GPU, and per-process network — one pane of glass, one TUI.**

![omnitop demo](https://raw.githubusercontent.com/RyanGaoo/omnitop/main/docs/demo.gif)

Existing monitors each show a slice: `htop`/`btop` show processes, `docker stats` shows containers, `nvidia-smi`/`asitop` show GPU, `nethogs` shows network. omnitop's goal is all of it, in one fast, beautiful terminal UI.

## Status

**v0.5 — the full stack in one TUI** (working): processes, Docker/Podman containers, GPU, **and per-process network throughput** — the original goal. On macOS, per-process RX/TX comes from the built-in `nettop` (**no `sudo`**); the process table gains `RX/s`/`TX/s` columns and a live network total.

Process monitoring: live table, search/filter, kill (SIGTERM/SIGKILL), tree view, CPU/memory gauges with history sparklines, per-core bars, sorting, pause, config file. **Containers tab** (`Tab`/`2`) and a **GPU panel** read from the IORegistry.

## How it compares

Other tools each cover one slice; omnitop unifies them in a single TUI.

|                            | **omnitop** | htop | btop | ctop | nvtop | nethogs |
| -------------------------- | :---------: | :--: | :--: | :--: | :---: | :-----: |
| Processes (CPU/mem)        |     ✅      |  ✅  |  ✅  |  —   |   —   |    —    |
| Process tree               |     ✅      |  ✅  |  ✅  |  —   |   —   |    —    |
| Kill / signal              |     ✅      |  ✅  |  ✅  |  —   |   —   |    —    |
| Containers (Docker/Podman) |     ✅      |  —   |  —   |  ✅  |   —   |    —    |
| GPU utilization            |    ✅ ¹     |  —   | ✅ ² |  —   | ✅ ²  |    —    |
| Per-process network        |    ✅ ¹     |  —   |  —   |  —   |   —   |  ✅ ²   |
| Everything in one TUI      |     ✅      |  —   |  —   |  —   |   —   |    —    |
| Runs without root          |     ✅      |  ✅  |  ✅  |  ✅  |  ✅   |   ❌    |

¹ omnitop GPU: Apple Silicon (macOS) and NVIDIA (Linux/Windows, via NVML). Per-process network is macOS today; Linux is on the roadmap.
² btop/nvtop GPU is Linux (NVIDIA/AMD/Intel); nethogs network is Linux and requires root.

## Install / Run

omnitop is built from source with Cargo. First install the Rust toolchain from
[rustup.rs](https://rustup.rs), then:

```sh
git clone https://github.com/RyanGaoo/omnitop
cd omnitop
cargo run --release
```

The binary lands at `target/release/omnitop`. (Once published, `cargo install omnitop`
will also work.)

### Windows

1. Install Rust from [rustup.rs](https://rustup.rs). When prompted, also install the
   **Visual Studio C++ Build Tools** (rustup links to the installer) — this provides the
   MSVC linker Cargo needs.
2. In **PowerShell** or **Windows Terminal** (use Windows Terminal for proper Unicode and
   colors — the legacy console renders the gauges poorly):

   ```powershell
   git clone https://github.com/RyanGaoo/omnitop
   cd omnitop
   cargo run --release
   ```

On Windows you get the full process view including kill (via `TerminateProcess`) and,
with an NVIDIA GPU, the GPU panel. Container monitoring needs a Unix socket and isn't
wired up on Windows yet — but it works today if you run omnitop **inside WSL2** (where
Docker exposes a Unix socket).

### Platform support

| Feature                                 |      macOS       |   Linux   |  Windows  |
| --------------------------------------- | :--------------: | :-------: | :-------: |
| Processes — view / tree / sort / filter |        ✅        |    ✅     |    ✅     |
| Kill / signal processes                 |        ✅        |    ✅     |   ✅ ¹    |
| Containers (Docker/Podman)              |        ✅        |    ✅     |    — ²    |
| GPU                                     | ✅ Apple Silicon | ✅ NVIDIA | ✅ NVIDIA |
| Per-process network                     |        ✅        |    — ³    |    — ³    |

¹ Windows has no POSIX signals, so both `x` and `X` do a hard `TerminateProcess` (like `SIGKILL`).
² Windows Docker is reached over a named pipe (planned); containers work today under WSL2.
³ Per-process network is macOS-only today (via `nettop`); Linux/Windows are on the roadmap.

## Keybindings

| Key                 | Action                                                               |
| ------------------- | -------------------------------------------------------------------- |
| `q` / `Esc`         | Quit                                                                 |
| `Tab`               | Switch between Processes and Containers views                        |
| `1` / `2`           | Jump to Processes / Containers view                                  |
| `↑`/`↓` or `j`/`k`  | Navigate the current list                                            |
| `/`                 | Filter by name or PID (Enter to apply, Esc to clear)                 |
| `x`                 | Kill selected process with SIGTERM (confirmation)                    |
| `X`                 | Force-kill with SIGKILL (confirmation)                               |
| `t`                 | Toggle process tree view                                             |
| `space`             | Pause/resume refresh                                                 |
| `c`/`m`/`p`/`n`/`N` | Sort by CPU / memory / PID / name / network (press again to reverse) |
| `s` / `r`           | Stop / restart selected container (Containers view)                  |

## Roadmap

v0.1–v0.5 (processes, containers, GPU, network) are shipped. See
[`docs/roadmap.md`](docs/roadmap.md) for the full milestone breakdown and what's planned
next (Windows containers via named pipe, per-process GPU attribution, Linux per-process
network, and v1.0 packaging).

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
  gpu.rs     — GPU sampling (macOS: hand-written IOKit FFI; Linux/Windows: NVML)
  net.rs     — per-process network rates (macOS: nettop poller + cumulative-counter diffing)
```

Per-process network on macOS samples `nettop` on a background thread and diffs consecutive cumulative byte counters into per-second rates, keyed by pid (the parser keeps process names containing spaces intact). Like the container poller, it runs off the UI thread and pushes updates over an `mpsc` channel. Parsing is unit tested; the full pipeline has an ignored live test (`cargo test -- --ignored --nocapture live_net`).

GPU stats on macOS come from the IORegistry's accelerator `PerformanceStatistics` (`Device Utilization %`, `In use system memory`) via hand-written IOKit + CoreFoundation FFI — the same source Activity Monitor uses, readable without elevated privileges. Verified live on Apple Silicon; covered by an ignored hardware test (`cargo test -- --ignored --nocapture live_gpu`).

The Docker client is hand-rolled: it speaks HTTP/1.1 over the runtime's Unix socket (`/var/run/docker.sock` and common Podman/Colima paths), handles chunked transfer encoding, and parses the container list + per-container stats with `serde_json`. Polling runs on a background thread that pushes updates to the UI over an `mpsc` channel, so slow stats calls never block rendering. Pure parsing/CPU-math logic is unit tested (`cargo test`).

The `sysinfo` crate is the initial sampling backend; the plan is to replace hot paths with direct `/proc` (Linux) and `libproc` (macOS) readers as profiling demands.

## License

MIT
