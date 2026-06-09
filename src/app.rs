use std::collections::VecDeque;

use ratatui::widgets::TableState;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, ProcessesToUpdate, RefreshKind, System};

pub const HISTORY_LEN: usize = 120;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Memory,
    Pid,
    Name,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
    ConfirmKill,
}

pub struct ProcRow {
    pub pid: u32,
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
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            sys: System::new_all(),
            processes: Vec::new(),
            visible: Vec::new(),
            table_state: TableState::default().with_selected(0),
            sort_key: SortKey::Cpu,
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

        self.processes = self
            .sys
            .processes()
            .values()
            .map(|p| ProcRow {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().into_owned(),
                cpu: p.cpu_usage(),
                mem_bytes: p.memory(),
            })
            .collect();

        self.sort();
        self.apply_filter();
    }

    pub fn sort_by(&mut self, key: SortKey) {
        self.sort_key = key;
        self.sort();
        self.apply_filter();
    }

    pub fn apply_filter(&mut self) {
        let needle = self.filter.to_lowercase();
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
        self.clamp_selection();
    }

    pub fn selected_proc(&self) -> Option<&ProcRow> {
        self.table_state
            .selected()
            .and_then(|i| self.visible.get(i))
            .and_then(|&idx| self.processes.get(idx))
    }

    pub fn kill_selected(&mut self) {
        let target = self.selected_proc().map(|p| (p.pid, p.name.clone()));
        if let Some((pid, name)) = target {
            self.status = Some(match send_sigterm(pid) {
                Ok(()) => format!("Sent SIGTERM to {name} ({pid})"),
                Err(e) => format!("Could not kill {name} ({pid}): {e}"),
            });
        }
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
        match self.sort_key {
            SortKey::Cpu => self
                .processes
                .sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal)),
            SortKey::Memory => self.processes.sort_by(|a, b| b.mem_bytes.cmp(&a.mem_bytes)),
            SortKey::Pid => self.processes.sort_by(|a, b| a.pid.cmp(&b.pid)),
            SortKey::Name => self
                .processes
                .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
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
        if self.visible.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => (i + 1).min(self.visible.len() - 1),
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.table_state.select(Some(i));
    }
}

fn push_history(history: &mut VecDeque<u64>, value: u64) {
    if history.len() >= HISTORY_LEN {
        history.pop_front();
    }
    history.push_back(value);
}

#[cfg(unix)]
fn send_sigterm(pid: u32) -> Result<(), String> {
    let ret = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
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
fn send_sigterm(pid: u32) -> Result<(), String> {
    Err("kill not supported on this platform yet".to_string())
}
