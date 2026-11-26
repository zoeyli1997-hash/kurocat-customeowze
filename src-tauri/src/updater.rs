use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

const QIANYI_API_LATEST: &str = "https://qianyi.one/kurocat/releases/latest.json";
const REPO_API_LATEST: &str = "https://api.github.com/repos/qianyiai/kurocat/releases/latest";
const REPO_RELEASES_PAGE: &str = "https://github.com/qianyiai/kurocat/releases/latest";

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
// A fork must never replace itself with an upstream binary. Enable updates only
// after a separately configured and reviewed fork release channel exists.
pub const UPSTREAM_UPDATES_ENABLED: bool = false;
const UPDATES_DISABLED_MESSAGE: &str = "定制版已停用官方更新，避免覆盖 DIY 功能。";

static LATEST_UPDATE_CACHE: OnceLock<Mutex<Option<UpdateInfo>>> = OnceLock::new();

pub fn get_cached_update_info() -> Option<UpdateInfo> {
    if !UPSTREAM_UPDATES_ENABLED {
        return None;
    }
    LATEST_UPDATE_CACHE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn store_cached_update_info(info: &UpdateInfo) {
    if let Ok(mut guard) = LATEST_UPDATE_CACHE.get_or_init(|| Mutex::new(None)).lock() {
        *guard = Some(info.clone());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub asset_name: Option<String>,
    pub download_url: Option<String>,
    pub asset_sha256: Option<String>,
    pub asset_size: Option<u64>,
    pub install_kind: String,
    pub notes_summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressEvent {
    pub stage: String,
    pub message: String,
    pub info: Option<UpdateInfo>,
}

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: Option<String>,
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: Option<u64>,
    digest: Option<String>,
}

pub fn parse_semver(raw: &str) -> (u64, u64, u64) {
    let clean = raw.trim().trim_start_matches(['v', 'V']);
    let main = clean.split('-').next().unwrap_or(clean);
    let mut parts = main.split('.');
    let major = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
    (major, minor, patch)
}

pub fn is_newer_version(latest: &str, current: &str) -> bool {
    parse_semver(latest) > parse_semver(current)
}

fn extract_sha256(digest: &Option<String>) -> Option<String> {
    let raw = digest.as_deref()?.trim();
    let hex = raw.strip_prefix("sha256:").unwrap_or(raw).trim().to_ascii_lowercase();
    if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(hex)
    } else {
        None
    }
}

fn detect_install_kind() -> &'static str {
    #[cfg(all(windows, feature = "win7"))]
    {
        return "windows-win7";
    }
    #[cfg(all(windows, not(feature = "win7")))]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                if dir.join("uninstall.exe").exists() {
                    return "windows-nsis";
                }
            }
        }
        return "windows-portable";
    }
    #[cfg(target_os = "macos")]
    {
        return "macos-app";
    }
    #[cfg(target_os = "linux")]
    {
        if cfg!(target_arch = "aarch64") {
            return "linux-deb-arm64";
        }
        return "linux-deb-amd64";
    }
    #[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
    {
        return "manual";
    }
}

