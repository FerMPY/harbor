//! Gather everything listening on a local TCP port and enrich it with the
//! owning process, its working directory (= which project), git branch,
//! framework / database / docker label, and health.
//!
//! Cross-platform (macOS + Linux) via the `listeners` and `sysinfo` crates —
//! no `lsof`/`ps` shell-outs. `docker ps` is the one optional shell-out, used
//! only to map host ports to container names (skipped if docker isn't running).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use sysinfo::{
    Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, Signal, System, UpdateKind,
};

/// Refresh kind that actually fetches the command line and working directory
/// (the plain `refresh_processes` skips both for speed). cmd/cwd/exe are only
/// fetched once per process; memory/cpu refresh every time.
fn detail_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_memory()
        .with_cpu()
        .with_cmd(UpdateKind::OnlyIfNotSet)
        .with_cwd(UpdateKind::OnlyIfNotSet)
        .with_exe(UpdateKind::OnlyIfNotSet)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dev,
    Database,
    Docker,
    System,
}

impl Kind {
    fn rank(self) -> u8 {
        match self {
            Kind::Dev => 0,
            Kind::Database => 1,
            Kind::Docker => 2,
            Kind::System => 3,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Dev => "dev",
            Kind::Database => "db",
            Kind::Docker => "docker",
            Kind::System => "system",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    Orphaned,
    Zombie,
}

#[derive(Clone)]
pub struct DockerInfo {
    pub name: String,
    pub image: String,
}

#[derive(Clone)]
pub struct Listener {
    pub pid: u32,
    pub ports: Vec<u16>,
    pub command: String,
    pub full_cmd: String,
    pub cwd: Option<PathBuf>,
    pub project: Option<String>,
    pub git_branch: Option<String>,
    pub framework: Option<String>,
    pub kind: Kind,
    pub health: Health,
    pub cpu: String,
    pub mem: String,
    pub uptime: String,
    pub docker: Option<DockerInfo>,
}

impl Listener {
    pub fn is_dev(&self) -> bool {
        self.kind != Kind::System
    }
    pub fn ports_str(&self) -> String {
        self.ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
    pub fn min_port(&self) -> u16 {
        self.ports.iter().copied().min().unwrap_or(0)
    }
    /// Best human label for "which project / container", excluding git branch.
    pub fn display_project(&self) -> String {
        if self.kind == Kind::Docker {
            return self.project.clone().unwrap_or_default();
        }
        self.cwd
            .as_ref()
            .map(|p| short_home(p))
            .filter(|s| s != "/")
            .or_else(|| self.project.clone())
            .unwrap_or_default()
    }
    pub fn haystack(&self) -> String {
        format!(
            "{} {} {} {} {} {} {}",
            self.ports_str(),
            self.pid,
            self.command,
            self.full_cmd,
            self.framework.as_deref().unwrap_or(""),
            self.git_branch.as_deref().unwrap_or(""),
            self.cwd
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        )
        .to_lowercase()
    }
}

const DEV_TOKENS: &[&str] = &[
    "node",
    "bun",
    "deno",
    "npm",
    "pnpm",
    "yarn",
    "nodemon",
    "tsx",
    "ts-node",
    "next",
    "vite",
    "nuxt",
    "webpack",
    "esbuild",
    "rollup",
    "parcel",
    "astro",
    "remix",
    "serve",
    "http-server",
    "python",
    "flask",
    "gunicorn",
    "uvicorn",
    "hypercorn",
    "celery",
    "ruby",
    "rails",
    "puma",
    "rackup",
    "php",
    "artisan",
    "java",
    "gradle",
    "mvn",
    "cargo",
    "rustc",
    "air",
    "dotnet",
    "caddy",
    "ng",
    "rsbuild",
    "turbo",
    "wrangler",
    "vitest",
    "jest",
    "metro",
];

/// Holds a persistent `System` so CPU usage is meaningful across refreshes.
pub struct Collector {
    sys: System,
}

impl Collector {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        Self { sys }
    }

    /// One refresh — CPU reflects usage since the previous snapshot (good for
    /// the always-on TUI which ticks every couple seconds).
    pub fn snapshot(&mut self) -> Vec<Listener> {
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, detail_kind());
        build(&self.sys)
    }

    /// Two refreshes with a short gap — gives a real CPU reading for one-shot
    /// CLI commands (`--list`, `ps`, `--json`).
    pub fn snapshot_measured(&mut self) -> Vec<Listener> {
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, detail_kind());
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, detail_kind());
        build(&self.sys)
    }

    pub fn kill(&mut self, pid: u32, hard: bool) -> bool {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        match self.sys.process(Pid::from_u32(pid)) {
            Some(p) if hard => p.kill(),
            Some(p) => p.kill_with(Signal::Term).unwrap_or(false),
            None => false,
        }
    }
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}

