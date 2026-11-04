//! Otto's statistics engine: R.
//!
//! Every statistic Otto reports is computed by R, in one persistent `Rscript` worker that
//! answers JSON requests (one per line). Otto only gathers the data and lays out what R
//! returns. Packages live in Otto's own library, installed from a dated CRAN snapshot, so
//! results are reproducible and each one records the R and package versions used.
//!
//! There is no fallback: without R, statistics report that the engine needs setting up.

pub mod locate;
pub mod results;

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Hides the console window of child processes started from the GUI.
#[cfg(windows)]
pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Packages the worker needs.
/// (`survival` ships with R as a recommended package.)
pub const PACKAGES: &[&str] = &["jsonlite", "car", "nortest", "minpack.lm", "survival"];
/// Installed with them for cross-checks against established implementations (drc for
/// dose-response, multcomp for multiple comparisons).
pub const CHECK_PACKAGES: &[&str] = &["drc", "multcomp", "coin"];
/// The dated CRAN snapshot (Posit Package Manager, prebuilt Windows binaries) every Otto
/// installs from, so everyone computes with the same package versions.
pub const REPOSITORY: &str = "https://packagemanager.posit.co/cran/2026-10-01";

const SOURCES: &[(&str, &str)] = &[
    ("worker.R", include_str!("../r/worker.R")),
    ("common.R", include_str!("../r/common.R")),
    ("tests.R", include_str!("../r/tests.R")),
    ("anova.R", include_str!("../r/anova.R")),
    ("twoway.R", include_str!("../r/twoway.R")),
    ("survival.R", include_str!("../r/survival.R")),
    ("contingency.R", include_str!("../r/contingency.R")),
    ("normality.R", include_str!("../r/normality.R")),
    ("outliers.R", include_str!("../r/outliers.R")),
    ("xy.R", include_str!("../r/xy.R")),
    ("status.R", include_str!("../r/status.R")),
    ("packages.R", include_str!("../r/packages.R")),
];

/// Seconds a request may take before the worker is restarted.
const TIMEOUT: Duration = Duration::from_secs(90);
const CACHE_LIMIT: usize = 2000;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RError {
    #[error("The statistics engine is not set up: R is not installed (or not found). Install R from https://cran.r-project.org (or: winget install RProject.R), then open Analyze > Statistics engine and click Set up.")]
    NotInstalled,
    #[error("The statistics engine is not set up: R packages missing ({0}). Open Analyze > Statistics engine and click Set up.")]
    NotSetUp(String),
    /// R could not compute the result (its own error message).
    #[error("{0}")]
    Failed(String),
    #[error("The statistics engine (R) stopped: {0}")]
    Crashed(String),
    #[error("The statistics engine (R) took longer than {0} seconds and was restarted")]
    Timeout(u64),
    /// Queued for the background worker (see [`set_background`]); ask again later.
    #[error("Calculating in R…")]
    Pending,
}

impl RError {
    /// Whether the engine itself is missing or broken (as opposed to R rejecting the data,
    /// or a result still being calculated).
    pub fn is_engine_problem(&self) -> bool {
        !matches!(self, RError::Failed(_) | RError::Pending)
    }
}

/// One request to R.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub op: String,
    pub data: Value,
    pub options: Value,
}

impl Request {
    pub fn new(op: &str, data: Value, options: Value) -> Self {
        Request { op: op.to_string(), data, options }
    }

    fn key(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// R's answer and which engine computed it ("R 4.6.1 · stats · minpack.lm 1.2-4").
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub result: Value,
    pub engine: String,
}

/// Where Otto keeps its R scripts and package library (`%LOCALAPPDATA%\Otto\r`).
pub fn home() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("Otto").join("r")
}

/// Otto's own R package library.
pub fn library_dir() -> PathBuf {
    home().join("library")
}

fn scripts_dir() -> PathBuf {
    home().join("scripts")
}

