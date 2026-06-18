//! Application state and the actions the event loop drives.

use std::time::Instant;

use ratatui::widgets::TableState;

use crate::collect::{self, Collector, Listener};

#[derive(PartialEq, Eq)]
pub enum Mode {
    Normal,
    Filter,
    Confirm,
}

pub struct App {
    collector: Collector,
    all: Vec<Listener>,      // raw snapshot
    pub rows: Vec<Listener>, // filtered + sorted view
    pub state: TableState,
    pub filter: String,
    pub mode: Mode,
    pub show_system: bool,
    pub status: String,
    pub last_refresh: Instant,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            collector: Collector::new(),
            all: Vec::new(),
            rows: Vec::new(),
            state: TableState::default(),
            filter: String::new(),
            mode: Mode::Normal,
            show_system: false,
            status: String::new(),
            last_refresh: Instant::now(),
        };
        app.refresh();
        app
    }

    /// Re-scan the system, preserving the selected pid where possible.
    pub fn refresh(&mut self) {
        let selected_pid = self.selected().map(|l| l.pid);
        self.all = self.collector.snapshot();
        self.rebuild();
        if let Some(pid) = selected_pid {
            if let Some(i) = self.rows.iter().position(|l| l.pid == pid) {
                self.state.select(Some(i));
            }
        }
        self.last_refresh = Instant::now();
    }

    /// Apply the current filter + dev/system toggle to produce `rows`.
    pub fn rebuild(&mut self) {
        let needle = self.filter.to_lowercase();
        self.rows = self
            .all
            .iter()
            .filter(|l| self.show_system || l.is_dev())
            .filter(|l| needle.is_empty() || l.haystack().contains(&needle))
            .cloned()
            .collect();
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        if self.rows.is_empty() {
            self.state.select(None);
        } else {
            let i = self.state.selected().unwrap_or(0).min(self.rows.len() - 1);
            self.state.select(Some(i));
        }
    }

    pub fn selected(&self) -> Option<&Listener> {
        self.state.selected().and_then(|i| self.rows.get(i))
    }

    pub fn next(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let i = self
            .state
            .selected()
            .map_or(0, |i| (i + 1) % self.rows.len());
        self.state.select(Some(i));
    }

    pub fn prev(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        let i = self
            .state
            .selected()
            .map_or(0, |i| (i + self.rows.len() - 1) % self.rows.len());
        self.state.select(Some(i));
    }

    pub fn toggle_system(&mut self) {
        self.show_system = !self.show_system;
        self.rebuild();
    }

    /// Open http://localhost:<port> for the selected process in the browser.
    pub fn open_selected(&mut self) {
        if let Some(l) = self.selected() {
            let port = l.min_port();
            self.status = if collect::open_url(port) {
                format!("opened http://localhost:{port}")
            } else {
                format!("could not open http://localhost:{port}")
            };
        }
    }

    /// Kill the selected process; `hard` => SIGKILL.
    pub fn kill_selected(&mut self, hard: bool) {
        if let Some(l) = self.selected() {
            let (pid, name, port) = (l.pid, l.command.clone(), l.ports_str());
            let ok = self.collector.kill(pid, hard);
            self.status = if ok {
                format!("killed {name} on :{port} (pid {pid})")
            } else {
                format!("could not kill pid {pid} (try K to force)")
            };
            self.refresh();
        }
    }
}
