//! harbor — see what's docked at every local port, grouped by project.
//! Interactive TUI by default, plus a CLI surface (kill / ps / watch / <port> / --json).
//! Cross-platform (macOS + Linux) via the listeners + sysinfo crates.

mod app;
mod collect;
mod ui;

use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::DefaultTerminal;

use app::{App, Mode};
use collect::{Collector, Health, Listener};

const TICK: Duration = Duration::from_millis(250);
const REFRESH: Duration = Duration::from_secs(2);

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let first = args.first().map(String::as_str);

    match first {
        None => run_tui(),
        Some("-h" | "--help") => {
            print_help();
            Ok(())
        }
        Some("-l" | "--list") | Some("ps") => {
            print_list();
            Ok(())
        }
        Some("--json") => {
            print_json();
            Ok(())
        }
        Some("kill") => {
            cmd_kill(&args[1..]);
            Ok(())
        }
        Some("clean") => {
            cmd_clean(&args[1..]);
            Ok(())
        }
        Some("watch") => cmd_watch(),
        Some(s) if s.parse::<u16>().is_ok() => {
            cmd_port_view(s.parse().unwrap());
            Ok(())
        }
        Some(other) => {
            eprintln!("harbor: unknown command '{other}'\n");
            print_help();
            Ok(())
        }
    }
}

// ---------------------------------------------------------------- TUI

fn run_tui() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = tui_loop(&mut terminal);
    ratatui::restore();
    result
}

fn tui_loop(terminal: &mut DefaultTerminal) -> io::Result<()> {
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
        _ => app.mode = Mode::Normal,
    }
}

// ---------------------------------------------------------------- CLI output

fn color() -> bool {
    io::stdout().is_terminal()
}

fn health_label(h: Health) -> &'static str {
    match h {
        Health::Ok => "ok",
        Health::Orphaned => "orphaned",
        Health::Zombie => "zombie",
    }
}

fn marker(l: &Listener) -> &'static str {
    if !color() {
        return if l.is_dev() { "*" } else { " " };
    }
    match l.health {
        Health::Zombie => "\x1b[31m●\x1b[0m",   // red
        Health::Orphaned => "\x1b[33m●\x1b[0m", // yellow
        Health::Ok if l.is_dev() => "\x1b[32m●\x1b[0m", // green
        _ => "\x1b[2m·\x1b[0m",                 // dim
    }
}

fn print_list() {
    let rows = Collector::new().snapshot_measured();
    if rows.is_empty() {
        println!("nothing is listening on TCP.");
        return;
    }
    for l in &rows {
        let label = l.framework.clone().unwrap_or_default();
        let mut project = l.display_project();
        if let Some(b) = &l.git_branch {
            project.push_str(&format!("  ⎇ {b}"));
        }
        let health = if l.health == Health::Ok {
            String::new()
        } else {
            format!(" !{}", health_label(l.health))
        };
        println!(
            "{} :{:<13} {:<14} {:<11} pid {:<7} {:>6} {:>5}%  {:<11} {}{}",
            marker(l),
            l.ports_str(),
            l.command,
            label,
            l.pid,
            l.mem,
            l.cpu,
            l.uptime,
            project,
            health
        );
    }
}

fn print_json() {
    let rows = Collector::new().snapshot_measured();
    let arr: Vec<serde_json::Value> = rows
        .iter()
        .map(|l| {
            serde_json::json!({
                "pid": l.pid,
                "ports": l.ports,
                "command": l.command,
                "full_cmd": l.full_cmd,
                "cwd": l.cwd.as_ref().map(|p| p.display().to_string()),
                "project": l.project,
                "git_branch": l.git_branch,
                "label": l.framework,
                "kind": l.kind.label(),
                "health": health_label(l.health),
                "cpu": l.cpu,
                "mem": l.mem,
                "uptime": l.uptime,
                "docker": l.docker.as_ref().map(|d| serde_json::json!({"name": d.name, "image": d.image})),
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&serde_json::Value::Array(arr)).unwrap());
}

fn cmd_kill(args: &[String]) {
    let hard = args.iter().any(|a| a == "-f" || a == "--force");
    let targets: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if targets.is_empty() {
        eprintln!("usage: harbor kill <port|pid|range> [more...] [-f]");
        return;
    }

    let mut collector = Collector::new();
    let snapshot = collector.snapshot();

    // Expand requested numbers (ranges like 3000-3010 included).
    let mut wanted: Vec<u32> = Vec::new();
    for t in &targets {
        if let Some((a, b)) = t.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) {
                wanted.extend(a..=b);
                continue;
            }
        }
        match t.parse::<u32>() {
            Ok(n) => wanted.push(n),
            Err(_) => eprintln!("skipping '{t}' (not a number/range)"),
        }
    }

    for n in wanted {
        // Treat n as a port first; fall back to pid.
        let by_port: Vec<&Listener> = snapshot
            .iter()
            .filter(|l| l.ports.contains(&(n as u16)) && n <= u16::MAX as u32)
            .collect();
        let pids: Vec<(u32, String)> = if !by_port.is_empty() {
            by_port.iter().map(|l| (l.pid, format!(":{n} {}", l.command))).collect()
        } else if snapshot.iter().any(|l| l.pid == n) {
            vec![(n, format!("pid {n}"))]
        } else {
            // maybe it's a live pid not in the listener set
            vec![(n, format!("pid {n}"))]
        };
        for (pid, what) in pids {
            let ok = collector.kill(pid, hard);
            let sig = if hard { "SIGKILL" } else { "SIGTERM" };
            if ok {
                println!("killed {what} (pid {pid}, {sig})");
            } else {
                eprintln!("could not kill {what} (pid {pid})");
            }
        }
    }
}

