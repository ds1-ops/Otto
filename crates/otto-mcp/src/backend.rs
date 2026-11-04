use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use rmcp::service::RunningService;
use rmcp::RoleClient;
use tokio::sync::{mpsc, oneshot};

use otto_engine::{Engine, EngineError, Outcome};

use crate::discovery::Discovery;
use crate::server::outcome_to_result;

/// Something that executes Otto tool calls.
pub trait Backend: Send + Sync + 'static {
    fn call(&self, tool: String, args: serde_json::Value) -> impl Future<Output = CallToolResult> + Send;
}

/// A headless engine owned by the MCP server.
#[derive(Clone)]
pub struct LocalBackend {
    engine: Arc<Mutex<Engine>>,
}

impl LocalBackend {
    pub fn new(engine: Engine) -> Self {
        LocalBackend { engine: Arc::new(Mutex::new(engine)) }
    }

    pub fn engine(&self) -> Arc<Mutex<Engine>> {
        self.engine.clone()
    }
}

impl Backend for LocalBackend {
    async fn call(&self, tool: String, args: serde_json::Value) -> CallToolResult {
        let engine = self.engine.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut e = engine.lock().unwrap_or_else(|p| p.into_inner());
            e.call(&tool, args)
        })
        .await
        .unwrap_or_else(|e| Err(EngineError::Invalid(format!("tool crashed: {e}"))));
        outcome_to_result(result)
    }
}

/// A tool call sent to the GUI thread, which applies it and replies.
#[derive(Debug)]
pub struct McpRequest {
    pub tool: String,
    pub args: serde_json::Value,
    pub reply: oneshot::Sender<Result<Outcome, EngineError>>,
}

/// Forwards tool calls to the running GUI over a channel.
#[derive(Clone)]
pub struct ChannelBackend {
    tx: mpsc::Sender<McpRequest>,
}

impl ChannelBackend {
    pub fn new(tx: mpsc::Sender<McpRequest>) -> Self {
        ChannelBackend { tx }
    }
}

impl Backend for ChannelBackend {
    async fn call(&self, tool: String, args: serde_json::Value) -> CallToolResult {
        let (reply, rx) = oneshot::channel();
        if self.tx.send(McpRequest { tool, args, reply }).await.is_err() {
            return outcome_to_result(Err(EngineError::Invalid("the Otto window was closed".into())));
        }
        outcome_to_result(rx.await.unwrap_or_else(|_| Err(EngineError::Invalid("the Otto window did not answer".into()))))
    }
}

/// What Claude is told while the app doesn't accept MCP calls.
pub const SWITCHED_OFF: &str = "The Otto app is not accepting Claude's calls: \"Claude MCP\" is switched off at the top right of \
     the Otto window (or the window was closed). Ask the user to switch it on, then try again.";

/// Forwards tool calls to a running Otto app's HTTP MCP endpoint. If the app switches
/// Claude access off and on again (new endpoint and session), it reconnects on the next call.
pub struct RemoteBackend {
    client: tokio::sync::RwLock<RunningService<RoleClient, ()>>,
    /// Where the app's current endpoint is advertised (normally the discovery file).
    discover: Box<dyn Fn() -> Option<Discovery> + Send + Sync>,
}

impl RemoteBackend {
    pub fn new(client: RunningService<RoleClient, ()>) -> Self {
        RemoteBackend { client: tokio::sync::RwLock::new(client), discover: Box::new(Discovery::read) }
    }

    /// Connects to the endpoint `discover` returns, and reconnects through it later.
    pub async fn connect_with(discover: impl Fn() -> Option<Discovery> + Send + Sync + 'static) -> anyhow::Result<Self> {
        let d = discover().ok_or_else(|| anyhow::anyhow!("no Otto app endpoint"))?;
        let client = crate::transport::connect(&d).await?;
        Ok(RemoteBackend { client: tokio::sync::RwLock::new(client), discover: Box::new(discover) })
    }
}

