//! Gather everything listening on a local TCP port and enrich it with the
//! owning process, its working directory (= which project), and a best-effort
//! framework guess. Everything comes from `lsof` + `ps`, which ship with macOS.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct Listener {
    pub pid: u32,
    pub ports: Vec<u16>,
    pub command: String,         // basename of the executable / process title
    pub full_cmd: String,        // full command line
    pub cwd: Option<PathBuf>,    // working directory of the process
    pub project: Option<String>, // project name (package.json name or dir basename)
    pub framework: Option<String>,
    pub cpu: String,
    pub uptime: String,
    pub is_dev: bool,
}

impl Listener {
    pub fn ports_str(&self) -> String {
        self.ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
    /// Lowercased haystack used for filtering.
    pub fn haystack(&self) -> String {
        format!(
            "{} {} {} {} {} {}",
            self.ports_str(),
            self.pid,
            self.command,
            self.full_cmd,
            self.framework.as_deref().unwrap_or(""),
            self.cwd.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        )
        .to_lowercase()
    }
}

/// Process names we consider "dev servers".
const DEV_TOKENS: &[&str] = &[
    "node", "bun", "deno", "npm", "pnpm", "yarn", "nodemon", "tsx", "ts-node", "next",
    "vite", "nuxt", "webpack", "esbuild", "rollup", "parcel", "astro", "remix", "serve",
    "http-server", "python", "flask", "gunicorn", "uvicorn", "hypercorn", "celery", "ruby",
    "rails", "puma", "rackup", "php", "artisan", "java", "gradle", "mvn", "cargo", "rustc",
    "air", "dotnet", "caddy", "ng", "rsbuild", "turbo", "wrangler", "vitest", "jest",
];

fn run(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

pub fn short_home(p: &Path) -> String {
    let s = p.display().to_string();
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy().to_string();
        if s == home {
            return "~".into();
        }
        if let Some(rest) = s.strip_prefix(&format!("{home}/")) {
            return format!("~/{rest}");
        }
    }
    s
}

/// Collect and enrich all current TCP listeners.
pub fn collect() -> Vec<Listener> {
    let lsof = run("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN"]);

    let mut ports_by_pid: BTreeMap<u32, Vec<u16>> = BTreeMap::new();
    for line in lsof.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        let pid: u32 = match fields[1].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        // The final field is "(LISTEN)"; the address sits just before it.
        let addr = if fields.last() == Some(&"(LISTEN)") {
            fields[fields.len() - 2]
        } else {
            fields[fields.len() - 1]
        };
        if let Some(port) = addr.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) {
            let entry = ports_by_pid.entry(pid).or_default();
            if !entry.contains(&port) {
                entry.push(port);
            }
        }
    }
    if ports_by_pid.is_empty() {
        return vec![];
    }

    let pids: Vec<u32> = ports_by_pid.keys().copied().collect();
    let csv = pids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");

    // Batch metadata: pid, %cpu, elapsed time, command name.
    let mut meta: HashMap<u32, (String, String, String)> = HashMap::new();
    for line in run("ps", &["-o", "pid=,%cpu=,etime=,comm=", "-p", &csv]).lines() {
        let mut it = line.split_whitespace();
        let pid = it.next().and_then(|s| s.parse::<u32>().ok());
        let cpu = it.next().unwrap_or("").to_string();
        let etime = it.next().unwrap_or("").to_string();
        let comm = it.collect::<Vec<_>>().join(" ");
        if let Some(pid) = pid {
            meta.insert(pid, (cpu, etime, comm));
        }
    }

    // Batch full command lines.
    let mut cmds: HashMap<u32, String> = HashMap::new();
    for line in run("ps", &["-o", "pid=,command=", "-p", &csv]).lines() {
        let line = line.trim_start();
        if let Some((pidstr, rest)) = line.split_once(char::is_whitespace) {
            if let Ok(pid) = pidstr.parse::<u32>() {
                cmds.insert(pid, rest.trim_start().to_string());
            }
        }
    }

    // Working directory per pid via lsof field output (p<pid> / n<path>).
    let mut cwds: HashMap<u32, PathBuf> = HashMap::new();
    let mut cur = 0u32;
    for line in run("lsof", &["-a", "-d", "cwd", "-p", &csv, "-Fpn"]).lines() {
        if let Some(rest) = line.strip_prefix('p') {
            cur = rest.parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix('n') {
            if cur != 0 {
                cwds.insert(cur, PathBuf::from(rest));
            }
        }
    }

    let mut out = Vec::new();
    for (pid, ports) in ports_by_pid {
        let (cpu, uptime, comm_raw) = meta.get(&pid).cloned().unwrap_or_default();
        let full_cmd = cmds.get(&pid).cloned().unwrap_or_else(|| comm_raw.clone());
        let command = basename(&comm_raw);
        let cwd = cwds.get(&pid).cloned();
        let framework = detect_framework(&full_cmd, &command, cwd.as_deref());
        let project = cwd.as_ref().and_then(|d| project_name(d));
        let is_dev = framework.is_some() || is_dev_name(&command);
        out.push(Listener {
            pid,
            ports,
            command,
            full_cmd,
            cwd,
            project,
            framework,
            cpu,
            uptime,
            is_dev,
        });
    }

    // Dev servers first, then by lowest port.
    out.sort_by(|a, b| {
        b.is_dev
            .cmp(&a.is_dev)
            .then(a.ports.iter().min().cmp(&b.ports.iter().min()))
    });
    out
}

