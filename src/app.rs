use ratatui::widgets::TableState;
use sysinfo::System;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Cpu,
    Memory,
    Pid,
    Name,
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
    pub table_state: TableState,
    pub sort_key: SortKey,
    pub cpu_usage: f32,
    pub per_core: Vec<f32>,
    pub mem_used: u64,
    pub mem_total: u64,
}

impl App {
    pub fn new() -> Self {
        let mut app = App {
            sys: System::new_all(),
            processes: Vec::new(),
            table_state: TableState::default().with_selected(0),
            sort_key: SortKey::Cpu,
            cpu_usage: 0.0,
            per_core: Vec::new(),
            mem_used: 0,
            mem_total: 0,
        };
        app.refresh();
        app
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_all();

        self.cpu_usage = self.sys.global_cpu_usage();
        self.per_core = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        self.mem_used = self.sys.used_memory();
        self.mem_total = self.sys.total_memory();

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
        self.clamp_selection();
    }

    pub fn sort_by(&mut self, key: SortKey) {
        self.sort_key = key;
        self.sort();
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
        if self.processes.is_empty() {
            self.table_state.select(None);
        } else {
            let selected = self.table_state.selected().unwrap_or(0);
            self.table_state
                .select(Some(selected.min(self.processes.len() - 1)));
        }
    }

    pub fn next(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => (i + 1).min(self.processes.len() - 1),
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        let i = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.table_state.select(Some(i));
    }
}
