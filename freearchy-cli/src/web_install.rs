//! `freearchy install-system --web` — browser-based installer.
//!
//! Runs a tiny HTTP server on the live system; the GUI is a page opened in
//! any browser pointed at http://<host>:<port>. On a VM this means the
//! installer GUI renders on your *macOS* browser — no display stack needed
//! in the guest at all.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use tiny_http::{Header, Method, Response, Server};

use crate::install_system::{self, InstallSpec};
use crate::util;

const PAGE: &str = include_str!("../web/installer.html");
const LOGO: &[u8] = include_bytes!("../assets/logo.png");

struct Shared {
    lines: Mutex<Vec<String>>,
    state: Mutex<InstallState>,
    running: AtomicBool,
}

#[derive(Clone)]
enum InstallState {
    Idle,
    Running,
    Done,
    Failed(String),
}

impl InstallState {
    fn as_json(&self) -> serde_json::Value {
        match self {
            Self::Idle => json!("idle"),
            Self::Running => json!("running"),
            Self::Done => json!("done"),
            Self::Failed(e) => json!({ "failed": e }),
        }
    }
}

#[derive(Deserialize)]
struct InstallRequest {
    disk: String,
    hostname: String,
    username: String,
    password: Option<String>,
}

pub fn serve(port: u16) -> Result<()> {
    let server = Server::http(("0.0.0.0", port))
        .map_err(|e| anyhow::anyhow!("bind port {port}: {e}"))?;
    let shared = Arc::new(Shared {
        lines: Mutex::new(Vec::new()),
        state: Mutex::new(InstallState::Idle),
        running: AtomicBool::new(false),
    });

    println!("freearchy installer UI listening on :{port}");
    println!("open http://<this-machine>:{port} in a browser");

    for request in server.incoming_requests() {
        if let Err(e) = handle(request, &shared) {
            eprintln!("request error: {e:#}");
        }
    }
    Ok(())
}

fn handle(request: tiny_http::Request, shared: &Arc<Shared>) -> Result<()> {
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or("/");

    match (request.method(), path) {
        (Method::Get, "/") => respond(request, PAGE, "text/html; charset=utf-8"),
        (Method::Get, "/logo.png") => respond_bytes(request, LOGO, "image/png"),
        (Method::Get, "/api/disks") => api_disks(request),
        (Method::Post, "/api/install") => api_install(request, shared),
        (Method::Get, "/api/log") => api_log(request, shared, &url),
        _ => {
            let resp = Response::from_string("not found").with_status_code(404);
            request.respond(resp).context("respond")
        }
    }
}

fn respond(request: tiny_http::Request, body: &str, ctype: &str) -> Result<()> {
    let header = Header::from_bytes("Content-Type", ctype).unwrap();
    request
        .respond(Response::from_string(body).with_header(header))
        .context("respond")
}

fn respond_bytes(request: tiny_http::Request, body: &'static [u8], ctype: &str) -> Result<()> {
    let header = Header::from_bytes("Content-Type", ctype).unwrap();
    request
        .respond(Response::from_data(body).with_header(header))
        .context("respond")
}

fn json_respond(request: tiny_http::Request, value: serde_json::Value) -> Result<()> {
    respond(request, &value.to_string(), "application/json")
}

fn api_disks(request: tiny_http::Request) -> Result<()> {
    let disks = install_system::disks()
        .unwrap_or_default()
        .into_iter()
        .map(|(name, label)| json!({ "name": name, "label": label }))
        .collect::<Vec<_>>();
    json_respond(request, json!({ "disks": disks }))
}

fn api_install(mut request: tiny_http::Request, shared: &Arc<Shared>) -> Result<()> {
    // Single-shot: refuse a second run while one is active.
    if shared.running.swap(true, Ordering::SeqCst) {
        return json_respond(
            request,
            json!({ "ok": false, "error": "install already running" }),
        );
    }

    let mut body = Vec::new();
    request.as_reader().read_to_end(&mut body)?;
    let req: InstallRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            shared.running.store(false, Ordering::SeqCst);
            return json_respond(
                request,
                json!({ "ok": false, "error": format!("bad request: {e}") }),
            );
        }
    };

    *shared.state.lock().unwrap() = InstallState::Running;
    let shared2 = Arc::clone(shared);
    thread::spawn(move || {
        let buf = Arc::clone(&shared2);
        let log = move |m: &str| buf.lines.lock().unwrap().push(m.to_string());
        let spec = InstallSpec {
            disk: req.disk,
            hostname: req.hostname,
            username: req.username,
            fullname: None,
            email: None,
            keymap: "us".into(),
            password: req.password.filter(|p| !p.is_empty()),
        };
        let result = install_system::execute(&spec, &log);
        *shared2.state.lock().unwrap() = match result {
            Ok(()) => InstallState::Done,
            Err(e) => InstallState::Failed(format!("{e:#}")),
        };
    });

    json_respond(request, json!({ "ok": true }))
}

fn api_log(request: tiny_http::Request, shared: &Arc<Shared>, url: &str) -> Result<()> {
    let from: usize = url
        .split("from=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let (slice, total, state) = {
        let lines = shared.lines.lock().unwrap();
        let slice: Vec<String> = lines.iter().skip(from).cloned().collect();
        let total = lines.len();
        let state = shared.state.lock().unwrap().clone();
        (slice, total, state)
    };

    json_respond(
        request,
        json!({ "lines": slice, "total": total, "state": state.as_json() }),
    )
}

/// Convenience for callers that want the machine's primary IP in the banner.
#[allow(dead_code)]
fn primary_ip() -> Option<String> {
    util::capture("sh", &["-c", "ifconfig | awk '/inet /{print $2; exit}'"])
        .map(|s| s.trim().to_string())
}