fn select_asset<'a>(assets: &'a [GhAsset], install_kind: &str) -> Option<&'a GhAsset> {
    let want_108 = cfg!(feature = "layout-108");
    match install_kind {
        "windows-nsis" => assets
            .iter()
            .find(|a| {
                let n = a.name.to_ascii_lowercase();
                n.ends_with("-setup.exe") && n.contains("x64") && (n.contains("108key") == want_108)
            })
            .or_else(|| {
                assets.iter().find(|a| {
                    let n = a.name.to_ascii_lowercase();
                    n.ends_with("-setup.exe") && n.contains("x64")
                })
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_ascii_lowercase().ends_with("-windows-x64.exe"))
            }),
        "windows-portable" => assets
            .iter()
            .find(|a| {
                let n = a.name.to_ascii_lowercase();
                n.ends_with("-windows-x64.exe") && (n.contains("108key") == want_108)
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_ascii_lowercase().ends_with("-windows-x64.exe"))
            })
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_ascii_lowercase().ends_with("-setup.exe"))
            }),
        "windows-win7" => assets.iter().find(|a| {
            let n = a.name.to_ascii_lowercase();
            n.contains("win7") && n.ends_with(".zip")
        }),
        "macos-app" => assets
            .iter()
            .find(|a| a.name.to_ascii_lowercase().ends_with("-macos-universal.app.tar.gz"))
            .or_else(|| {
                assets
                    .iter()
                    .find(|a| a.name.to_ascii_lowercase().ends_with("-macos-universal.dmg"))
            }),
        "linux-deb-arm64" => assets
            .iter()
            .find(|a| {
                let n = a.name.to_ascii_lowercase();
                n.ends_with(".deb") && (n.contains("arm64") || n.contains("aarch64"))
            })
            .or_else(|| {
                assets.iter().find(|a| {
                    let n = a.name.to_ascii_lowercase();
                    n.ends_with(".appimage") && (n.contains("arm64") || n.contains("aarch64"))
                })
            }),
        "linux-deb-amd64" => assets
            .iter()
            .find(|a| {
                let n = a.name.to_ascii_lowercase();
                n.ends_with(".deb") && (n.contains("amd64") || n.contains("x86_64"))
            })
            .or_else(|| {
                assets.iter().find(|a| {
                    let n = a.name.to_ascii_lowercase();
                    n.ends_with(".appimage") && (n.contains("amd64") || n.contains("x86_64"))
                })
            }),
        _ => None,
    }
}

fn summarize_notes(body: Option<&str>) -> String {
    let Some(raw) = body else {
        return String::new();
    };
    raw.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(180)
        .collect()
}

#[cfg(any(windows, test))]
fn parse_http_url(url: &str) -> Result<(bool, String, u16, String), String> {
    let (is_https, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return Err(format!("不支持的更新地址协议: {}", url));
    };

    let (authority, path_and_query) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    if authority.is_empty() {
        return Err("更新地址缺少主机名".to_string());
    }

    let default_port = if is_https { 443 } else { 80 };
    let (host, port) = if let Some(colon) = authority.rfind(':') {
        let port_str = &authority[colon + 1..];
        if let Ok(p) = port_str.parse::<u16>() {
            (&authority[..colon], p)
        } else {
            (authority, default_port)
        }
    } else {
        (authority, default_port)
    };

    Ok((
        is_https,
        host.to_string(),
        port,
        path_and_query.to_string(),
    ))
}

/// Compact pure-Rust FIPS 180-4 SHA-256 hasher (zero external crate overhead, zero PowerShell).
#[cfg(any(windows, test))]
struct Sha256 {
    state: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_bytes: u64,
}

#[cfg(any(windows, test))]
impl Sha256 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            buf: [0u8; 64],
            buf_len: 0,
            total_bytes: 0,
        }
    }

    fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(Self::K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    fn update(&mut self, mut data: &[u8]) {
        self.total_bytes = self.total_bytes.wrapping_add(data.len() as u64);
        if self.buf_len > 0 {
            let take = (64 - self.buf_len).min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                Self::compress(&mut self.state, &block);
                self.buf_len = 0;
            }
        }
        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            Self::compress(&mut self.state, &block);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buf[..data.len()].copy_from_slice(data);
            self.buf_len = data.len();
        }
    }

    fn finish_hex(mut self) -> String {
        let bit_len = self.total_bytes.wrapping_mul(8);
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        if self.buf_len > 56 {
            self.buf[self.buf_len..64].fill(0);
            let block = self.buf;
            Self::compress(&mut self.state, &block);
            self.buf_len = 0;
        }
        self.buf[self.buf_len..56].fill(0);
        self.buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buf;
        Self::compress(&mut self.state, &block);

        let mut out = String::with_capacity(64);
        for word in self.state {
            use std::fmt::Write;
            let _ = write!(&mut out, "{:08x}", word);
        }
        out
    }
}