/// Writes the embedded R sources next to the library (only when they changed).
fn write_sources() -> std::io::Result<PathBuf> {
    let dir = scripts_dir();
    std::fs::create_dir_all(&dir)?;
    for (name, text) in SOURCES {
        let path = dir.join(name);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(*text) {
            std::fs::write(&path, text)?;
        }
    }
    Ok(dir)
}

fn command(rscript: &PathBuf) -> Command {
    let mut c = Command::new(rscript);
    let lib = library_dir();
    c.env("R_LIBS", &lib).env("R_LIBS_USER", &lib).env_remove("R_HOME");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

// ---------------------------------------------------------------------------
// The worker
// ---------------------------------------------------------------------------

struct Worker {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    next_id: u64,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn worker_slot() -> &'static Mutex<Option<Worker>> {
    static W: OnceLock<Mutex<Option<Worker>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(None))
}

fn cache() -> &'static Mutex<HashMap<String, Result<Reply, String>>> {
    static C: OnceLock<Mutex<HashMap<String, Result<Reply, String>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn stderr_tail(w: &Worker) -> String {
    let lines = w.stderr.lock().map(|l| l.iter().cloned().collect::<Vec<_>>()).unwrap_or_default();
    lines.join("\n")
}

fn start_worker() -> Result<Worker, RError> {
    let rscript = locate::rscript().ok_or(RError::NotInstalled)?;
    let dir = write_sources().map_err(|e| RError::Crashed(format!("could not write the R scripts: {e}")))?;
    std::fs::create_dir_all(library_dir()).map_err(|e| RError::Crashed(format!("could not create the R library: {e}")))?;
    let mut child = command(&rscript)
        .arg("--vanilla")
        .arg(dir.join("worker.R"))
        .arg(&dir)
        .arg(library_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RError::Crashed(format!("could not start {}: {e}", rscript.display())))?;
    let stdin = child.stdin.take().ok_or_else(|| RError::Crashed("no stdin".into()))?;
    let stdout = child.stdout.take().ok_or_else(|| RError::Crashed("no stdout".into()))?;
    let stderr_pipe = child.stderr.take().ok_or_else(|| RError::Crashed("no stderr".into()))?;
    let (tx, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if line.starts_with("@@otto") && tx.send(line).is_err() {
                break;
            }
        }
    });
    let stderr = Arc::new(Mutex::new(VecDeque::new()));
    let tail = stderr.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr_pipe).lines() {
            let Ok(line) = line else { break };
            if let Ok(mut t) = tail.lock() {
                t.push_back(line);
                if t.len() > 40 {
                    t.pop_front();
                }
            }
        }
    });
    let w = Worker { child, stdin, lines, stderr, next_id: 1 };
    let hello = match w.lines.recv_timeout(Duration::from_secs(60)) {
        Ok(l) => l,
        Err(_) => return Err(RError::Crashed(format!("R did not start. {}", stderr_tail(&w)))),
    };
    // "@@otto-hello R=4.5.1 missing=car,nortest"
    let missing = hello.split_whitespace().find_map(|p| p.strip_prefix("missing=")).unwrap_or("");
    if !missing.is_empty() {
        return Err(RError::NotSetUp(missing.replace(',', ", ")));
    }
    Ok(w)
}