fn build(sys: &System) -> Vec<Listener> {
    let all = match listeners::get_all() {
        Ok(set) => set,
        Err(_) => return vec![],
    };

    // pid -> (ports, fallback name from the listeners crate)
    let mut by_pid: BTreeMap<u32, (Vec<u16>, String)> = BTreeMap::new();
    for l in all {
        let entry = by_pid
            .entry(l.process.pid)
            .or_insert_with(|| (Vec::new(), l.process.name.clone()));
        let port = l.socket.port();
        if !entry.0.contains(&port) {
            entry.0.push(port);
        }
    }
    if by_pid.is_empty() {
        return vec![];
    }

    let docker = docker_map();

    let mut out = Vec::new();
    for (pid, (mut ports, fallback_name)) in by_pid {
        ports.sort_unstable();
        let proc = sys.process(Pid::from_u32(pid));

        let command = proc
            .map(|p| basename(&p.name().to_string_lossy()))
            .unwrap_or(fallback_name);
        let full_cmd = proc
            .map(|p| {
                p.cmd()
                    .iter()
                    .map(|s| s.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| command.clone());
        let cwd = proc.and_then(|p| p.cwd().map(Path::to_path_buf));
        let cpu = proc
            .map(|p| format!("{:.1}", p.cpu_usage()))
            .unwrap_or_default();
        let mem = proc.map(|p| human_mem(p.memory())).unwrap_or_default();
        let uptime = proc.map(|p| fmt_uptime(p.run_time())).unwrap_or_default();
        let parent_is_init = proc.and_then(|p| p.parent()).map(|pp| pp.as_u32()) == Some(1);
        let zombie = proc
            .map(|p| matches!(p.status(), ProcessStatus::Zombie))
            .unwrap_or(false);

        // Classify: docker > database > dev > system.
        let mut docker_info = None;
        let framework;
        let kind;
        if let Some(di) = ports.iter().find_map(|p| docker.get(p)) {
            docker_info = Some(di.clone());
            framework = Some(di.image.clone());
            kind = Kind::Docker;
        } else if let Some(db) = detect_database(&command, &ports) {
            framework = Some(db);
            kind = Kind::Database;
        } else {
            framework = detect_framework(&full_cmd, &command, cwd.as_deref());
            kind = if framework.is_some() || is_dev_name(&command) {
                Kind::Dev
            } else {
                Kind::System
            };
        }

        let health = if zombie {
            Health::Zombie
        } else if parent_is_init && kind == Kind::Dev {
            Health::Orphaned
        } else {
            Health::Ok
        };

        let project = cwd
            .as_ref()
            .and_then(|d| project_name(d))
            .or_else(|| docker_info.as_ref().map(|d| d.name.clone()));
        let git_branch = cwd.as_deref().and_then(git_branch);

        out.push(Listener {
            pid,
            ports,
            command,
            full_cmd,
            cwd,
            project,
            git_branch,
            framework,
            kind,
            health,
            cpu,
            mem,
            uptime,
            docker: docker_info,
        });
    }

    out.sort_by(|a, b| {
        a.kind
            .rank()
            .cmp(&b.kind.rank())
            .then(a.min_port().cmp(&b.min_port()))
    });
    out
}

pub fn open_url(port: u16) -> bool {
    opener::open(format!("http://localhost:{port}")).is_ok()
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

// --- pure helpers (unit-tested) -----------------------------------------

pub fn human_mem(bytes: u64) -> String {
    if bytes == 0 {
        return String::new();
    }
    let kb = bytes / 1024;
    if kb >= 1024 * 1024 {
        format!("{:.1}G", kb as f64 / (1024.0 * 1024.0))
    } else if kb >= 1024 {
        format!("{}M", kb / 1024)
    } else {
        format!("{kb}K")
    }
}

pub fn fmt_uptime(secs: u64) -> String {
    let d = secs / 86_400;
    let h = (secs % 86_400) / 3_600;
    let m = (secs % 3_600) / 60;
    let s = secs % 60;
    if d > 0 {
        format!("{d}d {h:02}:{m:02}:{s:02}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
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

pub fn detect_database(comm: &str, ports: &[u16]) -> Option<String> {
    let n = comm.to_lowercase();
    const NAMES: &[(&str, &str)] = &[
        ("postgres", "PostgreSQL"),
        ("postmaster", "PostgreSQL"),
        ("redis-server", "Redis"),
        ("redis", "Redis"),
        ("mongod", "MongoDB"),
        ("mysqld", "MySQL"),
        ("mariadbd", "MariaDB"),
        ("memcached", "Memcached"),
        ("nginx", "nginx"),
        ("etcd", "etcd"),
        ("clickhouse", "ClickHouse"),
        ("rabbitmq", "RabbitMQ"),
        ("cockroach", "CockroachDB"),
    ];
    for (k, v) in NAMES {
        if n.contains(k) {
            return Some((*v).to_string());
        }
    }
    // Fall back to canonical ports only when the process name gave nothing away.
    const PORTS: &[(u16, &str)] = &[
        (5432, "PostgreSQL"),
        (6379, "Redis"),
        (27017, "MongoDB"),
        (3306, "MySQL"),
        (11211, "Memcached"),
        (9042, "Cassandra"),
        (5672, "RabbitMQ"),
    ];
    for &p in ports {
        if let Some((_, name)) = PORTS.iter().find(|(cp, _)| *cp == p) {
            return Some((*name).to_string());
        }
    }
    None
}

pub fn detect_framework(full_cmd: &str, comm: &str, cwd: Option<&Path>) -> Option<String> {
    let hay = format!("{comm} {full_cmd}").to_lowercase();
    const KW: &[(&str, &str)] = &[
        ("next-server", "Next.js"),
        ("next", "Next.js"),
        ("nuxt", "Nuxt"),
        ("vite", "Vite"),
        ("remix", "Remix"),
        ("astro", "Astro"),
        ("gatsby", "Gatsby"),
        ("sveltekit", "SvelteKit"),
        ("@sveltejs", "SvelteKit"),
        ("solid-start", "SolidStart"),
        ("qwik", "Qwik"),
        ("storybook", "Storybook"),
        ("nodemon", "nodemon"),
        ("ts-node", "ts-node"),
        ("vitest", "Vitest"),
        ("jest", "Jest"),
        ("webpack", "Webpack"),
        ("rollup", "Rollup"),
        ("parcel", "Parcel"),
        ("esbuild", "esbuild"),
        ("metro", "Metro"),
        ("expo", "Expo"),
        ("ng serve", "Angular"),
        ("@angular", "Angular"),
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
            || json
                .get("devDependencies")
                .and_then(|d| d.get(dep))
                .is_some()
    };
    const DEPS: &[(&str, &str)] = &[
        ("next", "Next.js"),
        ("nuxt", "Nuxt"),
        ("vite", "Vite"),
        ("@angular/core", "Angular"),
        ("@sveltejs/kit", "SvelteKit"),
        ("@remix-run/react", "Remix"),
        ("astro", "Astro"),
        ("gatsby", "Gatsby"),
        ("expo", "Expo"),
        ("react-native", "React Native"),
        ("@nestjs/core", "NestJS"),
        ("react-scripts", "CRA"),
        ("express", "Express"),
        ("fastify", "Fastify"),
        ("koa", "Koa"),
        ("hono", "Hono"),
        ("@hono/node-server", "Hono"),
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

/// Current git branch for the repo containing `start` (worktree-aware).
pub fn git_branch(start: &Path) -> Option<String> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let dotgit = d.join(".git");
        if dotgit.is_dir() {
            return read_head(&dotgit);
        }
        if dotgit.is_file() {
            // Linked worktree: ".git" is a file "gitdir: <path>".
            let content = std::fs::read_to_string(&dotgit).ok()?;
            let gitdir = content.strip_prefix("gitdir:")?.trim();
            return read_head(Path::new(gitdir));
        }
        dir = d.parent();
    }
    None
}

fn read_head(gitdir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(gitdir.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(branch) = head.strip_prefix("ref: refs/heads/") {
        Some(branch.to_string())
    } else if head.len() >= 7 {
        Some(head.chars().take(7).collect()) // detached HEAD -> short sha
    } else {
        None
    }
}

/// Map host port -> container, via `docker ps`. Empty if docker is unavailable.
fn docker_map() -> HashMap<u16, DockerInfo> {
    let mut map = HashMap::new();
    let out = Command::new("docker")
        .args(["ps", "--format", "{{.Names}}\t{{.Image}}\t{{.Ports}}"])
        .output();
    let Ok(out) = out else { return map };
    if !out.status.success() {
        return map;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let mut cols = line.split('\t');
        let name = cols.next().unwrap_or("").to_string();
        let image = cols.next().unwrap_or("").to_string();
        let ports = cols.next().unwrap_or("");
        for hp in parse_docker_host_ports(ports) {
            map.entry(hp).or_insert_with(|| DockerInfo {
                name: name.clone(),
                image: image.clone(),
            });
        }
    }
    map
}

/// Extract host ports from a docker "Ports" column like
/// "0.0.0.0:5432->5432/tcp, :::5432->5432/tcp, 8080->80/tcp".
pub fn parse_docker_host_ports(ports: &str) -> Vec<u16> {
    let mut out = Vec::new();
    for seg in ports.split(',') {
        let seg = seg.trim();
        let Some((host, _)) = seg.split_once("->") else {
            continue;
        };
        // host is like "0.0.0.0:5432" / ":::5432" / "5432"
        let port = host.rsplit(':').next().unwrap_or(host);
        if let Ok(p) = port.parse::<u16>() {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Descendant processes of `root` as (pid, name, depth), depth-first.
pub fn process_tree(root: u32) -> Vec<(u32, String, usize)> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut names: HashMap<u32, String> = HashMap::new();
    for (pid, p) in sys.processes() {
        let id = pid.as_u32();
        names.insert(id, p.name().to_string_lossy().to_string());
        if let Some(parent) = p.parent() {
            children.entry(parent.as_u32()).or_default().push(id);
        }
    }

    fn walk(
        id: u32,
        depth: usize,
        children: &HashMap<u32, Vec<u32>>,
        names: &HashMap<u32, String>,
        out: &mut Vec<(u32, String, usize)>,
    ) {
        if let Some(kids) = children.get(&id) {
            let mut kids = kids.clone();
            kids.sort_unstable();
            for c in kids {
                out.push((c, names.get(&c).cloned().unwrap_or_default(), depth));
                walk(c, depth + 1, children, names, out);
            }
        }
    }

    let mut out = Vec::new();
    walk(root, 0, &children, &names, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn mem_formatting() {
        assert_eq!(human_mem(0), "");
        assert_eq!(human_mem(512), "0K"); // <1KB rounds down to 0K
        assert_eq!(human_mem(2 * 1024), "2K");
        assert_eq!(human_mem(5 * 1024 * 1024), "5M");
        assert_eq!(human_mem(2 * 1024 * 1024 * 1024), "2.0G");
    }

    #[test]
    fn uptime_formatting() {
        assert_eq!(fmt_uptime(0), "00:00:00");
        assert_eq!(fmt_uptime(65), "00:01:05");
        assert_eq!(fmt_uptime(3 * 3600 + 4 * 60 + 5), "03:04:05");
        assert_eq!(fmt_uptime(2 * 86_400 + 3600), "2d 01:00:00");
    }

    #[test]
    fn framework_from_cmdline() {
        assert_eq!(
            detect_framework("node /x/next dev", "node", None).as_deref(),
            Some("Next.js")
        );
        assert_eq!(
            detect_framework("vite", "node", None).as_deref(),
            Some("Vite")
        );
        assert_eq!(
            detect_framework("ng serve", "node", None).as_deref(),
            Some("Angular")
        );
        assert_eq!(
            detect_framework("python -m uvicorn app:app", "python", None).as_deref(),
            Some("Uvicorn")
        );
        assert_eq!(
            detect_framework("/usr/bin/something", "something", None),
            None
        );
    }

    #[test]
    fn database_by_name_and_port() {
        assert_eq!(
            detect_database("postgres", &[5432]).as_deref(),
            Some("PostgreSQL")
        );
        assert_eq!(
            detect_database("redis-server", &[6379]).as_deref(),
            Some("Redis")
        );
        assert_eq!(detect_database("node", &[6379]).as_deref(), Some("Redis")); // port fallback
        assert_eq!(detect_database("node", &[3000]), None);
    }

    #[test]
    fn docker_port_parsing() {
        assert_eq!(
            parse_docker_host_ports("0.0.0.0:5432->5432/tcp, :::5432->5432/tcp"),
            vec![5432]
        );
        assert_eq!(parse_docker_host_ports("8080->80/tcp"), vec![8080]);
        assert_eq!(parse_docker_host_ports(""), Vec::<u16>::new());
        assert_eq!(
            parse_docker_host_ports("0.0.0.0:5432->5432/tcp, 0.0.0.0:8025->8025/tcp"),
            vec![5432, 8025]
        );
    }

    #[test]
    fn head_parsing_via_tempdir() {
        // build a fake .git dir
        let base = std::env::temp_dir().join(format!("harbor_test_git_{}", std::process::id()));
        let dotgit = base.join(".git");
        std::fs::create_dir_all(&dotgit).unwrap();
        std::fs::write(dotgit.join("HEAD"), "ref: refs/heads/feat/multiplatform\n").unwrap();
        assert_eq!(git_branch(&base).as_deref(), Some("feat/multiplatform"));
        std::fs::write(dotgit.join("HEAD"), "a1b2c3d4e5f6\n").unwrap();
        assert_eq!(git_branch(&base).as_deref(), Some("a1b2c3d"));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn short_home_collapses() {
        if let Some(home) = std::env::var_os("HOME") {
            let home = std::path::PathBuf::from(home);
            assert_eq!(short_home(&home), "~");
            assert_eq!(short_home(&home.join("x/y")), "~/x/y");
        }
        assert_eq!(short_home(Path::new("/etc/hosts")), "/etc/hosts");
    }
}
