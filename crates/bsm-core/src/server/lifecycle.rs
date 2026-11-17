//! Starting, watching and stopping the server process.

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunInfo {
    pub pid: u32,
    pub created: u64,
    pub exe: PathBuf,
    pub port: u16,
    pub map: String,
    pub log: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Stopped,
    /// Process is running but does not answer queries yet (loading the map).
    Starting,
    Running,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub state: State,
    pub pid: Option<u32>,
    pub port: u16,
    /// Started by this tool (false: a server from the same folder started some other way).
    pub managed: bool,
    pub info: BTreeMap<String, String>,
    pub log: Option<PathBuf>,
}

impl Status {
    pub fn summary(&self) -> String {
        match self.state {
            State::Stopped => "Server: stopped".to_string(),
            State::Starting => format!(
                "Server: starting on UDP port {} (process {}), loading the map...",
                self.port,
                self.pid.unwrap_or(0)
            ),
            State::Running => {
                let g = |k: &str| self.info.get(k).cloned().unwrap_or_else(|| "?".into());
                format!(
                    "Server: RUNNING on UDP port {} (process {}) - map {}, players {}/{}",
                    self.port,
                    self.pid.unwrap_or(0),
                    g("mapname"),
                    g("clients"),
                    g("sv_maxclients")
                )
            }
        }
    }
}

/// Exact command line of the server (everything after the program name).
pub fn launch_arguments(port: u16, fs_game: &str) -> String {
    format!("-headless +set fs_game \"{fs_game}\" +set net_port \"{port}\" +set logfile 2 +exec {CFG_NAME}")
}

pub(super) fn load_run(env: &Env) -> Option<RunInfo> {
    serde_json::from_str(&fs::read_to_string(env.run_file()).ok()?).ok()
}

pub(super) fn managed_process(run: &RunInfo) -> Option<proc::ProcInfo> {
    proc::info(run.pid).filter(|p| p.created == run.created && proc::same_file(&p.exe, &run.exe))
}

/// Whether the server is running, and on which port.
pub fn status(env: &Env) -> Status {
    let run = load_run(env);
    let (pid, managed) = match run.as_ref().and_then(managed_process) {
        Some(p) => (Some(p.pid), true),
        None => (proc::running_from(&env.client_exe()).first().map(|p| p.pid), false),
    };
    let port = run.as_ref().map(|r| r.port).unwrap_or(env.settings.port);
    let log = run.as_ref().map(|r| r.log.clone());
    let Some(pid) = pid else {
        return Status {
            state: State::Stopped,
            pid: None,
            port: env.settings.port,
            managed: false,
            info: BTreeMap::new(),
            log,
        };
    };
    match net::query(Ipv4Addr::LOCALHOST.into(), port, Duration::from_millis(700)) {
        // The server answers a little before the map is loaded; it is only
        // joinable once it reports a map name.
        Some(info) if info.get("mapname").map(|m| !m.is_empty()).unwrap_or(false) => {
            Status { state: State::Running, pid: Some(pid), port, managed, info, log }
        }
        _ => Status { state: State::Starting, pid: Some(pid), port, managed, info: BTreeMap::new(), log },
    }
}

pub(super) fn spawn_server(env: &Env, fs_game: &str, log_path: &Path) -> Result<std::process::Child> {
    let exe = env.client_exe();
    let out = io_ctx(fs::File::create(log_path), "create", log_path)?;
    let err = io_ctx(out.try_clone(), "create", log_path)?;
    proc::keep_std_handles_private();
    let child = std::process::Command::new(&exe) // full path on purpose
        .raw_arg(launch_arguments(env.settings.port, fs_game))
        .current_dir(&env.server_dir)
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err)
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
        .spawn();
    io_ctx(child, "start", &exe)
}

pub(super) fn log_tail(path: &Path, lines: usize) -> String {
    let text = fs::read(path).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\r\n")
}

