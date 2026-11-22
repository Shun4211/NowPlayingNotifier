//! 更新の確認とインストール。
//!
//! GitHub Releases の最新版を問い合わせ、今のバージョンより新しければ知らせる。
//! 利用者が更新を選んだら、インストーラーをダウンロードし、アプリを終了してから
//! 画面の少ないモード（/SILENT）で実行する。インストーラーは終わるとアプリを起動し直す。
//! 通信は Windows の WinHTTP で行う（送るのはバージョン確認とダウンロードのリクエストだけ）。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;
use windows::core::{w, PCWSTR};
use windows::Win32::Networking::WinHttp::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::win::{post_event, wide, AppEvent};

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/Shun4211/NowPlayingNotifier/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/Shun4211/NowPlayingNotifier/releases/latest";

/// インストーラーが作るアンインストーラー。exe の隣にあれば、インストーラーで入れたものとみなす。
const UNINSTALLER: &str = "unins000.exe";

/// 公開されている最新版。
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    /// "1.2.3"（先頭の v は除く）
    pub version: String,
    /// リリースのページ
    pub page_url: String,
    /// インストーラー（*-setup.exe）のダウンロード URL
    pub installer_url: Option<String>,
}

pub enum UpdateEvent {
    /// 確認が終わった。新しい版があれば Some。`manual` は利用者が「今すぐ確認」を押したとき。
    Checked { manual: bool, result: Result<Option<Release>, String> },
    /// インストーラーのダウンロードが終わった。
    Downloaded(Result<PathBuf, String>),
}

/// "v1.2.3" や "1.2" を比較用の数字の組にする。数字でない部分があれば None。
pub fn parse_version(s: &str) -> Option<[u32; 3]> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    // "1.2.3-beta" や "1.2.3+build" の後ろは比べない
    let core = s.split(['-', '+']).next()?;
    let mut parts = [0u32; 3];
    let mut count = 0;
    for (i, p) in core.split('.').enumerate() {
        if i >= 3 {
            return None;
        }
        parts[i] = p.parse().ok()?;
        count += 1;
    }
    (count > 0).then_some(parts)
}

/// `latest` が `current` より新しいか。どちらかが読めなければ false。
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// GitHub API の releases/latest の応答から、必要な情報を取り出す。
pub fn parse_release(json: &str) -> Result<Release, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("invalid response: {e}"))?;
    let tag = v.get("tag_name").and_then(Value::as_str).ok_or("missing tag_name")?;
    let version = tag.trim_start_matches(['v', 'V']).to_string();
    if parse_version(&version).is_none() {
        return Err(format!("unexpected version: {tag}"));
    }
    let page_url = v
        .get("html_url")
        .and_then(Value::as_str)
        .filter(|u| u.starts_with("https://"))
        .unwrap_or(RELEASES_PAGE)
        .to_string();
    let installer_url = v
        .get("assets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|a| a.get("name").and_then(Value::as_str).is_some_and(|n| n.ends_with("-setup.exe")))
        .and_then(|a| a.get("browser_download_url").and_then(Value::as_str))
        .filter(|u| u.starts_with("https://"))
        .map(str::to_string);
    Ok(Release { version, page_url, installer_url })
}

/// 別スレッドで最新版を確認し、結果を `UpdateEvent::Checked` で知らせる。
pub fn check(manual: bool) {
    std::thread::spawn(move || {
        let result = http_get(LATEST_RELEASE_API, "Accept: application/vnd.github+json")
            .and_then(|body| parse_release(&String::from_utf8_lossy(&body)))
            .map(|r| is_newer(&r.version, crate::APP_VERSION).then_some(r));
        post_event(AppEvent::Update(UpdateEvent::Checked { manual, result }));
    });
}

/// インストーラーで入れたアプリか（zip 版の exe を直接使っている場合は自動で入れ替えない）。
pub fn is_installed() -> bool {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(UNINSTALLER))).is_some_and(|p| p.is_file())
}

fn download_dir() -> PathBuf {
    std::env::temp_dir().join("NowPlayingNotifier-update")
}

/// 別スレッドでインストーラーをダウンロードし、結果を `UpdateEvent::Downloaded` で知らせる。
pub fn download(url: String) {
    std::thread::spawn(move || {
        let result = (|| {
            let body = http_get(&url, "Accept: application/octet-stream")?;
            // 小さすぎるものはエラーページなどなので使わない
            if body.len() < 100 * 1024 || !body.starts_with(b"MZ") {
                return Err("downloaded file is not an installer".to_string());
            }
            let dir = download_dir();
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let path = dir.join("NowPlayingNotifier-setup.exe");
            std::fs::write(&path, body).map_err(|e| e.to_string())?;
            Ok(path)
        })();
        post_event(AppEvent::Update(UpdateEvent::Downloaded(result)));
    });
}

/// アプリの終了後に実行するインストーラー。
static PENDING_INSTALLER: Mutex<Option<PathBuf>> = Mutex::new(None);

/// アプリの終了後にインストーラーを実行するよう予約する。
pub fn set_pending_installer(path: PathBuf) {
    if let Ok(mut p) = PENDING_INSTALLER.lock() {
        *p = Some(path);
    }
}

/// 予約されたインストーラーを実行する。二重起動防止の Mutex を手放した後（終了直前）に呼ぶこと。
/// （インストーラーはアプリが起動中だと止まるため）
pub fn launch_pending_installer() {
    let Some(path) = PENDING_INSTALLER.lock().ok().and_then(|mut p| p.take()) else { return };
    launch_installer(&path);
}