fn send(w: &mut Worker, req: &Request) -> Result<Reply, RError> {
    let id = w.next_id;
    w.next_id += 1;
    let line = json!({ "id": id, "op": req.op, "data": req.data, "options": req.options });
    writeln!(w.stdin, "{line}").and_then(|_| w.stdin.flush()).map_err(|e| RError::Crashed(format!("{e}. {}", stderr_tail(w))))?;
    loop {
        let line = match w.lines.recv_timeout(TIMEOUT) {
            Ok(l) => l,
            Err(RecvTimeoutError::Timeout) => return Err(RError::Timeout(TIMEOUT.as_secs())),
            Err(RecvTimeoutError::Disconnected) => return Err(RError::Crashed(stderr_tail(w))),
        };
        let Some(body) = line.strip_prefix("@@otto ") else { continue };
        let v: Value = serde_json::from_str(body).map_err(|e| RError::Crashed(format!("unreadable reply: {e}")))?;
        if v["id"].as_u64() != Some(id) {
            continue;
        }
        let engine = v["engine"].as_str().unwrap_or_default().to_string();
        return if v["ok"].as_bool() == Some(true) {
            Ok(Reply { result: v["result"].clone(), engine })
        } else {
            Err(RError::Failed(v["error"].as_str().unwrap_or("R reported an error").to_string()))
        };
    }
}

// ---------------------------------------------------------------------------
// Background calculation
// ---------------------------------------------------------------------------

thread_local! {
    /// On the GUI thread: requests that are not cached go to the background worker and
    /// `call` answers `Pending` at once, so the window never waits for R.
    static BACKGROUND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Increments whenever a background result arrives (views refresh when it changes).
static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn pending_set() -> &'static Mutex<std::collections::HashSet<String>> {
    static P: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(std::collections::HashSet::new()))
}

/// The engine problem the background worker last ran into (R missing, packages missing…):
/// answered at once instead of retrying on every redraw, until [`retry`] or [`restart`].
fn engine_down() -> &'static Mutex<Option<RError>> {
    static D: OnceLock<Mutex<Option<RError>>> = OnceLock::new();
    D.get_or_init(|| Mutex::new(None))
}

fn queue() -> &'static Mutex<mpsc::Sender<Request>> {
    static Q: OnceLock<Mutex<mpsc::Sender<Request>>> = OnceLock::new();
    Q.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Request>();
        std::thread::spawn(move || {
            for req in rx {
                let key = req.key();
                let result = run(&req, &key);
                if let Ok(mut d) = engine_down().lock() {
                    *d = match result {
                        Err(e) if e.is_engine_problem() => Some(e),
                        _ => None,
                    };
                }
                if let Ok(mut p) = pending_set().lock() {
                    p.remove(&key);
                }
                GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        Mutex::new(tx)
    })
}

/// Calculate in the background on this thread (the GUI thread) instead of waiting for R.
pub fn set_background(on: bool) {
    BACKGROUND.with(|b| b.set(on));
}

/// Runs `f` waiting for R's answers even on a background thread (e.g. a Claude tool call,
/// which needs its results).
pub fn blocking<T>(f: impl FnOnce() -> T) -> T {
    let before = BACKGROUND.with(|b| b.replace(false));
    let out = f();
    BACKGROUND.with(|b| b.set(before));
    out
}

/// Changes whenever a background result arrives.
pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::SeqCst)
}

/// Requests queued or running in the background.
pub fn pending() -> usize {
    pending_set().lock().map(|p| p.len()).unwrap_or(0)
}

/// Lets the background worker try R again after an engine problem.
pub fn retry() {
    if let Ok(mut d) = engine_down().lock() {
        *d = None;
    }
}

/// Runs a request in R (or answers it from the cache: the same request always gives the
/// same answer, so redraws never wait for R). On a background thread (see
/// [`set_background`]) a request that is not cached is queued and answered `Pending`.
pub fn call(req: &Request) -> Result<Reply, RError> {
    record(req);
    let key = req.key();
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit.map_err(RError::Failed);
    }
    if BACKGROUND.with(|b| b.get()) {
        if let Some(e) = engine_down().lock().ok().and_then(|d| d.clone()) {
            return Err(e);
        }
        let new = pending_set().lock().map(|mut p| p.insert(key)).unwrap_or(false);
        if new {
            let _ = queue().lock().map(|q| q.send(req.clone()));
        }
        return Err(RError::Pending);
    }
    run(req, &key)
}