fn cmd_clean(args: &[String]) {
    let hard = args.iter().any(|a| a == "-f" || a == "--force");
    let dry = args.iter().any(|a| a == "-n" || a == "--dry-run");
    let mut collector = Collector::new();
    let snapshot = collector.snapshot();
    let reap: Vec<&Listener> = snapshot
        .iter()
        .filter(|l| l.is_dev() && l.health != Health::Ok)
        .collect();
    if reap.is_empty() {
        println!("nothing to clean — no orphaned or zombie dev processes.");
        return;
    }
    for l in reap {
        let what = format!(":{} {} [{}]", l.ports_str(), l.command, health_label(l.health));
        if dry {
            println!("would reap {what} (pid {})", l.pid);
            continue;
        }
        if collector.kill(l.pid, hard) {
            println!("reaped {what} (pid {})", l.pid);
        } else {
            eprintln!("could not reap {what} (pid {}) — zombies need their parent to exit", l.pid);
        }
    }
}

fn cmd_watch() -> io::Result<()> {
    use std::collections::HashMap;
    println!("harbor watch — Ctrl-C to stop. Watching for ports starting/stopping…\n");
    let mut collector = Collector::new();
    let mut prev: HashMap<u16, String> = HashMap::new();
    let mut first = true;
    loop {
        let mut now: HashMap<u16, String> = HashMap::new();
        for l in collector.snapshot() {
            let label = {
                let proj = l.project.clone().unwrap_or_default();
                let fw = l.framework.clone().map(|f| format!(" {f}")).unwrap_or_default();
                format!("{}{}  {}", l.command, fw, proj)
            };
            for p in &l.ports {
                now.insert(*p, label.clone());
            }
        }
        if !first {
            for (port, label) in &now {
                if !prev.contains_key(port) {
                    println!("{}  + :{port}  {label}", stamp());
                }
            }
            for (port, label) in &prev {
                if !now.contains_key(port) {
                    println!("{}  - :{port}  {label}", stamp());
                }
            }
            io::stdout().flush().ok();
        }
        prev = now;
        first = false;
        std::thread::sleep(Duration::from_millis(1500));
    }
}

fn stamp() -> String {
    // seconds since process start, monotonic — avoids pulling in a clock dep.
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    let s = start.elapsed().as_secs();
    format!("[+{:02}:{:02}]", s / 60, s % 60)
}

fn cmd_port_view(port: u16) {
    let rows = Collector::new().snapshot_measured();
    let matches: Vec<&Listener> = rows.iter().filter(|l| l.ports.contains(&port)).collect();
    if matches.is_empty() {
        println!("nothing listening on :{port}");
        return;
    }
    for l in matches {
        println!("● :{}  {}", l.ports_str(), bold(&l.command));
        println!("  pid       {}", l.pid);
        if let Some(f) = &l.framework {
            println!("  label     {f} ({})", l.kind.label());
        }
        if l.health != Health::Ok {
            println!("  health    {}", health_label(l.health));
        }
        if let Some(b) = &l.git_branch {
            println!("  branch    {b}");
        }
        if let Some(p) = &l.project {
            println!("  project   {p}");
        }
        if let Some(cwd) = &l.cwd {
            println!("  path      {}", collect::short_home(cwd));
        }
        println!("  uptime    {}", l.uptime);
        println!("  cpu/mem   {}%  {}", l.cpu, l.mem);
        println!("  command   {}", l.full_cmd);

        let tree = collect::process_tree(l.pid);
        if !tree.is_empty() {
            println!("  tree");
            for (pid, name, depth) in tree {
                println!("    {}{} {}", "  ".repeat(depth), pid, name);
            }
        }
        println!();
    }
}

fn bold(s: &str) -> String {
    if color() {
        format!("\x1b[1m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

fn print_help() {
    println!(
        "harbor — see what's docked at every local port\n\n\
         USAGE:\n  \
           harbor                interactive TUI (live)\n  \
           harbor <port>         deep view of one port (tree, branch, repo)\n  \
           harbor ps | --list    print every listener once\n  \
           harbor --json         machine-readable output\n  \
           harbor kill <p> [-f]  kill by port/pid/range (3000, 42872, 3000-3010), -f = SIGKILL\n  \
           harbor clean [-f][-n] reap orphaned/zombie dev processes (-n = preview)\n  \
           harbor watch          stream port start/stop events\n  \
           harbor -h | --help    this help\n\n\
         TUI KEYS:\n  \
           ↑/↓ or j/k  move      o  open in browser   x  kill\n  \
           /           filter    a  toggle system     r  refresh   q  quit"
    );
}