#[cfg(windows)]
mod winhttp_client {
    use super::parse_http_url;
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
        WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
        WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts,
        WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE,
        WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2, WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3,
        WINHTTP_OPTION_SECURE_PROTOCOLS, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    };

    struct Handle(*mut c_void);

    impl Drop for Handle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Performs a native WinHTTP GET request (no PowerShell / curl child process),
    /// streaming response chunks into `on_chunk`. Supports Win7 SP1 through Win11
    /// and honors system / PAC proxies automatically.
    pub fn get_stream(
        url: &str,
        user_agent: &str,
        extra_headers: Option<&str>,
        receive_timeout_ms: i32,
        mut on_chunk: impl FnMut(&[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        let (is_https, host, port, path) = parse_http_url(url)?;
        let ua_w = to_wide(user_agent);

        // Win8.1/10/11: AUTOMATIC_PROXY reads user IE/system proxy & PAC automatically.
        // Win7 SP1 fallback: DEFAULT_PROXY.
        let session_raw = unsafe {
            let h = WinHttpOpen(
                ua_w.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                null(),
                null(),
                0,
            );
            if !h.is_null() {
                h
            } else {
                WinHttpOpen(
                    ua_w.as_ptr(),
                    WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                    null(),
                    null(),
                    0,
                )
            }
        };
        if session_raw.is_null() {
            let err = unsafe { GetLastError() };
            return Err(format!("初始化 WinHTTP 会话失败 (错误码 {})", err));
        }
        let session = Handle(session_raw);

        unsafe {
            let _ = WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, receive_timeout_ms);
            let mut protocols =
                WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3;
            if WinHttpSetOption(
                session.0,
                WINHTTP_OPTION_SECURE_PROTOCOLS,
                &protocols as *const u32 as *const c_void,
                std::mem::size_of::<u32>() as u32,
            ) == 0
            {
                protocols = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2;
                let _ = WinHttpSetOption(
                    session.0,
                    WINHTTP_OPTION_SECURE_PROTOCOLS,
                    &protocols as *const u32 as *const c_void,
                    std::mem::size_of::<u32>() as u32,
                );
            }
        }

        let host_w = to_wide(&host);
        let connect_raw = unsafe { WinHttpConnect(session.0, host_w.as_ptr(), port, 0) };
        if connect_raw.is_null() {
            let err = unsafe { GetLastError() };
            return Err(format!("连接更新服务器失败 (错误码 {})", err));
        }
        let connect = Handle(connect_raw);

        let verb_w = to_wide("GET");
        let path_w = to_wide(&path);
        let flags = if is_https { WINHTTP_FLAG_SECURE } else { 0 };
        let req_raw = unsafe {
            WinHttpOpenRequest(
                connect.0,
                verb_w.as_ptr(),
                path_w.as_ptr(),
                null(),
                null(),
                null(),
                flags,
            )
        };
        if req_raw.is_null() {
            let err = unsafe { GetLastError() };
            return Err(format!("创建更新请求失败 (错误码 {})", err));
        }
        let request = Handle(req_raw);

        let headers_w = extra_headers.map(to_wide);
        let (hdr_ptr, hdr_len) = match &headers_w {
            Some(buf) => (buf.as_ptr(), (buf.len().saturating_sub(1)) as u32),
            None => (null(), 0),
        };

        let sent = unsafe {
            WinHttpSendRequest(request.0, hdr_ptr, hdr_len, null(), 0, 0, 0)
        };
        if sent == 0 {
            let err = unsafe { GetLastError() };
            return Err(format!("连接 GitHub 检查更新失败，请检查网络或代理 (错误码 {})", err));
        }

        let received = unsafe { WinHttpReceiveResponse(request.0, null_mut()) };
        if received == 0 {
            let err = unsafe { GetLastError() };
            return Err(format!("等待 GitHub 响应失败，请检查网络或代理 (错误码 {})", err));
        }

        let mut status_code: u32 = 0;
        let mut status_size = std::mem::size_of::<u32>() as u32;
        let got_status = unsafe {
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                &mut status_code as *mut u32 as *mut c_void,
                &mut status_size,
                null_mut(),
            )
        };
        if got_status == 0 {
            let err = unsafe { GetLastError() };
            return Err(format!("读取 HTTP 状态码失败 (错误码 {})", err));
        }
        if !(200..=299).contains(&status_code) {
            return Err(format!("服务器返回 HTTP {}", status_code));
        }

        let mut buf = vec![0u8; 32 * 1024];
        loop {
            let mut available: u32 = 0;
            if unsafe { WinHttpQueryDataAvailable(request.0, &mut available) } == 0 {
                let err = unsafe { GetLastError() };
                return Err(format!("读取网络数据流失败 (错误码 {})", err));
            }
            if available == 0 {
                break;
            }
            let mut remaining = available as usize;
            while remaining > 0 {
                let to_read = remaining.min(buf.len()) as u32;
                let mut bytes_read: u32 = 0;
                if unsafe {
                    WinHttpReadData(
                        request.0,
                        buf.as_mut_ptr() as *mut c_void,
                        to_read,
                        &mut bytes_read,
                    )
                } == 0
                {
                    let err = unsafe { GetLastError() };
                    return Err(format!("读取响应内容失败 (错误码 {})", err));
                }
                if bytes_read == 0 {
                    break;
                }
                on_chunk(&buf[..bytes_read as usize])?;
                remaining = remaining.saturating_sub(bytes_read as usize);
            }
        }

        Ok(())
    }
}

