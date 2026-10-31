//! TPrompt за Windows: обвивка около www/index.html.
//!
//! Команди, които страницата вика през `window.__TAURI__.core.invoke`:
//!  - запазване на файл, втори прозорец за четене (втори монитор), брой монитори;
//!  - малък HTTP сървър за дистанционно управление (приложението на телефона и уеб страница);
//!  - старт с Windows.

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::{AppHandle, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartExt};
use tauri_plugin_dialog::DialogExt;

const REMOTE_HTML: &str = include_str!("remote.html");
const FIRST_PORT: u16 = 8723;
const PORT_TRIES: u16 = 20;
const MAX_QUEUE: usize = 500;

#[derive(Default)]
struct Inner {
    token: String,
    port: u16,
    /// състояние от главния прозорец (за уеб дистанционното)
    state_json: String,
    /// текст и стил (за уеб дистанционното)
    content_json: String,
    /// команди от уеб дистанционното
    cmds: Vec<String>,
    /// съобщения от приложението на телефона
    rx_msgs: Vec<String>,
    rx_seq: u64,
    rx_st: Value,
    /// заявка „покажи на телефона“
    wseq: u64,
    want: String,
    /// съобщения към телефона, когато той чете на цял екран
    tx: Vec<String>,
    last_rx: Option<Instant>,
    last_web: Option<Instant>,
}

#[derive(Default)]
struct Remote {
    inner: Arc<Mutex<Inner>>,
    server: Mutex<Option<Arc<tiny_http::Server>>>,
}

fn lock(m: &Arc<Mutex<Inner>>) -> std::sync::MutexGuard<'_, Inner> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn ago_ms(t: &Option<Instant>) -> Value {
    match t {
        Some(i) => json!(i.elapsed().as_millis() as u64),
        None => Value::Null,
    }
}

fn gen_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    const AB: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut out = String::with_capacity(16);
    for i in 0..16usize {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(nanos);
        h.write_usize(i);
        let v = h.finish();
        out.push(AB[(v % AB.len() as u64) as usize] as char);
    }
    out
}

fn token_ok(t: &str) -> bool {
    (6..=64).contains(&t.len())
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// адресът на компютъра в локалната мрежа (без да се праща нищо)
fn lan_ip() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn query_param(q: &str, key: &str) -> Option<String> {
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        if k == key {
            Some(v.to_string())
        } else {
            None
        }
    })
}

fn respond(rq: tiny_http::Request, code: u16, mime: &str, body: &str) {
    let hdr = |k: &str, v: &str| tiny_http::Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap();
    let resp = tiny_http::Response::from_string(body)
        .with_status_code(code)
        .with_header(hdr("Content-Type", mime))
        .with_header(hdr("Cache-Control", "no-store"))
        .with_header(hdr("Access-Control-Allow-Origin", "*"))
        .with_header(hdr("Access-Control-Allow-Methods", "GET, POST, OPTIONS"))
        .with_header(hdr("Access-Control-Allow-Headers", "Content-Type"))
        .with_header(hdr("Access-Control-Allow-Private-Network", "true"));
    let _ = rq.respond(resp);
}

