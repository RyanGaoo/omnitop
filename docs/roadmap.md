# omnitop roadmap

Status of each milestone. Shipped items are checked.

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
  - Stop / restart containers from the UI (`s` / `r`), run off the UI thread
  - _Follow-up (v0.3.x):_ map host processes to containers (Linux cgroups; not possible on macOS where Docker runs in a VM)
  - _Follow-up:_ Windows support via the Docker named pipe (`\\.\pipe\docker_engine`)
- **v0.4 — GPU** ✅
  - Apple Silicon via IOKit/IORegistry (hand-written FFI, no `sudo`)
  - NVIDIA via NVML on Linux/Windows (loaded at runtime; no-ops without a driver)
  - GPU utilization gauge + memory + history sparkline in the header
  - _Follow-up:_ per-process GPU attribution
- **v0.5 — Network** ✅
  - Per-process network throughput on macOS via `nettop` (no `sudo`), background poller diffing cumulative counters into live rates
  - `RX/s`/`TX/s` columns + aggregate network total + sort by throughput (`N`)
  - _Follow-up:_ Linux per-process network (eBPF / nethogs-style capture)
- **Cross-platform** ✅
  - Compiles and runs on macOS, Linux, and Windows
  - Windows process kill via `TerminateProcess` (no POSIX signals there)
  - _Follow-up:_ container monitoring on Windows via the Docker named pipe
- **v1.0 — Release**
  - Homebrew tap, prebuilt binaries, AUR package
  - Benchmarks: omnitop's own overhead vs. htop/btop