#[cfg(windows)]
fn fetch_json_from_url(url: &str, accept: Option<&str>, timeout_ms: i32) -> Result<String, String> {
    let ua = format!("KuroCat/{}", CURRENT_VERSION);
    let mut body = Vec::with_capacity(8192);
    winhttp_client::get_stream(url, &ua, accept, timeout_ms, |chunk| {
        if body.len() + chunk.len() > 2 * 1024 * 1024 {
            return Err("更新接口响应过大".to_string());
        }
        body.extend_from_slice(chunk);
        Ok(())
    })?;

    let text = String::from_utf8_lossy(&body).trim().to_string();
    if text.is_empty() {
        return Err("更新接口返回空响应".to_string());
    }
    Ok(text)
}

#[cfg(windows)]
fn fetch_latest_release_json() -> Result<String, String> {
    // Primary: fast domestic CDN mirror (no proxy needed, zero GitHub API rate limit)
    if let Ok(text) = fetch_json_from_url(QIANYI_API_LATEST, Some("Accept: application/json\r\n"), 8_000) {
        if serde_json::from_str::<GhRelease>(&text).is_ok() {
            return Ok(text);
        }
    }
    // Fallback: official GitHub Releases API
    fetch_json_from_url(
        REPO_API_LATEST,
        Some("Accept: application/vnd.github+json\r\n"),
        20_000,
    )
}

#[cfg(not(windows))]
fn fetch_json_from_url(url: &str, accept: &str, connect_secs: &str, max_secs: &str) -> Result<String, String> {
    use std::process::Command;

    let curl_bin = if cfg!(target_os = "macos") {
        "/usr/bin/curl"
    } else {
        "curl"
    };
    let output = Command::new(curl_bin)
        .args([
            "-fsSL",
            "--connect-timeout",
            connect_secs,
            "--max-time",
            max_secs,
            "-H",
            &format!("User-Agent: KuroCat/{}", CURRENT_VERSION),
            "-H",
            accept,
            url,
        ])
        .output()
        .map_err(|e| format!("无法调用 curl 检查更新: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "连接更新服务器失败，请检查网络或代理".to_string()
        } else {
            format!("检查更新失败: {}", stderr)
        });
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("更新接口返回空响应".to_string());
    }
    Ok(text)
}

#[cfg(not(windows))]
fn fetch_latest_release_json() -> Result<String, String> {
    // Primary: fast domestic CDN mirror (no proxy needed, zero GitHub API rate limit)
    if let Ok(text) = fetch_json_from_url(QIANYI_API_LATEST, "Accept: application/json", "5", "9") {
        if serde_json::from_str::<GhRelease>(&text).is_ok() {
            return Ok(text);
        }
    }
    // Fallback: official GitHub Releases API
    fetch_json_from_url(REPO_API_LATEST, "Accept: application/vnd.github+json", "10", "20")
}