fn launch_installer(path: &Path) {
    let file = wide(&path.to_string_lossy());
    // /RELAUNCH=1: インストールが終わったらアプリを起動し直す（installer\NowPlayingNotifier.iss）
    let args = w!("/SILENT /SUPPRESSMSGBOXES /NORESTART /RELAUNCH=1");
    unsafe {
        ShellExecuteW(None, w!("open"), file.pcwstr(), args, PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// ブラウザーでページを開く。
pub fn open_page(url: &str) {
    if !url.starts_with("https://") {
        return;
    }
    let url = wide(url);
    unsafe {
        ShellExecuteW(None, w!("open"), url.pcwstr(), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

// ---------------------------------------------------------------------------
// WinHTTP

/// "https://host/path" を (host, path) に分ける。https 以外は使わない。
fn split_https_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    (!host.is_empty() && !host.contains([':', '@'])).then_some((host, path))
}

struct Handle(*mut core::ffi::c_void);

impl Handle {
    fn new(h: *mut core::ffi::c_void) -> Result<Self, String> {
        if h.is_null() {
            Err(windows::core::Error::from_win32().message())
        } else {
            Ok(Self(h))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

/// HTTPS で GET し、本文を返す（リダイレクトは WinHTTP がたどる）。200 以外はエラー。
fn http_get(url: &str, accept: &str) -> Result<Vec<u8>, String> {
    let (host, path) = split_https_url(url).ok_or_else(|| format!("unsupported url: {url}"))?;
    let agent = wide(&format!("NowPlayingNotifier/{}", crate::APP_VERSION));
    let host = wide(host);
    let path = wide(path);
    let headers: Vec<u16> = accept.encode_utf16().collect();
    let err = |e: windows::core::Error| e.message();

    unsafe {
        let session = Handle::new(WinHttpOpen(
            agent.pcwstr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        // 名前解決・接続・送信・受信のタイムアウト（ミリ秒）
        WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 60_000).map_err(err)?;
        let connect = Handle::new(WinHttpConnect(session.0, host.pcwstr(), INTERNET_DEFAULT_HTTPS_PORT, 0))?;
        let request = Handle::new(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            path.pcwstr(),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).map_err(err)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(err)?;

        let mut status = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut _),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(err)?;
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }

        let mut body = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read).map_err(err)?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buf[..read as usize]);
            if body.len() > 200 * 1024 * 1024 {
                return Err("response too large".into());
            }
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("v1.2.3"), Some([1, 2, 3]));
        assert_eq!(parse_version("1.2"), Some([1, 2, 0]));
        assert_eq!(parse_version("1.2.3-beta"), Some([1, 2, 3]));
        assert_eq!(parse_version("abc"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("v1.2.1", "1.2.0"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("1.2.0", "1.2.0"));
        assert!(!is_newer("1.1.9", "1.2.0"));
        assert!(!is_newer("garbage", "1.0.0"));
    }

    #[test]
    fn parses_github_release_response() {
        let json = r#"{
            "tag_name": "v1.3.0",
            "html_url": "https://github.com/Shun4211/NowPlayingNotifier/releases/tag/v1.3.0",
            "assets": [
                { "name": "NowPlayingNotifier-1.3.0-win-x64.zip", "browser_download_url": "https://example.com/a.zip" },
                { "name": "NowPlayingNotifier-1.3.0-win-x64-setup.exe", "browser_download_url": "https://example.com/setup.exe" }
            ]
        }"#;
        let r = parse_release(json).unwrap();
        assert_eq!(r.version, "1.3.0");
        assert_eq!(r.page_url, "https://github.com/Shun4211/NowPlayingNotifier/releases/tag/v1.3.0");
        assert_eq!(r.installer_url.as_deref(), Some("https://example.com/setup.exe"));
    }

    #[test]
    fn release_without_installer_or_with_bad_tag() {
        let r = parse_release(r#"{ "tag_name": "v2.0.0", "assets": [] }"#).unwrap();
        assert_eq!(r.installer_url, None);
        assert_eq!(r.page_url, RELEASES_PAGE);
        assert!(parse_release(r#"{ "tag_name": "nightly" }"#).is_err());
        assert!(parse_release("not json").is_err());
    }

    /// 実際に GitHub に問い合わせる（ネットワークが必要なので `cargo test -- --ignored` で実行）。
    #[test]
    #[ignore]
    fn fetches_latest_release_from_github() {
        let body = http_get(LATEST_RELEASE_API, "Accept: application/vnd.github+json").unwrap();
        let r = parse_release(&String::from_utf8_lossy(&body)).unwrap();
        assert!(parse_version(&r.version).is_some());
        let url = r.installer_url.expect("release has an installer");
        let setup = http_get(&url, "Accept: application/octet-stream").unwrap();
        assert!(setup.starts_with(b"MZ") && setup.len() > 100 * 1024);
    }

    #[test]
    fn splits_only_https_urls() {
        assert_eq!(split_https_url("https://api.github.com/repos/x"), Some(("api.github.com", "/repos/x")));
        assert_eq!(split_https_url("https://example.com"), Some(("example.com", "/")));
        assert_eq!(split_https_url("http://example.com/"), None);
        assert_eq!(split_https_url("https://user@example.com/"), None);
    }
}
