//! Localhost OBS Browser Source HTTP + Server-Sent Events (SSE) server.
//!
//! Zero external crates, disabled by default (`obs_enabled: false`) with zero open ports,
//! zero threads, and a 1-nanosecond atomic fast-path when inactive or when no OBS client
//! is connected.

use crate::cats::all_cats;
use crate::AppState;
use serde::Serialize;
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const SSE_CLIENT_QUEUE_CAP: usize = 256;

struct ObsServerControl {
    port: u16,
    running: Arc<AtomicBool>,
}

static OBS_CONTROL: OnceLock<Mutex<Option<ObsServerControl>>> = OnceLock::new();
static OBS_CLIENTS: OnceLock<Mutex<Vec<mpsc::SyncSender<String>>>> = OnceLock::new();
static OBS_CLIENT_COUNT: AtomicUsize = AtomicUsize::new(0);
static OBS_PRIVACY_ACTIVE: AtomicBool = AtomicBool::new(false);
static OBS_GAME_ACTIVE: AtomicBool = AtomicBool::new(false);

fn control_slot() -> &'static Mutex<Option<ObsServerControl>> {
    OBS_CONTROL.get_or_init(|| Mutex::new(None))
}

fn clients_slot() -> &'static Mutex<Vec<mpsc::SyncSender<String>>> {
    OBS_CLIENTS.get_or_init(|| Mutex::new(Vec::new()))
}

struct ClientGuard;

impl Drop for ClientGuard {
    fn drop(&mut self) {
        OBS_CLIENT_COUNT.fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn obs_url(port: u16) -> String {
    let p = if port == 0 { 27421 } else { port };
    format!("http://127.0.0.1:{}/obs", p)
}

pub fn set_privacy_active(active: bool) {
    OBS_PRIVACY_ACTIVE.store(active, Ordering::Relaxed);
    broadcast_raw("privacy", if active { "true" } else { "false" });
}

pub fn set_game_active(active: bool, payload_json: &str) {
    OBS_GAME_ACTIVE.store(active, Ordering::Relaxed);
    broadcast_raw("gameActive", payload_json);
}

/// Non-blocking 1ns fast-path broadcast for high-frequency keyboard/mouse events.
pub fn broadcast_input<T: Serialize>(payload: &T) {
    if OBS_CLIENT_COUNT.load(Ordering::Relaxed) == 0 {
        return;
    }
    if let Ok(json_str) = serde_json::to_string(payload) {
        broadcast_raw("input", &json_str);
    }
}

/// Non-blocking broadcast for settings, cats, privacy, or arbitrary JSON payloads.
pub fn broadcast_raw(event_name: &str, json_data: &str) {
    if OBS_CLIENT_COUNT.load(Ordering::Relaxed) == 0 {
        return;
    }
    let frame = format_sse_frame(event_name, json_data);
    if let Ok(mut guard) = clients_slot().lock() {
        guard.retain(|tx| match tx.try_send(frame.clone()) {
            Ok(()) => true,
            // Drop oldest overflow safely without ever blocking input hooks
            Err(mpsc::TrySendError::Full(_)) => true,
            Err(mpsc::TrySendError::Disconnected(_)) => false,
        });
    }
}

fn format_sse_frame(event_name: &str, json_data: &str) -> String {
    let clean = json_data.replace('\r', "").replace('\n', " ");
    format!("event: {}\ndata: {}\n\n", event_name, clean)
}

fn build_state_snapshot_json(app: &AppHandle) -> String {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let cats = all_cats();
    let privacy = OBS_PRIVACY_ACTIVE.load(Ordering::Relaxed);
    let game_active = OBS_GAME_ACTIVE.load(Ordering::Relaxed);
    serde_json::to_string(&json!({
        "settings": settings,
        "cats": cats,
        "privacy": privacy,
        "gameActive": game_active,
    }))
    .unwrap_or_else(|_| "{}".to_string())
}

fn stop_locked(slot: &mut Option<ObsServerControl>) {
    if let Some(ctrl) = slot.take() {
        ctrl.running.store(false, Ordering::SeqCst);
        // Disconnect all active SSE clients immediately
        if let Ok(mut guard) = clients_slot().lock() {
            guard.clear();
        }
        // Wake the blocking TcpListener::accept() call so the port releases immediately
        let _ = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", ctrl.port).parse().unwrap(),
            Duration::from_millis(150),
        );
    }
}

/// Starts, stops, or restarts the local OBS Browser Source server to match `enabled` & `port`.
pub fn sync_obs_server(app: &AppHandle, enabled: bool, port: u16) {
    let actual_port = if port == 0 { 27421 } else { port };
    let mut slot = control_slot().lock().unwrap();

    if !enabled {
        stop_locked(&mut slot);
        return;
    }

    if let Some(existing) = slot.as_ref() {
        if existing.port == actual_port && existing.running.load(Ordering::Relaxed) {
            return;
        }
    }

    stop_locked(&mut slot);

    let running = Arc::new(AtomicBool::new(true));
    let running_thread = running.clone();
    let app_handle = app.clone();

    let Ok(listener) = TcpListener::bind(("127.0.0.1", actual_port)) else {
        eprintln!("[KuroCat OBS] Failed to bind 127.0.0.1:{}", actual_port);
        return;
    };

    *slot = Some(ObsServerControl {
        port: actual_port,
        running,
    });
    drop(slot);

    std::thread::spawn(move || {
        for incoming in listener.incoming() {
            if !running_thread.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = incoming else {
                continue;
            };
            if !running_thread.load(Ordering::SeqCst) {
                break;
            }
            let app_conn = app_handle.clone();
            let run_conn = running_thread.clone();
            std::thread::spawn(move || {
                handle_connection(stream, &app_conn, &run_conn);
            });
        }
    });
}

pub fn shutdown_obs_server() {
    let mut slot = control_slot().lock().unwrap();
    stop_locked(&mut slot);
}

fn sanitize_asset_path(raw_path: &str) -> Option<String> {
    let no_query = raw_path.split('?').next().unwrap_or("");
    let trimmed = no_query.trim_start_matches('/');
    if trimmed.is_empty() || trimmed == "obs" || trimmed == "obs/" {
        return Some("obs.html".to_string());
    }
    if trimmed.contains("..") || trimmed.contains('\\') || trimmed.contains('\0') {
        return None;
    }
    Some(trimmed.to_string())
}

fn guess_mime(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript; charset=utf-8"
    } else if lower.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".json") {
        "application/json; charset=utf-8"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
}