pub fn check_latest_update() -> Result<UpdateInfo, String> {
    if !UPSTREAM_UPDATES_ENABLED {
        return Err(UPDATES_DISABLED_MESSAGE.to_string());
    }
    let raw_json = fetch_latest_release_json()?;
    let rel: GhRelease = serde_json::from_str(&raw_json)
        .map_err(|e| format!("解析最新版本信息失败: {}", e))?;

    let latest_clean = rel
        .tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();
    let has_update = is_newer_version(&latest_clean, CURRENT_VERSION);
    let install_kind = detect_install_kind();
    let asset = select_asset(&rel.assets, install_kind);

    let info = UpdateInfo {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest_clean,
        has_update,
        release_url: rel
            .html_url
            .unwrap_or_else(|| REPO_RELEASES_PAGE.to_string()),
        asset_name: asset.map(|a| a.name.clone()),
        download_url: asset.map(|a| a.browser_download_url.clone()),
        asset_sha256: asset.and_then(|a| extract_sha256(&a.digest)),
        asset_size: asset.and_then(|a| a.size),
        install_kind: install_kind.to_string(),
        notes_summary: summarize_notes(rel.body.as_deref()),
    };
    store_cached_update_info(&info);
    Ok(info)
}

fn emit_progress(app: &AppHandle, stage: &str, message: impl Into<String>, info: Option<UpdateInfo>) {
    let _ = app.emit(
        "kuro-update-status",
        UpdateProgressEvent {
            stage: stage.to_string(),
            message: message.into(),
            info,
        },
    );
}

pub fn open_url_in_browser(url: &str) {
    #[cfg(windows)]
    {
        use std::ptr::{null, null_mut};
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let op: Vec<u16> = "open\0".encode_utf16().collect();
        let target: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            ShellExecuteW(
                null_mut(),
                op.as_ptr(),
                target.as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            );
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("/usr/bin/open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

#[cfg(windows)]
fn download_and_verify_windows(url: &str, dest: &PathBuf, expected_sha256: Option<&str>) -> Result<(), String> {
    use std::io::Write;

    let part_path = dest.with_extension("part");
    let _ = std::fs::remove_file(&part_path);
    let mut file = std::fs::File::create(&part_path)
        .map_err(|e| format!("无法创建下载临时文件: {}", e))?;

    let ua = format!("KuroCat/{}", CURRENT_VERSION);
    let mut hasher = Sha256::new();
    let mut total_bytes: u64 = 0;

    let download_res = winhttp_client::get_stream(url, &ua, None, 180_000, |chunk| {
        file.write_all(chunk)
            .map_err(|e| format!("写入安装包失败: {}", e))?;
        hasher.update(chunk);
        total_bytes += chunk.len() as u64;
        Ok(())
    });

    drop(file);
    if let Err(err) = download_res {
        let _ = std::fs::remove_file(&part_path);
        return Err(err);
    }

    if total_bytes < 500_000 {
        let _ = std::fs::remove_file(&part_path);
        return Err("下载文件不完整".to_string());
    }

    if let Some(expected) = expected_sha256 {
        let expected_clean = expected.trim().to_ascii_lowercase();
        if !expected_clean.is_empty() {
            let actual = hasher.finish_hex();
            if actual != expected_clean {
                let _ = std::fs::remove_file(&part_path);
                return Err(format!("SHA-256 校验失败: {}", actual));
            }
        }
    }

    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part_path, dest).map_err(|e| {
        let _ = std::fs::remove_file(&part_path);
        format!("保存安装包失败: {}", e)
    })?;
    Ok(())
}

#[cfg(windows)]
fn install_windows_update(app: &AppHandle, info: &UpdateInfo) -> Result<(), String> {
    use std::process::Command;

    if info.install_kind == "windows-win7"
        || info
            .asset_name
            .as_deref()
            .map(|n| n.to_ascii_lowercase().ends_with(".zip"))
            .unwrap_or(false)
    {
        let target_url = info.download_url.as_deref().unwrap_or(&info.release_url);
        open_url_in_browser(target_url);
        emit_progress(
            app,
            "up_to_date",
            format!("已在浏览器打开 v{} Win7 兼容整合包下载页", info.latest_version),
            Some(info.clone()),
        );
        return Ok(());
    }

    let url = info
        .download_url
        .as_deref()
        .ok_or_else(|| "未找到匹配的 Windows 更新包".to_string())?;
    let asset_name = info
        .asset_name
        .as_deref()
        .unwrap_or("KuroCat-update.exe");

    emit_progress(
        app,
        "downloading",
        format!("正在下载 v{}...", info.latest_version),
        Some(info.clone()),
    );

    let temp_dir = std::env::temp_dir().join("kurocat-updates");
    let _ = std::fs::create_dir_all(&temp_dir);
    let safe_name = asset_name.replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_");
    let target_file = temp_dir.join(safe_name);

    download_and_verify_windows(url, &target_file, info.asset_sha256.as_deref())?;

    emit_progress(
        app,
        "installing",
        format!("校验通过，正在安装 v{} 并重启...", info.latest_version),
        Some(info.clone()),
    );

    let is_setup_installer = asset_name.to_ascii_lowercase().ends_with("-setup.exe");

    if is_setup_installer {
        // Launch Tauri NSIS installer in passive auto-relaunch update mode (/P /R /UPDATE)
        Command::new(&target_file)
            .args(["/P", "/R", "/UPDATE"])
            .spawn()
            .map_err(|e| format!("启动安装程序失败: {}", e))?;
    } else {
        // Zero-PowerShell portable atomic swap: Windows allows renaming a running .exe in place,
        // so we move the running binary aside, copy the verified new binary into place, and launch
        // the new process with --wait-pid + --cleanup-old-exe to remove the old binary once we exit.
        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let pid = std::process::id();
        let old_exe = current_exe.with_extension("exe.old");
        let _ = std::fs::remove_file(&old_exe);

        std::fs::rename(&current_exe, &old_exe)
            .map_err(|e| format!("备份旧版本程序失败: {}", e))?;

        if let Err(e) = std::fs::copy(&target_file, &current_exe) {
            let _ = std::fs::rename(&old_exe, &current_exe);
            return Err(format!("替换新版本程序失败: {}", e));
        }
        let _ = std::fs::remove_file(&target_file);

        if let Err(e) = Command::new(&current_exe)
            .args([
                "--wait-pid",
                &pid.to_string(),
                "--cleanup-old-exe",
                &old_exe.to_string_lossy(),
            ])
            .spawn()
        {
            let _ = std::fs::remove_file(&current_exe);
            let _ = std::fs::rename(&old_exe, &current_exe);
            return Err(format!("启动新版本程序失败: {}", e));
        }
    }

    let app_clone = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(450));
        app_clone.exit(0);
    });

    Ok(())
}