/// Runs a request in the worker, waiting for the answer, and caches it.
fn run(req: &Request, key: &str) -> Result<Reply, RError> {
    let mut slot = worker_slot().lock().unwrap_or_else(|p| p.into_inner());
    // Another thread may have computed it meanwhile.
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(key).cloned()) {
        return hit.map_err(RError::Failed);
    }
    if slot.is_none() {
        *slot = Some(start_worker()?);
    }
    let result = match send(slot.as_mut().expect("worker"), req) {
        Err(e) if e.is_engine_problem() => {
            // A crashed or stuck worker is replaced on the next call.
            *slot = None;
            return Err(e);
        }
        r => r,
    };
    if let Ok(mut c) = cache().lock() {
        if c.len() >= CACHE_LIMIT {
            c.clear();
        }
        c.insert(key.to_string(), result.clone().map_err(|e| e.to_string()));
    }
    result
}

/// Runs a request and reads R's result into `T`.
pub fn call_as<T: DeserializeOwned>(req: &Request) -> Result<(T, String), RError> {
    let reply = call(req)?;
    let value = serde_json::from_value(reply.result).map_err(|e| RError::Crashed(format!("unexpected result from R for '{}': {e}", req.op)))?;
    Ok((value, reply.engine))
}

/// Starts the worker in the background and loads the packages, so the first analysis
/// doesn't wait for R to start.
pub fn warm_up() {
    std::thread::spawn(|| {
        let _ = call(&Request::new("warm", Value::Null, Value::Null));
    });
}

/// Stops the worker (e.g. after installing packages) and forgets cached results.
pub fn restart() {
    retry();
    *worker_slot().lock().unwrap_or_else(|p| p.into_inner()) = None;
    if let Ok(mut c) = cache().lock() {
        c.clear();
    }
}

// ---------------------------------------------------------------------------
// Status and setup
// ---------------------------------------------------------------------------

/// What the Statistics engine dialog shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    /// Rscript found (or set), if any.
    pub rscript: Option<String>,
    /// Whether the path comes from Otto's settings.
    pub configured: bool,
    pub r_version: Option<String>,
    pub library: String,
    /// Package name and installed version ("" = missing).
    pub packages: Vec<(String, String)>,
    /// Problem running R, if any.
    pub error: Option<String>,
}

impl Status {
    pub fn missing(&self) -> Vec<&str> {
        self.packages.iter().filter(|(p, v)| v.is_empty() && PACKAGES.contains(&p.as_str())).map(|(p, _)| p.as_str()).collect()
    }

    /// R found and every package the worker needs installed.
    pub fn ready(&self) -> bool {
        self.r_version.is_some() && self.missing().is_empty()
    }

    pub fn summary(&self) -> String {
        match (&self.r_version, self.missing()) {
            (None, _) => "R is not installed".into(),
            (Some(v), m) if m.is_empty() => format!("R {v}, packages installed"),
            (Some(v), m) => format!("R {v}, packages missing: {}", m.join(", ")),
        }
    }
}

/// Checks R and Otto's packages (runs R briefly; takes about a second).
pub fn status() -> Status {
    let mut s = Status { library: library_dir().display().to_string(), configured: locate::configured().is_some(), ..Default::default() };
    let Some(rscript) = locate::rscript() else {
        if let Some(p) = locate::configured() {
            s.error = Some(format!("no Rscript at {} (the R path in Otto's settings)", p.display()));
        }
        return s;
    };
    s.rscript = Some(rscript.display().to_string());
    let dir = match write_sources() {
        Ok(d) => d,
        Err(e) => {
            s.error = Some(e.to_string());
            return s;
        }
    };
    let all: Vec<&str> = PACKAGES.iter().chain(CHECK_PACKAGES).copied().collect();
    match command(&rscript).arg("--vanilla").arg(dir.join("status.R")).arg(library_dir()).args(&all).output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    if k == "R" {
                        s.r_version = Some(v.trim().to_string());
                    } else {
                        s.packages.push((k.to_string(), v.trim().to_string()));
                    }
                }
            }
            if s.r_version.is_none() {
                s.error = Some(format!("R did not run: {}", String::from_utf8_lossy(&out.stderr).trim()));
            }
        }
        Err(e) => s.error = Some(format!("could not run {}: {e}", rscript.display())),
    }
    s
}