fn basename(s: &str) -> String {
    Path::new(s)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| s.to_string())
}

fn is_dev_name(name: &str) -> bool {
    let n = name.to_lowercase();
    DEV_TOKENS.iter().any(|t| n.contains(t))
}

fn detect_framework(full_cmd: &str, comm: &str, cwd: Option<&Path>) -> Option<String> {
    let hay = format!("{comm} {full_cmd}").to_lowercase();
    const KW: &[(&str, &str)] = &[
        ("next-server", "Next.js"),
        ("next", "Next.js"),
        ("vite", "Vite"),
        ("nuxt", "Nuxt"),
        ("remix", "Remix"),
        ("astro", "Astro"),
        ("gatsby", "Gatsby"),
        ("storybook", "Storybook"),
        ("nodemon", "nodemon"),
        ("ts-node", "ts-node"),
        ("vitest", "Vitest"),
        ("jest", "Jest"),
        ("webpack", "Webpack"),
        ("rollup", "Rollup"),
        ("parcel", "Parcel"),
        ("esbuild", "esbuild"),
        ("expo", "Expo"),
        ("wrangler", "Wrangler"),
        ("uvicorn", "Uvicorn"),
        ("gunicorn", "Gunicorn"),
        ("hypercorn", "Hypercorn"),
        ("flask", "Flask"),
        ("manage.py", "Django"),
        ("django", "Django"),
        ("rails", "Rails"),
        ("puma", "Puma"),
        ("rackup", "Rack"),
        ("sinatra", "Sinatra"),
        ("artisan", "Laravel"),
        ("php", "PHP"),
        ("cargo", "Cargo"),
        ("air", "Air"),
    ];
    for (k, v) in KW {
        if hay.contains(k) {
            return Some((*v).to_string());
        }
    }
    cwd.and_then(framework_from_package_json)
}

fn framework_from_package_json(dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(dir.join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let has = |dep: &str| {
        json.get("dependencies").and_then(|d| d.get(dep)).is_some()
            || json.get("devDependencies").and_then(|d| d.get(dep)).is_some()
    };
    const DEPS: &[(&str, &str)] = &[
        ("next", "Next.js"),
        ("vite", "Vite"),
        ("nuxt", "Nuxt"),
        ("@remix-run/react", "Remix"),
        ("astro", "Astro"),
        ("gatsby", "Gatsby"),
        ("@nestjs/core", "NestJS"),
        ("react-scripts", "CRA"),
        ("express", "Express"),
        ("fastify", "Fastify"),
        ("koa", "Koa"),
        ("@hono/node-server", "Hono"),
        ("hono", "Hono"),
    ];
    for (dep, name) in DEPS {
        if has(dep) {
            return Some((*name).to_string());
        }
    }
    None
}

fn project_name(dir: &Path) -> Option<String> {
    if let Ok(raw) = std::fs::read_to_string(dir.join("package.json")) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(name) = json.get("name").and_then(|n| n.as_str()) {
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    dir.file_name().map(|n| n.to_string_lossy().to_string())
}

/// Kill a process by pid. `hard` sends SIGKILL instead of SIGTERM.
pub fn kill(pid: u32, hard: bool) -> bool {
    let sig = if hard { "-KILL" } else { "-TERM" };
    Command::new("kill")
        .args([sig, &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