#[cfg(target_os = "macos")]
fn find_current_macos_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    for ancestor in exe.ancestors() {
        if ancestor.extension().and_then(|s| s.to_str()) == Some("app") {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn install_macos_update(app: &AppHandle, info: &UpdateInfo) -> Result<(), String> {
    use std::process::Command;

    let url = info
        .download_url
        .as_deref()
        .ok_or_else(|| "未找到匹配的 macOS 更新包".to_string())?;
    let asset_name = info
        .asset_name
        .as_deref()
        .unwrap_or("KuroCat-macos-universal.app.tar.gz");

    if !asset_name.to_ascii_lowercase().ends_with(".tar.gz") {
        open_url_in_browser(url);
        return Ok(());
    }

    let app_bundle = find_current_macos_app_bundle()
        .unwrap_or_else(|| PathBuf::from("/Applications/KuroCat.app"));

    emit_progress(
        app,
        "downloading",
        format!("正在下载 macOS v{}...", info.latest_version),
        Some(info.clone()),
    );

    let temp_dir = std::env::temp_dir().join(format!("kurocat-update-{}", info.latest_version));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let archive_path = temp_dir.join("KuroCat-update.app.tar.gz");

    let dl = Command::new("/usr/bin/curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "15",
            "--max-time",
            "240",
            "-H",
            &format!("User-Agent: KuroCat/{}", CURRENT_VERSION),
            "-o",
            archive_path.to_str().unwrap_or("/tmp/KuroCat-update.app.tar.gz"),
            url,
        ])
        .output()
        .map_err(|e| format!("下载失败: {}", e))?;

    if !dl.status.success() {
        return Err("下载 macOS 更新包失败".to_string());
    }

    // Verify SHA-256 if GitHub Release provided a digest
    if let Some(expected_sha) = info.asset_sha256.as_deref() {
        emit_progress(
            app,
            "verifying",
            "正在校验安装包完整性...",
            Some(info.clone()),
        );
        let shasum = Command::new("/usr/bin/shasum")
            .args(["-a", "256", archive_path.to_str().unwrap_or("")])
            .output()
            .map_err(|e| format!("校验失败: {}", e))?;
        let out_str = String::from_utf8_lossy(&shasum.stdout);
        let actual = out_str.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        if actual != expected_sha.to_ascii_lowercase() {
            return Err("安装包 SHA-256 校验未通过".to_string());
        }
    }

    let extract_dir = temp_dir.join("extracted");
    std::fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;
    let untar = Command::new("/usr/bin/tar")
        .args([
            "-xzf",
            archive_path.to_str().unwrap_or(""),
            "-C",
            extract_dir.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| format!("解压失败: {}", e))?;
    if !untar.status.success() {
        return Err("解压更新包失败".to_string());
    }

    // Find extracted .app (KuroCat.app or KuroCatPet.app)
    let new_app = std::fs::read_dir(&extract_dir)
        .ok()
        .and_then(|mut it| {
            it.find_map(|entry| {
                let p = entry.ok()?.path();
                if p.extension().and_then(|s| s.to_str()) == Some("app") {
                    Some(p)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| "更新包内未找到 .app 程序".to_string())?;

    // Clear quarantine flag so macOS Gatekeeper won't block the freshly updated open-source app
    let _ = Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine", new_app.to_str().unwrap_or("")])
        .status();

    emit_progress(
        app,
        "installing",
        format!("正在安装 v{} 并重启...", info.latest_version),
        Some(info.clone()),
    );

    let pid = std::process::id();
    let dst_str = app_bundle.to_string_lossy().replace('\'', "'\\''");
    let src_str = new_app.to_string_lossy().replace('\'', "'\\''");
    let tmp_str = temp_dir.to_string_lossy().replace('\'', "'\\''");
    let swap_sh = format!(
        "for i in $(seq 1 40); do kill -0 {pid} 2>/dev/null || break; sleep 0.2; done; \
         sleep 0.25; \
         rm -rf '{dst}.bak'; \
         if [ -d '{dst}' ]; then mv '{dst}' '{dst}.bak'; fi; \
         if mv '{src}' '{dst}'; then \
           rm -rf '{dst}.bak' '{tmp}'; \
           /usr/bin/xattr -dr com.apple.quarantine '{dst}' 2>/dev/null || true; \
           /usr/bin/open -n '{dst}'; \
         else \
           if [ -d '{dst}.bak' ]; then mv '{dst}.bak' '{dst}'; fi; \
           /usr/bin/open '{url}'; \
         fi",
        pid = pid,
        dst = dst_str,
        src = src_str,
        tmp = tmp_str,
        url = info.release_url.replace('\'', "'\\''"),
    );

    Command::new("/bin/sh")
        .args(["-c", &swap_sh])
        .spawn()
        .map_err(|e| format!("启动更新脚本失败: {}", e))?;

    let app_clone = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(450));
        app_clone.exit(0);
    });

    Ok(())
}

pub fn perform_update_install(app: &AppHandle, info: UpdateInfo) -> Result<(), String> {
    if !UPSTREAM_UPDATES_ENABLED {
        return Err(UPDATES_DISABLED_MESSAGE.to_string());
    }
    if !info.has_update {
        emit_progress(
            app,
            "up_to_date",
            format!("已是最新版本 v{}", info.current_version),
            Some(info),
        );
        return Ok(());
    }

    #[cfg(windows)]
    {
        return install_windows_update(app, &info);
    }

    #[cfg(target_os = "macos")]
    {
        return install_macos_update(app, &info);
    }

    #[cfg(all(not(windows), not(target_os = "macos")))]
    {
        let target_url = info.download_url.as_deref().unwrap_or(&info.release_url);
        open_url_in_browser(target_url);
        emit_progress(
            app,
            "up_to_date",
            format!("已在浏览器打开 v{} 安装包下载页", info.latest_version),
            Some(info),
        );
        Ok(())
    }
}

pub fn start_background_update_checker(app: AppHandle) {
    if !UPSTREAM_UPDATES_ENABLED {
        return;
    }
    std::thread::spawn(move || {
        // Gentle initial delay after launch so startup stays instant & quiet
        std::thread::sleep(std::time::Duration::from_secs(8));
        loop {
            if let Ok(info) = check_latest_update() {
                if info.has_update {
                    emit_progress(
                        &app,
                        "available",
                        format!("发现新版本 v{}", info.latest_version),
                        Some(info),
                    );
                }
            }
            // Re-check every 6 hours while running
            std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison_works() {
        assert!(is_newer_version("4.4.0", "4.3.0"));
        assert!(is_newer_version("v4.10.0", "4.4.0"));
        assert!(is_newer_version("5.0.0", "4.9.9"));
        assert!(!is_newer_version("4.4.0", "4.4.0"));
        assert!(!is_newer_version("v4.3.9", "4.4.0"));
    }

    #[test]
    fn sha256_extraction_validates_hex() {
        let valid = Some("sha256:05c1be2c444f5303770f6d59e216bc0ed7b69321fc44368e4ec231405570f950".to_string());
        assert_eq!(
            extract_sha256(&valid).as_deref(),
            Some("05c1be2c444f5303770f6d59e216bc0ed7b69321fc44368e4ec231405570f950")
        );
        assert_eq!(extract_sha256(&Some("sha256:short".to_string())), None);
    }

    #[test]
    fn qianyi_mirror_latest_json_parses_assets_and_digest() {
        let sample = r#"{
            "tag_name": "v9.9.9",
            "html_url": "https://qianyi.one/kurocat/",
            "body": "Performance & UX update",
            "assets": [
                {
                    "name": "KuroCat-9.9.9-x64-setup.exe",
                    "browser_download_url": "https://qianyi.one/kurocat/releases/v9.9.9/KuroCat-9.9.9-x64-setup.exe",
                    "size": 2250000,
                    "digest": "sha256:05c1be2c444f5303770f6d59e216bc0ed7b69321fc44368e4ec231405570f950"
                },
                {
                    "name": "KuroCat-9.9.9-macos-universal.app.tar.gz",
                    "browser_download_url": "https://qianyi.one/kurocat/releases/v9.9.9/KuroCat-9.9.9-macos-universal.app.tar.gz",
                    "size": 3100000,
                    "digest": "sha256:05c1be2c444f5303770f6d59e216bc0ed7b69321fc44368e4ec231405570f950"
                }
            ]
        }"#;
        let rel: GhRelease = serde_json::from_str(sample).expect("parse mirror json");
        assert!(is_newer_version(&rel.tag_name, CURRENT_VERSION));
        let win_asset = select_asset(&rel.assets, "windows-nsis").expect("select win nsis");
        assert!(win_asset.browser_download_url.starts_with("https://qianyi.one/"));
        assert_eq!(
            extract_sha256(&win_asset.digest).as_deref(),
            Some("05c1be2c444f5303770f6d59e216bc0ed7b69321fc44368e4ec231405570f950")
        );
        let mac_asset = select_asset(&rel.assets, "macos-app").expect("select mac tar.gz");
        assert!(mac_asset.name.ends_with(".app.tar.gz"));
    }

    #[test]
    fn pure_rust_sha256_matches_nist_vectors() {
        let h_empty = Sha256::new();
        assert_eq!(
            h_empty.finish_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let mut h_abc = Sha256::new();
        h_abc.update(b"abc");
        assert_eq!(
            h_abc.finish_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let mut h_long = Sha256::new();
        h_long.update(b"abcdbcdecdefdefgefghfghighijhijk");
        h_long.update(b"ijkljklmklmnlmnomnopnopq");
        assert_eq!(
            h_long.finish_hex(),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn http_url_parser_handles_github_endpoints() {
        let (https, host, port, path) =
            parse_http_url("https://api.github.com/repos/qianyiai/kurocat/releases/latest").unwrap();
        assert!(https);
        assert_eq!(host, "api.github.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/repos/qianyiai/kurocat/releases/latest");

        let (https2, host2, port2, path2) =
            parse_http_url("http://127.0.0.1:8080/dl?v=1").unwrap();
        assert!(!https2);
        assert_eq!(host2, "127.0.0.1");
        assert_eq!(port2, 8080);
        assert_eq!(path2, "/dl?v=1");
    }

    #[test]
    fn custom_build_blocks_upstream_update_checks_and_cached_offers() {
        let error = check_latest_update().expect_err("fork must not contact upstream releases");
        assert!(error.contains("已停用官方更新"));
        assert!(get_cached_update_info().is_none());
    }
}
