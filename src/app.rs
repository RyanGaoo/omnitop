use std::collections::{HashMap, HashSet, VecDeque};

use ratatui::style::Color;
use ratatui::widgets::TableState;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::config::Config;
use crate::docker::{Container, DockerState};
use crate::gpu::{self, GpuStats};
use crate::net::NetRates;

pub const HISTORY_LEN: usize = 120;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Memory,
    Pid,
    Name,
}

impl SortKey {
    /// The natural default direction when first selecting a column: usage metrics
    /// start descending (biggest first), identifiers start ascending.
    fn default_desc(self) -> bool {
        matches!(self, SortKey::Cpu | SortKey::Memory)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
    ConfirmKill,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Processes,
    Containers,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KillSignal {
    Term,
    Kill,
}

impl KillSignal {
    pub fn label(self) -> &'static str {
        match self {
            KillSignal::Term => "SIGTERM",
            KillSignal::Kill => "SIGKILL",
        }
    }
}

pub struct ProcRow {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub cpu: f32,
    pub mem_bytes: u64,
}

pub struct App {
    sys: System,
    pub processes: Vec<ProcRow>,
    pub visible: Vec<usize>,
    pub table_state: TableState,
    pub sort_key: SortKey,
    pub sort_desc: bool,
    pub input_mode: InputMode,
    pub filter: String,
    pub cpu_usage: f32,
    pub per_core: Vec<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
    pub cpu_history: VecDeque<u64>,
    pub mem_history: VecDeque<u64>,
    pub paused: bool,
    pub status: Option<String>,
    pub pending_signal: KillSignal,
    pub tree: bool,
    pub depths: Vec<u16>,
    pub accent: Color,
    pub view: View,
    pub containers: Vec<Container>,
    pub container_state: TableState,
    pub docker_message: Option<String>,
    pub gpus: Vec<GpuStats>,
    pub gpu_history: VecDeque<u64>,
    pub net_rates: NetRates,
}

impl App {
    pub fn new(config: &Config) -> Self {
        let mut app = App {
            sys: System::new_all(),
            processes: Vec::new(),
            visible: Vec::new(),
            table_state: TableState::default().with_selected(0),
            sort_key: SortKey::Cpu,
            sort_desc: true,
            input_mode: InputMode::Normal,
            filter: String::new(),
            cpu_usage: 0.0,
            per_core: Vec::new(),
            mem_used: 0,
            mem_total: 0,
            cpu_history: VecDeque::with_capacity(HISTORY_LEN),
            mem_history: VecDeque::with_capacity(HISTORY_LEN),
            paused: false,
            status: None,
            pending_signal: KillSignal::Term,
            tree: false,
            depths: Vec::new(),
            accent: config.accent,
            view: View::Processes,
            containers: Vec::new(),
            container_state: TableState::default(),
            docker_message: Some("Connecting to container runtime…".to_string()),
            gpus: Vec::new(),
            gpu_history: VecDeque::with_capacity(HISTORY_LEN),
            net_rates: NetRates::new(),
        };
        app.refresh();
        app
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        self.sys.refresh_processes(ProcessesToUpdate::All, true);

        self.cpu_usage = self.sys.global_cpu_usage();
        self.per_core = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        self.mem_used = self.sys.used_memory();
        self.mem_total = self.sys.total_memory();

        push_history(&mut self.cpu_history, self.cpu_usage as u64);
        let mem_pct = if self.mem_total > 0 {
            (self.mem_used * 100 / self.mem_total) as u64
        } else {
            0
        };
        push_history(&mut self.mem_history, mem_pct);

        self.gpus = gpu::sample();
        let gpu_util = self
            .gpus
            .first()
            .and_then(|g| g.utilization)
            .unwrap_or(0.0);
        push_history(&mut self.gpu_history, gpu_util as u64);

        self.processes = self
            .sys
            .processes()
            .values()
            .map(|p| ProcRow {
                pid: p.pid().as_u32(),
                parent: p.parent().map(|pp| pp.as_u32()),
                name: p.name().to_string_lossy().into_owned(),
                cpu: p.cpu_usage(),
                mem_bytes: p.memory(),
            })
            .collect();

        self.sort();
        self.apply_filter();
    }

