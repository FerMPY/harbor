//! harbor — see what's docked at every local port, grouped by project.
//! An interactive TUI over `lsof`/`ps`. Arrow keys to move, x to kill.

mod app;
mod collect;
mod ui;

use std::io;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;

use app::{App, Mode};

const TICK: Duration = Duration::from_millis(250);
const REFRESH: Duration = Duration::from_secs(2);

fn main() -> io::Result<()> {
    // Non-interactive fallback: `harbor --list` / `-l` prints once and exits.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-l" || a == "--list") {
        print_once();
        return Ok(());
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut app = App::new();
    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        if event::poll(TICK)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.mode {
                    Mode::Normal => {
                        if handle_normal(&mut app, key.code) {
                            return Ok(());
                        }
                    }
                    Mode::Filter => handle_filter(&mut app, key.code),
                    Mode::Confirm => handle_confirm(&mut app, key.code),
                }
            }
        }

        // Live auto-refresh, but never yank the list out from under a dialog or filter edit.
        if app.mode == Mode::Normal && app.last_refresh.elapsed() >= REFRESH {
            app.refresh();
        }
    }
}

/// Returns true when the app should quit.
fn handle_normal(app: &mut App, code: KeyCode) -> bool {
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return true,
        KeyCode::Down | KeyCode::Char('j') => app.next(),
        KeyCode::Up | KeyCode::Char('k') => app.prev(),
        KeyCode::Char('x') | KeyCode::Delete => {
            if app.selected().is_some() {
                app.status.clear();
                app.mode = Mode::Confirm;
            }
        }
        KeyCode::Char('o') => app.open_selected(),
        KeyCode::Char('a') => app.toggle_system(),
        KeyCode::Char('r') => {
            app.status = "refreshed".into();
            app.refresh();
        }
        KeyCode::Char('/') => {
            app.status.clear();
            app.mode = Mode::Filter;
        }
        _ => {}
    }
    false
}

fn handle_filter(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            app.filter.clear();
            app.rebuild();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => app.mode = Mode::Normal,
        KeyCode::Backspace => {
            app.filter.pop();
            app.rebuild();
        }
        KeyCode::Char(c) => {
            app.filter.push(c);
            app.rebuild();
        }
        _ => {}
    }
}

fn handle_confirm(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('y') => {
            app.kill_selected(false);
            app.mode = Mode::Normal;
        }
        KeyCode::Char('K') => {
            app.kill_selected(true);
            app.mode = Mode::Normal;
        }
        _ => app.mode = Mode::Normal, // n, esc, anything else cancels
    }
}

fn print_once() {
    for l in collect::collect() {
        let marker = if l.is_dev { "●" } else { "·" };
        let fw = l.framework.as_deref().map(|f| format!(" [{f}]")).unwrap_or_default();
        let proj = l
            .cwd
            .as_ref()
            .map(|p| collect::short_home(p))
            .unwrap_or_default();
        println!(
            "{marker} :{:<14} {:<14} pid {:<7} {:>6}  {}{}",
            l.ports_str(),
            l.command,
            l.pid,
            l.mem,
            proj,
            fw
        );
    }
}

fn print_help() {
    println!(
        "harbor — see what's docked at every local port\n\n\
         USAGE:\n  harbor          interactive TUI\n  harbor -l, --list   print once and exit\n  harbor -h, --help   this help\n\n\
         KEYS (in the TUI):\n  ↑/↓ or j/k   move      o   open in browser   x   kill selected\n  /            filter    a   toggle system processes\n  r            refresh   q   quit"
    );
}