/// Progress of the package installation.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Setup {
    pub running: bool,
    /// Output of the installation, newest last.
    pub log: Vec<String>,
    /// Set when it finished: Ok, or what went wrong.
    pub finished: Option<Result<(), String>>,
}

fn setup_state() -> &'static Mutex<Setup> {
    static S: OnceLock<Mutex<Setup>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Setup::default()))
}

/// The installation's progress so far.
pub fn setup_progress() -> Setup {
    setup_state().lock().map(|s| s.clone()).unwrap_or_default()
}

/// Installs Otto's packages into its library in the background (see [`setup_progress`]).
pub fn start_setup() -> Result<(), RError> {
    let rscript = locate::rscript().ok_or(RError::NotInstalled)?;
    {
        let mut s = setup_state().lock().unwrap_or_else(|p| p.into_inner());
        if s.running {
            return Ok(());
        }
        *s = Setup { running: true, log: vec![format!("Using {}", rscript.display())], finished: None };
    }
    std::thread::spawn(move || {
        let push = |line: String| {
            if let Ok(mut s) = setup_state().lock() {
                s.log.push(line);
                if s.log.len() > 500 {
                    s.log.remove(0);
                }
            }
        };
        let result = (|| -> Result<(), String> {
            let dir = write_sources().map_err(|e| e.to_string())?;
            let all: Vec<&str> = PACKAGES.iter().chain(CHECK_PACKAGES).copied().collect();
            let mut child = command(&rscript)
                .arg("--vanilla")
                .arg(dir.join("packages.R"))
                .arg(library_dir())
                .arg(REPOSITORY)
                .args(&all)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            let err = child.stderr.take().map(|e| {
                std::thread::spawn(move || {
                    for line in BufReader::new(e).lines().map_while(Result::ok) {
                        if let Ok(mut s) = setup_state().lock() {
                            s.log.push(line);
                        }
                    }
                })
            });
            let mut done = false;
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    done |= line.starts_with("@@otto-setup-done");
                    if !line.starts_with("@@otto") {
                        push(line);
                    } else if let Some(m) = line.strip_prefix("@@otto-setup-failed ") {
                        push(format!("Not installed: {m}"));
                    }
                }
            }
            let status = child.wait().map_err(|e| e.to_string())?;
            if let Some(h) = err {
                let _ = h.join();
            }
            if done && status.success() {
                Ok(())
            } else {
                Err("the installation did not finish; see the log".into())
            }
        })();
        restart();
        let mut s = setup_state().lock().unwrap_or_else(|p| p.into_inner());
        s.log.push(match &result {
            Ok(()) => "Done: the statistics engine is ready.".to_string(),
            Err(e) => format!("Failed: {e}"),
        });
        s.running = false;
        s.finished = Some(result);
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Recording requests and R scripts
// ---------------------------------------------------------------------------

thread_local! {
    static RECORDING: std::cell::RefCell<Option<Vec<Request>>> = const { std::cell::RefCell::new(None) };
}

fn record(req: &Request) {
    RECORDING.with(|r| {
        if let Some(list) = r.borrow_mut().as_mut() {
            list.push(req.clone());
        }
    });
}

/// Runs `f` and returns what it sent to R alongside its result (for "Export R script").
pub fn recording<T>(f: impl FnOnce() -> T) -> (T, Vec<Request>) {
    RECORDING.with(|r| *r.borrow_mut() = Some(Vec::new()));
    let out = f();
    let reqs = RECORDING.with(|r| r.borrow_mut().take()).unwrap_or_default();
    (out, reqs)
}

/// A standalone R script that reruns `requests` with exactly Otto's R code and prints
/// the results, so a reviewer can reproduce every number.
pub fn script(title: &str, requests: &[Request]) -> String {
    let mut s = String::new();
    s.push_str(&format!("# {title}\n"));
    s.push_str("# Reproduces Otto's statistics in R: the functions below are exactly the code Otto runs,\n");
    s.push_str("# and the data are the values Otto analysed.\n");
    s.push_str(&format!("# Packages: {} (Otto installs them from {REPOSITORY}).\n", PACKAGES.join(", ")));
    let lib = library_dir().display().to_string().replace('\\', "/");
    s.push_str(&format!("# Otto's package library; remove this line to use your own packages.\nif (dir.exists(\"{lib}\")) .libPaths(c(\"{lib}\", .libPaths()))\n\n"));
    for (name, text) in SOURCES.iter().filter(|(n, _)| !matches!(*n, "worker.R" | "status.R" | "packages.R")) {
        s.push_str(&format!("# ---- {name} ----\n{text}\n"));
    }
    s.push_str("# ---- the analysis ----\n");
    for (i, r) in requests.iter().enumerate() {
        let body = serde_json::to_string(r).unwrap_or_default().replace('\\', "\\\\").replace('\'', "\\'");
        s.push_str(&format!("request{} <- parse_request('{body}')\n", i + 1));
        s.push_str(&format!("result{0} <- dispatch(request{0}$op, request{0}$data, request{0}$options)\n", i + 1));
        s.push_str(&format!("str(result{}, digits.d = 10)\n", i + 1));
    }
    s.push_str("cat(\"\\nComputed with\", engine_info(), \"\\n\")\n");
    s
}

/// Reads a number that R may report as null (not finite) as NaN.
pub mod de {
    use serde::{Deserialize, Deserializer};

    pub fn num<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        Ok(Option::<f64>::deserialize(d)?.unwrap_or(f64::NAN))
    }

    pub fn nums<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<f64>, D::Error> {
        Ok(Option::<Vec<Option<f64>>>::deserialize(d)?.unwrap_or_default().into_iter().map(|v| v.unwrap_or(f64::NAN)).collect())
    }

    /// A [low, high] interval; missing or incomplete gives `None`.
    pub fn interval<'de, D: Deserializer<'de>>(d: D) -> Result<Option<(f64, f64)>, D::Error> {
        let v = Option::<Vec<Option<f64>>>::deserialize(d)?;
        Ok(match v.as_deref() {
            Some([Some(a), Some(b)]) => Some((*a, *b)),
            _ => None,
        })
    }

    pub fn nan() -> f64 {
        f64::NAN
    }
}

/// Whether R and the packages are ready (starts the worker if needed).
pub fn available() -> bool {
    call(&Request::new("ping", Value::Null, Value::Null)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_embeds_code_and_data() {
        let s = script("t test", &[Request::new("t_test", json!({"a": [1, 2], "b": [3, 4]}), json!({"test": "unpaired"}))]);
        assert!(s.contains("op_t_test <- function"));
        assert!(s.contains(r#"parse_request('{"op":"t_test""#));
        assert!(!s.contains("readLines(input"));
    }

    #[test]
    fn numbers_from_r() {
        #[derive(Deserialize)]
        struct T {
            #[serde(deserialize_with = "de::num")]
            p: f64,
            #[serde(default, deserialize_with = "de::interval")]
            ci: Option<(f64, f64)>,
        }
        let t: T = serde_json::from_value(json!({"p": null, "ci": [1.0, null]})).unwrap();
        assert!(t.p.is_nan() && t.ci.is_none());
        let t: T = serde_json::from_value(json!({"p": 0.5})).unwrap();
        assert_eq!((t.p, t.ci), (0.5, None));
    }
}
