use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use rmcp::service::RunningService;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{RoleClient, ServiceExt};

use otto_engine::Engine;

use crate::backend::{Backend, HybridBackend};
use crate::discovery::{random_token, Discovery};
use crate::server::OttoServer;

/// Preferred local port for the app's MCP endpoint (another free port is used if taken).
pub const DEFAULT_PORT: u16 = 47800;

/// A running HTTP endpoint. Dropping it stops the server and removes the discovery file.
pub struct HttpHandle {
    pub discovery: Discovery,
    stop: Box<dyn Fn() + Send + Sync>,
}

impl HttpHandle {
    pub fn url(&self) -> &str {
        &self.discovery.url
    }
}

impl Drop for HttpHandle {
    fn drop(&mut self) {
        (self.stop)();
        self.discovery.remove_if_ours();
    }
}

/// Serves MCP over streamable HTTP on 127.0.0.1, protected by a bearer token. With
/// `advertise`, the endpoint is written to the discovery file for `otto mcp` to find.
pub async fn serve_http<B: Backend + Clone>(backend: B, port: u16, advertise: bool) -> anyhow::Result<HttpHandle> {
    // One endpoint per computer, always on the same port. Falling back to another port
    // used to look tidier, but Claude finds the app through a single discovery file: a
    // second window would take that file over and Claude would end up driving whichever
    // window happened to start last, while the one in front of the user stayed untouched.
    // A port of 0 asks for any free port (tests, which do not advertise).
    let listener = match tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        Err(e) if port == 0 => return Err(anyhow::Error::new(e).context("binding the MCP port")),
        Err(_) => anyhow::bail!(
            "another Otto window on this computer is already serving Claude MCP (port {port}).              Claude talks to that window; close it, or switch its \"Claude MCP\" off, and switch this one on again."
        ),
    };
    let addr = listener.local_addr()?;
    // Reuse the previous token so a client configured with it keeps working across launches.
    let previous = if advertise { Discovery::read().map(|d| d.token) } else { None };
    let token = previous.filter(|t| t.len() >= 32).unwrap_or_else(random_token);

    // Default config only accepts localhost Host headers (DNS-rebinding protection).
    let config = StreamableHttpServerConfig::default();
    let cancel = config.cancellation_token.clone();
    let service = StreamableHttpService::new(
        move || Ok(OttoServer::new(backend.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    let expected = format!("Bearer {token}");
    let auth = move |req: Request, next: Next| {
        let expected = expected.clone();
        async move {
            let ok = req.headers().get(axum::http::header::AUTHORIZATION).and_then(|v| v.to_str().ok()) == Some(expected.as_str());
            if ok {
                next.run(req).await
            } else {
                let r: Response = (StatusCode::UNAUTHORIZED, "Otto: missing or wrong bearer token (see ~/.otto/mcp.json)").into_response();
                r
            }
        }
    };
    let router = axum::Router::new().nest_service("/mcp", service).layer(axum::middleware::from_fn(auth));
    let shutdown = cancel.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router).with_graceful_shutdown(async move { shutdown.cancelled_owned().await }).await;
    });

    let discovery = Discovery { url: format!("http://{addr}/mcp"), token, pid: std::process::id() };
    if advertise {
        discovery.write().context("writing the MCP discovery file")?;
        // An app killed before it could clean up leaves the file naming an endpoint that
        // is dead or answers 401, and `otto mcp` then reports no window at all with the
        // live one sitting right there. Only one server advertises, so this one owns the
        // file: it writes it back whenever something else has changed it.
        let (mine, stop) = (discovery.clone(), cancel.clone());
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                }
                if Discovery::read().as_ref() != Some(&mine) {
                    let _ = mine.write();
                }
            }
        });
    }
    Ok(HttpHandle { discovery, stop: Box::new(move || cancel.cancel()) })
}

/// Connects to a running Otto app.
pub(crate) async fn connect(d: &Discovery) -> anyhow::Result<RunningService<RoleClient, ()>> {
    let config = StreamableHttpClientTransportConfig::with_uri(d.url.clone()).auth_header(d.token.clone());
    let transport = StreamableHttpClientTransport::from_config(config);
    let client = tokio::time::timeout(Duration::from_secs(3), ().serve(transport)).await.context("timed out")??;
    Ok(client)
}

/// `otto mcp`: MCP over stdio. Bridges to the running app when there is one (so the
/// user sees Claude work live), otherwise runs a headless engine.
pub async fn run_stdio(headless: bool) -> anyhow::Result<()> {
    // Whether there is a window is decided per call, not once here: Claude desktop starts
    // this server when it starts itself, usually well before the user opens Otto, and a
    // server that decided "no window" at that moment stayed headless for good. Until a
    // window answers, calls go to a background engine that can also open one
    // (ui_show_window).
    let backend = if headless {
        tracing::info!("running headless: --headless was given");
        HybridBackend::with_discovery(Engine::new(), || None)
    } else {
        HybridBackend::new(Engine::new())
    };
    let server = OttoServer::new(backend).serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