fn handle(mut rq: tiny_http::Request, inner: Arc<Mutex<Inner>>) {
    let url = rq.url().to_string();
    let (path, query) = match url.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (url.clone(), String::new()),
    };
    let method = rq.method().clone();
    if method == tiny_http::Method::Options {
        respond(rq, 204, "text/plain", "");
        return;
    }
    let tok = query_param(&query, "t");
    let ok = {
        let g = lock(&inner);
        !g.token.is_empty() && tok.as_deref() == Some(g.token.as_str())
    };
    if !ok {
        respond(rq, 403, "application/json", "{\"error\":\"forbidden\"}");
        return;
    }
    let mut body = String::new();
    if method == tiny_http::Method::Post {
        let mut buf = Vec::new();
        let _ = rq.as_reader().take(8_000_000).read_to_end(&mut buf);
        body = String::from_utf8_lossy(&buf).into_owned();
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    match (method == tiny_http::Method::Post, path.as_str()) {
        (false, "/") => {
            lock(&inner).last_web = Some(Instant::now());
            respond(rq, 200, "text/html; charset=utf-8", REMOTE_HTML);
        }
        (false, "/api/rx/state") => {
            let want_tx = query_param(&query, "tx").as_deref() == Some("1");
            let out = {
                let mut g = lock(&inner);
                g.last_rx = Some(Instant::now());
                let tx: Vec<Value> = if want_tx {
                    g.tx.drain(..)
                        .filter_map(|s| serde_json::from_str::<Value>(&s).ok())
                        .collect()
                } else {
                    Vec::new()
                };
                json!({
                    "now": now_ms,
                    "st": { "seq": g.rx_seq, "st": g.rx_st },
                    "wseq": g.wseq,
                    "want": g.want,
                    "tx": tx
                })
            };
            respond(rq, 200, "application/json", &out.to_string());
        }
        (true, "/api/rx/send") => {
            {
                let mut g = lock(&inner);
                g.last_rx = Some(Instant::now());
                if g.rx_msgs.len() < MAX_QUEUE {
                    g.rx_msgs.push(body);
                }
            }
            respond(rq, 200, "application/json", "{\"ok\":true}");
        }
        (false, "/api/state") => {
            let s = {
                let mut g = lock(&inner);
                g.last_web = Some(Instant::now());
                if g.state_json.is_empty() {
                    "{}".to_string()
                } else {
                    g.state_json.clone()
                }
            };
            respond(rq, 200, "application/json", &s);
        }
        (false, "/api/content") => {
            let s = {
                let mut g = lock(&inner);
                g.last_web = Some(Instant::now());
                if g.content_json.is_empty() {
                    "{\"rev\":0,\"html\":\"\",\"css\":{}}".to_string()
                } else {
                    g.content_json.clone()
                }
            };
            respond(rq, 200, "application/json", &s);
        }
        (true, "/api/cmd") => {
            {
                let mut g = lock(&inner);
                g.last_web = Some(Instant::now());
                if g.cmds.len() < 100 {
                    g.cmds.push(body);
                }
            }
            respond(rq, 200, "application/json", "{\"ok\":true}");
        }
        _ => respond(rq, 404, "application/json", "{\"error\":\"not found\"}"),
    }
}

fn stop_server(remote: &Remote) {
    let srv = remote
        .server
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(s) = srv {
        s.unblock();
    }
}

fn info(inner: &Arc<Mutex<Inner>>) -> Value {
    let g = lock(inner);
    json!({
        "url": format!("http://{}:{}/?t={}", lan_ip(), g.port, g.token),
        "token": g.token,
        "port": g.port
    })
}

// ---------------------------------------------------------------- команди