/// Starts the server and returns once the process is up. The map then takes
/// a while to load; poll [`status`] until it reports `Running`.
pub fn start(env: &Env, log: &dyn Fn(&str)) -> Result<RunInfo> {
    let current = status(env);
    if current.state != State::Stopped {
        return Err(Error::Other(format!(
            "The server is already running (process {}). Stop it first.",
            current.pid.unwrap_or(0)
        )));
    }
    if !env.has_server_files() || !env.client_exe().is_file() {
        return Err(Error::Other("The server is not set up yet. Run Setup first.".into()));
    }
    // Map, mod and menu options may have changed since setup.
    let sel = prepare_content(env, log)?;
    // A damaged map file makes the server sit at "loading" for ever; say so instead.
    let damaged = damaged_files(env, Some(&map_file(env, &sel)));
    if !damaged.is_empty() {
        return Err(damaged_error(&damaged));
    }
    install_scripts(env, log)?;
    write_config(env, &sel, log)?;
    let fs_game = sel.fs_game();
    let port = env.settings.port;
    if !net::port_free(port) {
        let hint = net::next_free_port(port.saturating_add(1))
            .map(|p| format!(" Port {p} is free."))
            .unwrap_or_default();
        return Err(Error::Other(format!(
            "UDP port {port} is already in use on this PC (a game client or another server?). Choose another port.{hint}"
        )));
    }
    io_ctx(fs::create_dir_all(env.logs_dir()), "create", &env.logs_dir())?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let log_path = env.logs_dir().join(format!("server-{stamp}.log"));
    let exe = env.client_exe();

    log(&format!("Starting: \"{}\" {}", exe.display(), launch_arguments(port, &fs_game)));
    let mut child = spawn_server(env, &fs_game, &log_path)?;
    let mut pid = child.id();

    // Watch the first seconds. BOIII may update itself: the process we started
    // then exits and a replacement runs without our arguments. Stop that one
    // and start again so the settings apply.
    let mut relaunched = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        match child.try_wait() {
            Ok(None) => {
                if net::query(Ipv4Addr::LOCALHOST.into(), port, Duration::from_millis(200)).is_some() {
                    break;
                }
            }
            _ => {
                let replacements = proc::running_from(&exe);
                if !replacements.is_empty() && !relaunched {
                    log("BOIII updated itself and restarted; starting the server again with its settings.");
                    for p in replacements {
                        proc::terminate(p.pid);
                    }
                    relaunched = true;
                    child = spawn_server(env, &fs_game, &log_path)?;
                    pid = child.id();
                    continue;
                }
                if replacements.is_empty() {
                    let tail = log_tail(&log_path, 15);
                    return Err(Error::Other(format!(
                        "boiii.exe exited right after starting.{}\r\nFull log: {}",
                        if tail.is_empty() { String::new() } else { format!("\r\nLast output:\r\n{tail}") },
                        log_path.display()
                    )));
                }
                break;
            }
        }
    }

    let info = proc::info(pid)
        .ok_or_else(|| Error::Other("The server process disappeared right after starting.".into()))?;
    let run = RunInfo {
        pid,
        created: info.created,
        exe: exe.clone(),
        port,
        map: env.settings.map.clone(),
        log: log_path,
    };
    io_ctx(fs::write(env.run_file(), serde_json::to_string_pretty(&run)?), "write", &env.run_file())?;
    log(&format!("Server process {pid} started on UDP port {port}. Loading the map can take a minute."));
    if !env.settings.continue_save.is_empty() {
        log(&format!(
            "Continuing from saved game slot {} once the first player is in.",
            env.settings.continue_save
        ));
        let mut saved = Settings::load(&env.data_dir);
        saved.continue_save.clear();
        let _ = saved.save(&env.data_dir);
    }
    Ok(run)
}

/// Polls until the server answers queries, the process dies or time runs out.
pub fn wait_until_running(env: &Env, timeout: Duration) -> Status {
    let deadline = Instant::now() + timeout;
    loop {
        let st = status(env);
        if st.state != State::Starting || Instant::now() >= deadline {
            return st;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Stops the server. Only processes started from this server folder's
/// `boiii.exe` are touched. Returns how many were stopped.
pub fn stop(env: &Env, log: &dyn Fn(&str)) -> Result<u32> {
    let mut pids: Vec<u32> = Vec::new();
    if let Some(p) = load_run(env).as_ref().and_then(managed_process) {
        pids.push(p.pid);
    }
    for p in proc::running_from(&env.client_exe()) {
        if !pids.contains(&p.pid) {
            pids.push(p.pid);
        }
    }
    let mut stopped = 0;
    for pid in &pids {
        if proc::terminate(*pid) {
            stopped += 1;
            log(&format!("Stopped server process {pid}."));
        } else {
            log(&format!("Could not stop process {pid}."));
        }
    }
    let _ = fs::remove_file(env.run_file());
    if pids.is_empty() {
        log("The server was not running.");
    } else if stopped as usize != pids.len() {
        return Err(Error::Other("Not every server process could be stopped.".into()));
    }
    Ok(stopped)
}