impl Backend for RemoteBackend {
    async fn call(&self, tool: String, args: serde_json::Value) -> CallToolResult {
        let params = || {
            let mut p = CallToolRequestParams::new(tool.clone());
            p.arguments = match &args {
                serde_json::Value::Object(o) => Some(o.clone()),
                _ => None,
            };
            p
        };
        let first = self.client.read().await.call_tool(params()).await;
        let error = match first {
            Ok(r) => return r,
            Err(e) => e,
        };
        // The endpoint is gone: reconnect if the app advertises a (new) one, else explain.
        let Some(d) = (self.discover)() else {
            return CallToolResult::error(vec![ContentBlock::text(SWITCHED_OFF)]);
        };
        let mut client = self.client.write().await;
        match crate::transport::connect(&d).await {
            Ok(c) => *client = c,
            Err(_) => return CallToolResult::error(vec![ContentBlock::text(format!("lost connection to the Otto app: {error}"))]),
        }
        match client.call_tool(params()).await {
            Ok(r) => r,
            Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("lost connection to the Otto app: {e}"))]),
        }
    }
}

/// How often a background server looks for a window that has appeared since it started.
const LOOK_FOR_WINDOW_EVERY: Duration = Duration::from_secs(2);

/// What `otto mcp` uses when no Otto window was open at start: a background engine until
/// Claude calls `ui_show_window`, which saves the project, opens an Otto window with it and
/// from then on forwards calls to that window (so the user sees the rest live). It also
/// attaches to a window the user opens later, which is the usual case: Claude desktop
/// starts its servers with itself, often minutes before the user opens Otto.
pub struct HybridBackend {
    local: LocalBackend,
    remote: tokio::sync::RwLock<Option<RemoteBackend>>,
    /// When to look for a window again, and why the last look found none.
    look: Mutex<(Instant, Option<String>)>,
    /// Held while looking, so that calls arriving together all end up in the same place:
    /// Claude sends several at once, and without this the first could attach to the
    /// window while the second was still answering from the background project.
    looking: tokio::sync::Mutex<()>,
    /// Where a window advertises its endpoint (the discovery file, or a stand-in in tests).
    discover: Arc<dyn Fn() -> Option<Discovery> + Send + Sync>,
}

impl HybridBackend {
    pub fn new(engine: Engine) -> Self {
        Self::with_discovery(engine, Discovery::read)
    }

    /// As [`HybridBackend::new`], looking for the window through `discover`.
    pub fn with_discovery(engine: Engine, discover: impl Fn() -> Option<Discovery> + Send + Sync + 'static) -> Self {
        HybridBackend {
            local: LocalBackend::new(engine),
            remote: tokio::sync::RwLock::new(None),
            look: Mutex::new((Instant::now(), None)),
            looking: tokio::sync::Mutex::new(()),
            discover: Arc::new(discover),
        }
    }

    /// Attaches to a window that is advertising an endpoint, so one opened after this
    /// server started is still picked up. Only while nothing has been put in the
    /// background project: switching after Claude has built tables here would leave that
    /// work behind in a copy nobody can see.
    async fn attach_to_window(&self) -> bool {
        let _one_at_a_time = self.looking.lock().await;
        if self.remote.read().await.is_some() {
            return true;
        }
        {
            let mut look = self.look.lock().unwrap_or_else(|p| p.into_inner());
            if Instant::now() < look.0 {
                return false;
            }
            look.0 = Instant::now() + LOOK_FOR_WINDOW_EVERY;
        }
        if !self.background_is_empty() {
            return false;
        }
        if (self.discover)().is_none() {
            self.why(Some("no Otto window is advertising an endpoint".into()));
            return false;
        }
        let discover = self.discover.clone();
        match RemoteBackend::connect_with(move || discover()).await {
            Ok(remote) => {
                *self.remote.write().await = Some(remote);
                self.why(None);
                true
            }
            Err(e) => {
                self.why(Some(format!("an Otto window is advertised but did not answer: {e}")));
                false
            }
        }
    }

    /// Nothing done in the background project yet, so attaching to a window loses nothing.
    fn background_is_empty(&self) -> bool {
        let engine = self.local.engine();
        let e = engine.lock().unwrap_or_else(|p| p.into_inner());
        e.path.is_none() && e.project.tables.is_empty() && e.project.graphs.is_empty() && e.project.analyses.is_empty()
    }

    fn why(&self, reason: Option<String>) {
        self.look.lock().unwrap_or_else(|p| p.into_inner()).1 = reason;
    }