#[tauri::command]
async fn save_file(app: AppHandle, name: String, data: String) -> Result<bool, String> {
    let ext = name.rsplit('.').next().unwrap_or("txt").to_lowercase();
    let label = if ext == "html" { "HTML" } else { "Текст" };
    let picked = app
        .dialog()
        .file()
        .set_file_name(&name)
        .add_filter(label, &[ext.as_str()])
        .blocking_save_file();
    match picked {
        Some(p) => {
            let path = p.into_path().map_err(|e| e.to_string())?;
            std::fs::write(path, data.as_bytes()).map_err(|e| e.to_string())?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[tauri::command]
async fn monitor_count(app: AppHandle) -> usize {
    app.available_monitors()
        .map(|m| m.len())
        .unwrap_or(1)
        .max(1)
}

#[tauri::command]
async fn open_reader(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("reader") {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let monitors = app.available_monitors().unwrap_or_default();
    let current = app
        .get_webview_window("main")
        .and_then(|w| w.current_monitor().ok().flatten());
    let other = monitors.iter().find(|m| match &current {
        Some(c) => m.position() != c.position(),
        None => false,
    });
    let w = WebviewWindowBuilder::new(&app, "reader", WebviewUrl::App("index.html#view".into()))
        .title("TPrompt")
        .inner_size(1280.0, 720.0)
        .initialization_script("window.__TP_VIEWER = true;")
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(m) = other {
        let _ = w.set_position(PhysicalPosition::new(m.position().x, m.position().y));
        let _ = w.set_fullscreen(true);
    }
    Ok(())
}

#[tauri::command]
async fn close_reader(app: AppHandle) {
    if let Some(w) = app.get_webview_window("reader") {
        let _ = w.close();
    }
}

#[tauri::command]
async fn reader_open(app: AppHandle) -> bool {
    app.get_webview_window("reader").is_some()
}

#[tauri::command]
fn remote_start(remote: State<'_, Remote>, token: Option<String>) -> Result<Value, String> {
    stop_server(&remote);
    let token = match token {
        Some(t) if token_ok(&t) => t,
        _ => gen_token(),
    };
    let mut bound: Option<(tiny_http::Server, u16)> = None;
    for p in FIRST_PORT..FIRST_PORT + PORT_TRIES {
        if let Ok(s) = tiny_http::Server::http(("0.0.0.0", p)) {
            bound = Some((s, p));
            break;
        }
    }
    let (server, port) = bound.ok_or_else(|| {
        "Не успях да отворя порт за дистанционното. Проверете защитната стена.".to_string()
    })?;
    {
        let mut g = lock(&remote.inner);
        g.token = token;
        g.port = port;
        g.cmds.clear();
        g.rx_msgs.clear();
        g.tx.clear();
        g.rx_st = Value::Null;
        g.rx_seq = 0;
        g.last_rx = None;
        g.last_web = None;
    }
    let server = Arc::new(server);
    *remote.server.lock().unwrap_or_else(|e| e.into_inner()) = Some(server.clone());
    let inner = remote.inner.clone();
    std::thread::spawn(move || {
        for rq in server.incoming_requests() {
            let inner = inner.clone();
            std::thread::spawn(move || handle(rq, inner));
        }
    });
    Ok(info(&remote.inner))
}

#[tauri::command]
fn remote_stop(remote: State<'_, Remote>) {
    stop_server(&remote);
    let mut g = lock(&remote.inner);
    g.token.clear();
    g.cmds.clear();
    g.rx_msgs.clear();
    g.tx.clear();
}

#[tauri::command]
fn remote_new_token(remote: State<'_, Remote>) -> Value {
    {
        let mut g = lock(&remote.inner);
        g.token = gen_token();
    }
    info(&remote.inner)
}

#[tauri::command]
fn remote_set_content(remote: State<'_, Remote>, content_json: String) {
    lock(&remote.inner).content_json = content_json;
}

/// главният прозорец праща състоянието си и получава командите от уеб дистанционното
#[tauri::command]
fn remote_sync(remote: State<'_, Remote>, state_json: String) -> Vec<String> {
    let mut g = lock(&remote.inner);
    g.state_json = state_json;
    g.cmds.drain(..).collect()
}

#[tauri::command]
fn remote_want(remote: State<'_, Remote>, view: bool) {
    let mut g = lock(&remote.inner);
    g.wseq += 1;
    g.want = if view { "view".into() } else { String::new() };
}

#[tauri::command]
fn tx_push(remote: State<'_, Remote>, msg_json: String) {
    let mut g = lock(&remote.inner);
    if g.tx.len() < MAX_QUEUE {
        g.tx.push(msg_json);
    }
}

#[tauri::command]
fn tx_clear(remote: State<'_, Remote>) {
    lock(&remote.inner).tx.clear();
}

/// приема съобщенията на приложението на телефона и подава състоянието на втория прозорец към него
#[tauri::command]
fn rx_pump(remote: State<'_, Remote>, rx_state: String) -> Value {
    let mut g = lock(&remote.inner);
    if !rx_state.is_empty() {
        if let Ok(v) = serde_json::from_str::<Value>(&rx_state) {
            g.rx_seq = v.get("seq").and_then(|s| s.as_u64()).unwrap_or(g.rx_seq);
            g.rx_st = v.get("st").cloned().unwrap_or(Value::Null);
        }
    }
    let msgs: Vec<String> = g.rx_msgs.drain(..).collect();
    json!({
        "msgs": msgs,
        "rxAgo": ago_ms(&g.last_rx),
        "webAgo": ago_ms(&g.last_web)
    })
}

#[tauri::command]
fn autostart_get(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
fn autostart_set(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let al = app.autolaunch();
    let r = if enabled { al.enable() } else { al.disable() };
    r.map(|_| true).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(Remote::default())
        .invoke_handler(tauri::generate_handler![
            save_file,
            monitor_count,
            open_reader,
            close_reader,
            reader_open,
            remote_start,
            remote_stop,
            remote_new_token,
            remote_set_content,
            remote_sync,
            remote_want,
            tx_push,
            tx_clear,
            rx_pump,
            autostart_get,
            autostart_set
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    window.app_handle().exit(0);
                }
            }
        })
        .setup(|app| {
            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.minimize();
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("грешка при стартиране на TPrompt");
}