    pub fn sort_by(&mut self, key: SortKey) {
        if self.sort_key == key {
            // Pressing the active sort key again flips the direction.
            self.sort_desc = !self.sort_desc;
        } else {
            self.sort_key = key;
            self.sort_desc = key.default_desc();
        }
        self.sort();
        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        let needle = self.filter.to_lowercase();
        if self.tree && needle.is_empty() {
            self.build_tree_order();
        } else {
            self.depths.clear();
            self.visible = self
                .processes
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    needle.is_empty()
                        || p.name.to_lowercase().contains(&needle)
                        || p.pid.to_string().contains(&needle)
                })
                .map(|(i, _)| i)
                .collect();
        }
        self.clamp_selection();
    }

    pub fn toggle_tree(&mut self) {
        self.tree = !self.tree;
        self.status = if self.tree && !self.filter.is_empty() {
            Some("Tree view active (shown when filter is cleared)".to_string())
        } else {
            None
        };
        self.apply_filter();
    }

    fn build_tree_order(&mut self) {
        let pid_set: HashSet<u32> = self.processes.iter().map(|p| p.pid).collect();
        // On Unix the OS reparents orphaned processes to init (PID 1). We mirror that:
        // a process whose parent is unknown (e.g. a root-owned daemon we can't introspect
        // without privileges) is attached to PID 1 rather than treated as a top-level root.
        let init_pid = pid_set.contains(&1).then_some(1u32);
        let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
        let mut roots: Vec<usize> = Vec::new();

        for (i, p) in self.processes.iter().enumerate() {
            let resolved = p
                .parent
                .filter(|pp| *pp != p.pid && pid_set.contains(pp))
                .or_else(|| init_pid.filter(|&init| init != p.pid));
            match resolved {
                Some(pp) => children.entry(pp).or_default().push(i),
                None => roots.push(i),
            }
        }

        self.visible.clear();
        self.depths.clear();
        let mut seen: HashSet<usize> = HashSet::new();
        let mut stack: Vec<(usize, u16)> = roots.into_iter().rev().map(|i| (i, 0)).collect();
        while let Some((idx, depth)) = stack.pop() {
            if !seen.insert(idx) {
                continue;
            }
            self.visible.push(idx);
            self.depths.push(depth);
            if let Some(kids) = children.get(&self.processes[idx].pid) {
                for &k in kids.iter().rev() {
                    stack.push((k, depth.saturating_add(1)));
                }
            }
        }
    }

    pub fn selected_proc(&self) -> Option<&ProcRow> {
        self.table_state
            .selected()
            .and_then(|i| self.visible.get(i))
            .and_then(|&idx| self.processes.get(idx))
    }

    pub fn kill_selected(&mut self) {
        let signal = self.pending_signal;
        let target = self.selected_proc().map(|p| (p.pid, p.name.clone()));
        if let Some((pid, name)) = target {
            self.status = Some(match send_signal(pid, signal) {
                Ok(()) => format!("Sent {} to {name} ({pid})", signal.label()),
                Err(e) => format!("Could not kill {name} ({pid}): {e}"),
            });
        }
    }

    pub fn toggle_view(&mut self) {
        self.view = match self.view {
            View::Processes => View::Containers,
            View::Containers => View::Processes,
        };
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
    }

    pub fn set_docker(&mut self, state: DockerState) {
        match state {
            DockerState::Containers(containers) => {
                self.containers = containers;
                self.docker_message = None;
                if self.containers.is_empty() {
                    self.container_state.select(None);
                } else {
                    let sel = self.container_state.selected().unwrap_or(0);
                    self.container_state
                        .select(Some(sel.min(self.containers.len() - 1)));
                }
            }
            DockerState::Disabled => {
                self.containers.clear();
                self.container_state.select(None);
                self.docker_message =
                    Some("No Docker or Podman socket found — is the daemon running?".to_string());
            }
            DockerState::Error(e) => {
                self.containers.clear();
                self.container_state.select(None);
                self.docker_message = Some(format!("Container runtime error: {e}"));
            }
        }
    }

    pub fn set_net_rates(&mut self, rates: NetRates) {
        self.net_rates = rates;
    }

    /// Aggregate (rx, tx) throughput across all processes, in bytes per second.
    pub fn net_totals(&self) -> (u64, u64) {
        self.net_rates.values().fold((0, 0), |(rx, tx), rate| {
            (rx + rate.rx_bps, tx + rate.tx_bps)
        })
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
        self.status = if self.paused {
            Some("Paused — press space to resume".to_string())
        } else {
            None
        };
    }

    fn sort(&mut self) {
        use std::cmp::Ordering::Equal;
        let desc = self.sort_desc;
        // Each arm computes the ascending ordering; flip it when descending.
        let dir = |ord: std::cmp::Ordering| if desc { ord.reverse() } else { ord };
        match self.sort_key {
            SortKey::Cpu => self
                .processes
                .sort_by(|a, b| dir(a.cpu.partial_cmp(&b.cpu).unwrap_or(Equal))),
            SortKey::Memory => self.processes.sort_by(|a, b| dir(a.mem_bytes.cmp(&b.mem_bytes))),
            SortKey::Pid => self.processes.sort_by(|a, b| dir(a.pid.cmp(&b.pid))),
            SortKey::Name => self
                .processes
                .sort_by(|a, b| dir(a.name.to_lowercase().cmp(&b.name.to_lowercase()))),
        }
    }

    fn clamp_selection(&mut self) {
        if self.visible.is_empty() {
            self.table_state.select(None);
        } else {
            let selected = self.table_state.selected().unwrap_or(0);
            self.table_state
                .select(Some(selected.min(self.visible.len() - 1)));
        }
    }

    pub fn next(&mut self) {
        match self.view {
            View::Processes => move_selection(&mut self.table_state, self.visible.len(), 1),
            View::Containers => move_selection(&mut self.container_state, self.containers.len(), 1),
        }
    }

    pub fn previous(&mut self) {
        match self.view {
            View::Processes => move_selection(&mut self.table_state, self.visible.len(), -1),
            View::Containers => {
                move_selection(&mut self.container_state, self.containers.len(), -1)
            }
        }
    }
}

fn move_selection(state: &mut TableState, len: usize, delta: i32) {
    if len == 0 {
        state.select(None);
        return;
    }
    let current = state.selected().unwrap_or(0) as i32;
    let next = (current + delta).clamp(0, len as i32 - 1);
    state.select(Some(next as usize));
}

fn push_history(history: &mut VecDeque<u64>, value: u64) {
    if history.len() >= HISTORY_LEN {
        history.pop_front();
    }
    history.push_back(value);
}

#[cfg(unix)]
fn send_signal(pid: u32, signal: KillSignal) -> Result<(), String> {
    let sig = match signal {
        KillSignal::Term => libc::SIGTERM,
        KillSignal::Kill => libc::SIGKILL,
    };
    let ret = unsafe { libc::kill(pid as libc::pid_t, sig) };
    if ret == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    Err(match err.raw_os_error() {
        Some(libc::ESRCH) => "process already exited".to_string(),
        Some(libc::EPERM) => "permission denied (owned by another user?)".to_string(),
        _ => err.to_string(),
    })
}

#[cfg(not(unix))]
fn send_signal(_pid: u32, _signal: KillSignal) -> Result<(), String> {
    Err("kill not supported on this platform yet".to_string())
}