    /// Says that this project is a background copy, not the window the user is looking at.
    /// Without it a status reads as an empty project and the window looks broken.
    fn background_note(&self) -> String {
        let why = self.look.lock().unwrap_or_else(|p| p.into_inner()).1.clone();
        let why = why.unwrap_or_else(|| "no Otto window is advertising an endpoint".into());
        format!(
            "Note: this is Otto's own background project, not the open Otto window - {why}. \
             Anything in that window (its tables, graphs and results) is not visible here and is not changed by these calls. \
             To work in it instead, ask the user to switch \"Claude MCP\" on at its top right; it is picked up within seconds. \
             ui_show_window opens a window for this background project."
        )
    }

    /// Saves the background project and opens it in an Otto window; returns what happened.
    async fn show_window(&self) -> CallToolResult {
        let engine = self.local.engine();
        let saved = tokio::task::spawn_blocking(move || -> Result<std::path::PathBuf, EngineError> {
            let mut e = engine.lock().unwrap_or_else(|p| p.into_inner());
            match e.path.clone() {
                Some(path) => {
                    e.call("project_save", serde_json::json!({}))?;
                    Ok(path)
                }
                None => {
                    let path = free_path(&e.project.name);
                    e.call("project_save_as", serde_json::json!({ "path": path.display().to_string() }))?;
                    Ok(e.path.clone().unwrap_or(path))
                }
            }
        })
        .await
        .unwrap_or_else(|e| Err(EngineError::Invalid(format!("saving crashed: {e}"))));
        let path = match saved {
            Ok(p) => p,
            Err(e) => return outcome_to_result(Err(e)),
        };
        let pid = match open_window(&path) {
            Ok(pid) => pid,
            Err(e) => return outcome_to_result(Err(EngineError::Io(format!("could not start the Otto window: {e}")))),
        };
        // Wait for the window's MCP endpoint, then continue there.
        for _ in 0..60 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if Discovery::read().is_some_and(|d| d.pid == pid) {
                if let Ok(remote) = RemoteBackend::connect_with(move || Discovery::read().filter(|d| d.pid == pid)).await {
                    *self.remote.write().await = Some(remote);
                    return outcome_to_result(Ok(Outcome::msg(format!(
                        "Opened the Otto window with the project ({}). From now on every change appears there live.",
                        path.display()
                    ))));
                }
            }
        }
        outcome_to_result(Ok(Outcome::msg(format!(
            "Opened the Otto window with the project ({}). \"Claude MCP\" is switched off in it, so further changes \
             stay in this background copy; ask the user to switch it on (top right) and call ui_show_window again.",
            path.display()
        ))))
    }
}

impl Backend for HybridBackend {
    async fn call(&self, tool: String, args: serde_json::Value) -> CallToolResult {
        if self.remote.read().await.is_none() && tool != "ui_show_window" {
            self.attach_to_window().await;
        }
        if let Some(remote) = self.remote.read().await.as_ref() {
            return remote.call(tool, args).await;
        }
        if tool == "ui_show_window" {
            return self.show_window().await;
        }
        // The two tools Claude reads this project's state from: without a note, an empty
        // background project reads as the window's, and the window looks broken.
        let status = matches!(tool.as_str(), "project_summary" | "engine_status");
        let mut result = self.local.call(tool, args).await;
        if status && result.is_error != Some(true) {
            result.content.push(ContentBlock::text(self.background_note()));
        }
        result
    }
}

/// `Documents\Otto\<name>.otto`, numbered if that file exists (never overwrites).
fn free_path(name: &str) -> std::path::PathBuf {
    let dir = dirs::document_dir().unwrap_or_else(std::env::temp_dir).join("Otto");
    let _ = std::fs::create_dir_all(&dir);
    let clean: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) { '_' } else { c }).collect();
    let clean = if clean.trim().is_empty() { "Untitled".to_string() } else { clean.trim().to_string() };
    (1..)
        .map(|i| dir.join(if i == 1 { format!("{clean}.otto") } else { format!("{clean} ({i}).otto") }))
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join("Untitled.otto"))
}

/// Starts this executable as the Otto app (a visible window) with `project`, detached from
/// the MCP process so it stays open after Claude disconnects.
fn open_window(project: &std::path::Path) -> std::io::Result<u32> {
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg(project).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        // Leave Claude's job object if allowed, so closing Claude doesn't close the window.
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB);
        if let Ok(child) = cmd.spawn() {
            return Ok(child.id());
        }
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    Ok(cmd.spawn()?.id())
}
