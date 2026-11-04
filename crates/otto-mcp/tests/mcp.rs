//! End-to-end MCP tests: a real rmcp client talks to Otto's server.

use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::ServiceExt;
use serde_json::json;

use otto_engine::{registry, Engine};
use otto_mcp::{serve_http, LocalBackend, OttoServer};

fn sample_path() -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/data.xlsx").display().to_string()
}

fn params(name: &str, args: serde_json::Value) -> CallToolRequestParams {
    let mut p = CallToolRequestParams::new(name.to_string());
    p.arguments = args.as_object().cloned();
    p
}

fn first_text(r: &CallToolResult) -> String {
    r.content
        .iter()
        .find_map(|c| match c {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Every text block of a result, joined: a background server adds a note after the answer.
fn all_text(r: &CallToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("
")
}

#[tokio::test]
async fn every_command_is_a_tool_and_a_full_workflow_works() {
    let (server_io, client_io) = tokio::io::duplex(1 << 22);
    let server = OttoServer::new(LocalBackend::new(Engine::new()));
    tokio::spawn(async move {
        let running = server.serve(server_io).await.unwrap();
        let _ = running.waiting().await;
    });
    let client = ().serve(client_io).await.unwrap();

    // Parity: the tool list is exactly the command registry.
    let tools = client.list_all_tools().await.unwrap();
    let mut names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    let mut expected: Vec<String> = registry().iter().map(|m| m.name.to_string()).collect();
    names.sort();
    expected.sort();
    assert_eq!(names, expected);
    assert!(tools.iter().all(|t| t.input_schema.get("type").and_then(|v| v.as_str()) == Some("object")));

    let r = client.call_tool(params("table_import_file", json!({ "path": sample_path() }))).await.unwrap();
    assert_ne!(r.is_error, Some(true), "{}", first_text(&r));
    assert!(first_text(&r).contains("24 rows"));

    let r = client
        .call_tool(params("graph_create", json!({ "kind": "grouped_interleaved", "x": "Condition", "y": ["IL2"], "group": "Additive" })))
        .await
        .unwrap();
    assert_ne!(r.is_error, Some(true), "{}", first_text(&r));

    let r = client.call_tool(params("graph_render_preview", json!({ "dpi": 150 }))).await.unwrap();
    let image = r.content.iter().find_map(|c| match c {
        ContentBlock::Image(i) => Some(i.clone()),
        _ => None,
    });
    let image = image.expect("preview returns an image");
    assert_eq!(image.mime_type, "image/png");
    assert!(image.data.len() > 1000);

    let dir = std::env::temp_dir().join(format!("otto-mcp-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let svg = dir.join("graph.svg");
    let r = client.call_tool(params("graph_export_file", json!({ "path": svg.display().to_string() }))).await.unwrap();
    assert_ne!(r.is_error, Some(true), "{}", first_text(&r));
    assert!(std::fs::read_to_string(&svg).unwrap().contains("<svg"));
    let _ = std::fs::remove_dir_all(&dir);

    // Mistakes come back as readable tool errors, not protocol failures.
    let r = client.call_tool(params("graph_set_mapping", json!({ "x": "Nope" }))).await.unwrap();
    assert_eq!(r.is_error, Some(true));
    assert!(first_text(&r).contains("Nope"));
    let r = client.call_tool(params("graph_set_format", json!({ "format": "giant" }))).await.unwrap();
    assert_eq!(r.is_error, Some(true));

    client.cancel().await.unwrap();
}

#[tokio::test]
async fn http_endpoint_requires_the_token() {
    let handle = serve_http(LocalBackend::new(Engine::new()), 0, false).await.unwrap();
    let url = handle.url().to_string();

    // Without the token the server refuses.
    let anonymous = StreamableHttpClientTransport::from_uri(url.clone());
    assert!(().serve(anonymous).await.is_err());

    // With the token it works.
    let config = StreamableHttpClientTransportConfig::with_uri(url).auth_header(handle.discovery.token.clone());
    let client = ().serve(StreamableHttpClientTransport::from_config(config)).await.unwrap();
    let r = client.call_tool(params("project_summary", json!({}))).await.unwrap();
    assert!(first_text(&r).contains("0 tables"));
    client.cancel().await.unwrap();
}

/// The `otto mcp` bridge while the user switches Claude MCP off and on in the app.
#[tokio::test]
async fn bridge_follows_the_app_switching_mcp_off_and_on() {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use otto_mcp::{Backend, Discovery, RemoteBackend};

    let app = LocalBackend::new(Engine::new());
    let first = serve_http(app.clone(), 0, false).await.unwrap();
    // Stands in for the discovery file (the real one belongs to the user's running app).
    let advertised: Arc<Mutex<Option<Discovery>>> = Arc::new(Mutex::new(Some(first.discovery.clone())));
    let source = advertised.clone();
    let bridge = RemoteBackend::connect_with(move || source.lock().unwrap().clone()).await.unwrap();
    let call = || async {
        tokio::time::timeout(Duration::from_secs(10), bridge.call("project_summary".into(), json!({}))).await.expect("call hung")
    };
    assert_ne!(call().await.is_error, Some(true));

    // Off: the server stops and the endpoint is no longer advertised.
    drop(first);
    *advertised.lock().unwrap() = None;
    let r = call().await;
    assert_eq!(r.is_error, Some(true));
    assert!(first_text(&r).contains("switched off"), "{}", first_text(&r));

    // On again: a new endpoint and token; the bridge reconnects on its own.
    let second = serve_http(app.clone(), 0, false).await.unwrap();
    *advertised.lock().unwrap() = Some(second.discovery.clone());
    let r = call().await;
    assert_ne!(r.is_error, Some(true), "{}", first_text(&r));
    assert!(first_text(&r).contains("0 tables"));
}

/// Claude desktop starts its MCP servers with itself, so the user usually opens Otto
/// minutes later. The background server has to pick that window up by itself.
#[tokio::test]
async fn a_window_opened_later_is_picked_up() {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use otto_mcp::{Backend, Discovery, HybridBackend};

    // Stands in for the discovery file: nothing is advertised yet.
    let advertised: Arc<Mutex<Option<Discovery>>> = Arc::new(Mutex::new(None));
    let source = advertised.clone();
    let bridge = HybridBackend::with_discovery(Engine::new(), move || source.lock().unwrap().clone());
    let call = |tool: &'static str| {
        let bridge = &bridge;
        async move { tokio::time::timeout(Duration::from_secs(10), bridge.call(tool.into(), json!({}))).await.expect("call hung") }
    };

    // No window: the background project answers, and says so rather than passing itself
    // off as the window the user is looking at.
    let r = call("project_summary").await;
    assert_ne!(r.is_error, Some(true));
    let text = all_text(&r);
    assert!(text.contains("0 tables"), "{text}");
    assert!(text.contains("background project"), "{text}");

    // The user opens Otto with a table in it and switches Claude MCP on.
    let app = LocalBackend::new(Engine::new());
    app.call("table_create".into(), json!({ "name": "Opened later" })).await;
    let window = serve_http(app, 0, false).await.unwrap();
    *advertised.lock().unwrap() = Some(window.discovery.clone());

    // The next call goes to the window, without restarting anything.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let text = all_text(&call("project_summary").await);
    assert!(text.contains("1 tables"), "still on the background project: {text}");
    assert!(!text.contains("background project"), "{text}");
}

/// Two windows must not both claim Claude: the discovery file names one endpoint, so the
/// second window to start would take it over and Claude would drive the wrong window.
#[tokio::test]
async fn a_second_window_does_not_take_claude_over() {
    // Port 0 is "any free port" (what the other tests use); a real port is exclusive.
    let port = {
        let probe = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        probe.local_addr().unwrap().port()
    };
    let first = serve_http(LocalBackend::new(Engine::new()), port, false).await.unwrap();
    assert!(first.url().contains(&port.to_string()));

    let second = serve_http(LocalBackend::new(Engine::new()), port, false).await;
    let e = second.err().expect("the second window must not start a server").to_string();
    assert!(e.contains("another Otto window"), "{e}");
}