fn load_asset(app: &AppHandle, rel_path: &str) -> Option<(Vec<u8>, String)> {
    if let Some(asset) = app.asset_resolver().get(rel_path.to_string()) {
        return Some((asset.bytes, asset.mime_type));
    }
    // Development fallback when running `cargo run` / `tauri dev` against built `../dist`
    let dev_dist = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("dist")
        .join(rel_path);
    if let Ok(bytes) = std::fs::read(&dev_dist) {
        return Some((bytes, guess_mime(rel_path).to_string()));
    }
    None
}

fn write_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    cache_control: &str,
    body: &[u8],
) {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len(),
        cache_control
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn handle_connection(mut stream: TcpStream, app: &AppHandle, running: &Arc<AtomicBool>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut request_line = String::new();
    {
        let mut reader = BufReader::new(&stream);
        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
            return;
        }
        // Drain remaining request headers cleanly
        let mut line = String::new();
        for _ in 0..64 {
            line.clear();
            let n = reader.read_line(&mut line).unwrap_or(0);
            if n == 0 || line == "\r\n" || line == "\n" {
                break;
            }
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let raw_path = parts.next().unwrap_or("/");

    if method == "OPTIONS" {
        let resp = "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, OPTIONS\r\nAccess-Control-Allow-Headers: *\r\nConnection: close\r\n\r\n";
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.flush();
        return;
    }

    if method != "GET" && method != "HEAD" {
        write_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            "no-store",
            b"Method Not Allowed",
        );
        return;
    }

    let clean_route = raw_path.split('?').next().unwrap_or("/");

    if let Some(id) = clean_route.strip_prefix("/obs/skins/") {
        match crate::custom_skins::load_saved_skin(id).and_then(|skin| serde_json::to_vec(&skin).map_err(|_|"Invalid skin".into())) {
            Ok(body) => write_response(&mut stream, "200 OK", "application/json; charset=utf-8", "no-store", if method == "HEAD" { b"" } else { &body }),
            Err(_) => write_response(&mut stream, "404 Not Found", "text/plain; charset=utf-8", "no-store", b"Skin not found"),
        }
        return;
    }

    if clean_route == "/obs/events" {
        handle_sse(stream, app, running);
        return;
    }

    if clean_route == "/obs/state" {
        let body = build_state_snapshot_json(app);
        write_response(
            &mut stream,
            "200 OK",
            "application/json; charset=utf-8",
            "no-store",
            if method == "HEAD" { b"" } else { body.as_bytes() },
        );
        return;
    }

    let Some(rel) = sanitize_asset_path(raw_path) else {
        write_response(
            &mut stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            "no-store",
            b"Invalid path",
        );
        return;
    };

    match load_asset(app, &rel) {
        Some((bytes, mime)) => {
            let cache = if rel.starts_with("assets/") {
                "public, max-age=3600"
            } else {
                "no-store"
            };
            write_response(
                &mut stream,
                "200 OK",
                &mime,
                cache,
                if method == "HEAD" { b"" } else { &bytes },
            );
        }
        None => {
            write_response(
                &mut stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                "no-store",
                b"Not Found",
            );
        }
    }
}

fn handle_sse(mut stream: TcpStream, app: &AppHandle, running: &Arc<AtomicBool>) {
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_nodelay(true);

    let header = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/event-stream; charset=utf-8\r\n",
        "Cache-Control: no-cache, no-transform\r\n",
        "Connection: keep-alive\r\n",
        "Access-Control-Allow-Origin: *\r\n",
        "X-Accel-Buffering: no\r\n\r\n",
    );
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }

    // Push immediate initial snapshot so OBS renders the exact active skin & layout on frame 1
    let init_frame = format_sse_frame("state", &build_state_snapshot_json(app));
    if stream.write_all(init_frame.as_bytes()).is_err() || stream.flush().is_err() {
        return;
    }

    let (tx, rx) = mpsc::sync_channel::<String>(SSE_CLIENT_QUEUE_CAP);
    {
        let mut guard = clients_slot().lock().unwrap();
        guard.push(tx);
    }
    OBS_CLIENT_COUNT.fetch_add(1, Ordering::Relaxed);
    let _guard = ClientGuard;

    loop {
        if !running.load(Ordering::Relaxed) {
            break;
        }
        match rx.recv_timeout(Duration::from_secs(15)) {
            Ok(frame) => {
                if stream.write_all(frame.as_bytes()).is_err() || stream.flush().is_err() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                if stream.write_all(b": ping\n\n").is_err() || stream.flush().is_err() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_obs_routes_and_blocks_traversal() {
        assert_eq!(sanitize_asset_path("/"), Some("obs.html".to_string()));
        assert_eq!(sanitize_asset_path("/obs"), Some("obs.html".to_string()));
        assert_eq!(sanitize_asset_path("/obs?scale=1.2"), Some("obs.html".to_string()));
        assert_eq!(
            sanitize_asset_path("/assets/obs-abc.js?v=1"),
            Some("assets/obs-abc.js".to_string())
        );
        assert_eq!(sanitize_asset_path("/../Cargo.toml"), None);
        assert_eq!(sanitize_asset_path("/assets/..\\secret"), None);
    }

    #[test]
    fn formats_valid_sse_frames() {
        let frame = format_sse_frame("input", "{\"kind\":\"keyDown\",\n\"vk\":65}");
        assert_eq!(frame, "event: input\ndata: {\"kind\":\"keyDown\", \"vk\":65}\n\n");
    }
}
