//! OS との連携: 二重起動の防止、自動起動、全画面判定、再生元アプリの前面化、モニター一覧。

use std::ffi::c_void;

use windows::core::{w, Result, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Shell::{SHQueryUserNotificationState, ShellExecuteW, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::position::Rect;
use crate::win::{post_event, wide, AppEvent};

pub const APP_NAME: &str = "NowPlayingNotifier";
/// 以前の名前。設定と自動起動の登録を引き継ぐために使う。
pub const LEGACY_APP_NAME: &str = "OtherPlayingNotifier";

// ---------------------------------------------------------------------------
// 二重起動の防止（名前は C# 版・インストーラーの AppMutex と同じ）

pub struct SingleInstance {
    mutex: HANDLE,
    event: HANDLE,
    wait: HANDLE,
}

impl SingleInstance {
    /// 最初のインスタンスなら Some。2つ目以降なら1つ目に表示を依頼して None を返す。
    pub fn acquire() -> Option<Self> {
        unsafe {
            let mutex = CreateMutexW(None, true, w!("Local\\NowPlayingNotifier.SingleInstance")).ok()?;
            let already = GetLastError() == ERROR_ALREADY_EXISTS;
            let event = CreateEventW(None, false, false, w!("Local\\NowPlayingNotifier.Activate")).ok()?;
            if already {
                let _ = SetEvent(event);
                let _ = CloseHandle(event);
                let _ = CloseHandle(mutex);
                return None;
            }
            Some(Self { mutex, event, wait: HANDLE::default() })
        }
    }

    /// 後から起動されたインスタンスからの依頼を待ち受ける。
    pub fn listen(&mut self) {
        unsafe extern "system" fn on_signal(_: *mut c_void, _: BOOLEAN) {
            post_event(AppEvent::ActivateRequested);
        }
        unsafe {
            let _ = RegisterWaitForSingleObject(
                &mut self.wait,
                self.event,
                Some(on_signal),
                None,
                INFINITE,
                WT_EXECUTEDEFAULT,
            );
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            if !self.wait.is_invalid() {
                let _ = UnregisterWaitEx(self.wait, INVALID_HANDLE_VALUE);
            }
            let _ = CloseHandle(self.event);
            let _ = ReleaseMutex(self.mutex);
            let _ = CloseHandle(self.mutex);
        }
    }
}

// ---------------------------------------------------------------------------
// Windows 起動時の自動起動（HKCU\...\Run）

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

fn exe_path() -> Option<String> {
    std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned())
}

/// 自動起動の登録・解除を行う。登録済みなら実行ファイルのパスも最新にする。
pub fn apply_startup_registration(enabled: bool) -> Result<()> {
    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, None, REG_OPTION_NON_VOLATILE, KEY_READ | KEY_WRITE, None, &mut key, None).ok()?;
        // 旧名で登録した自動起動は、古い exe を指しているので消す（必要なら新しい名前で登録し直す）
        let legacy = wide(LEGACY_APP_NAME);
        let _ = RegDeleteValueW(key, legacy.pcwstr());

        let name = wide(APP_NAME);
        let result = if enabled {
            let path = exe_path().ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))?;
            let command = format!("\"{path}\"");
            if read_string(key, &name) .as_deref() != Some(command.as_str()) {
                let data: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
                let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
                RegSetValueExW(key, name.pcwstr(), 0, REG_SZ, Some(bytes)).ok()
            } else {
                Ok(())
            }
        } else {
            match RegDeleteValueW(key, name.pcwstr()) {
                e if e == ERROR_FILE_NOT_FOUND => Ok(()),
                e => e.ok(),
            }
        };
        let _ = RegCloseKey(key);
        result
    }
}

unsafe fn read_string(key: HKEY, name: &crate::win::WideString) -> Option<String> {
    let mut size = 0u32;
    let mut kind = REG_VALUE_TYPE::default();
    RegQueryValueExW(key, name.pcwstr(), None, Some(&mut kind), None, Some(&mut size)).ok().ok()?;
    if kind != REG_SZ {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    RegQueryValueExW(key, name.pcwstr(), None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut size)).ok().ok()?;
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

// ---------------------------------------------------------------------------

/// 全画面アプリやプレゼンテーション中など、通知を控えるべき状態か。
pub fn is_fullscreen_app_running() -> bool {
    match unsafe { SHQueryUserNotificationState() } {
        Ok(state) => state == QUNS_BUSY || state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE,
        Err(_) => false,
    }
}

/// メディアセッションの SourceAppUserModelId から再生元アプリを前面に出す。
pub fn activate_source_app(app_id: &str) {
    let app_id = app_id.trim();
    if app_id.is_empty() {
        return;
    }

    // ストアアプリなどの AUMID（例: "SpotifyAB.SpotifyMusic_xxx!Spotify"）
    if app_id.contains('!') {
        let args = wide(&format!("shell:AppsFolder\\{app_id}"));
        unsafe {
            ShellExecuteW(None, w!("open"), w!("explorer.exe"), args.pcwstr(), PCWSTR::null(), SW_SHOWNORMAL);
        }
        return;
    }

    // デスクトップアプリ（例: "Spotify.exe", "chrome"）
    let stem = std::path::Path::new(app_id)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if let Some(hwnd) = find_main_window(&stem) {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

/// 指定した実行ファイル名（拡張子なし・小文字）のプロセスが持つ、表示中で所有者のないトップレベルウィンドウ。
fn find_main_window(process_stem: &str) -> Option<HWND> {
    struct Search<'a> {
        stem: &'a str,
        found: Option<HWND>,
    }

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut Search);
        if !IsWindowVisible(hwnd).as_bool() || !GetWindow(hwnd, GW_OWNER).unwrap_or_default().is_invalid() {
            return TRUE;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if process_stem_of(pid).as_deref() == Some(search.stem) {
            search.found = Some(hwnd);
            return FALSE;
        }
        TRUE
    }

    let mut search = Search { stem: process_stem, found: None };
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut search as *mut _ as isize));
    }
    search.found
}

fn process_stem_of(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(process);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_lowercase())
    }
}

// ---------------------------------------------------------------------------
// モニター

#[derive(Clone, Debug)]
pub struct MonitorInfo {
    pub device_name: String,
    pub bounds: Rect,
    pub work_area: Rect,
    pub is_primary: bool,
    pub dpi: u32,
}

fn to_rect(r: RECT) -> Rect {
    Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

/// 接続中のモニター一覧（OS が列挙する順）。
pub fn monitors() -> Vec<MonitorInfo> {
    unsafe extern "system" fn callback(hmon: HMONITOR, _: HDC, _: *mut RECT, lparam: LPARAM) -> BOOL {
        let list = &mut *(lparam.0 as *mut Vec<MonitorInfo>);
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(hmon, &mut info as *mut _ as *mut MONITORINFO).as_bool() {
            let len = info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len());
            let (mut dx, mut dy) = (96u32, 96u32);
            let _ = GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            list.push(MonitorInfo {
                device_name: String::from_utf16_lossy(&info.szDevice[..len]),
                bounds: to_rect(info.monitorInfo.rcMonitor),
                work_area: to_rect(info.monitorInfo.rcWork),
                is_primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                dpi: dx,
            });
        }
        TRUE
    }

    let mut list: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(callback), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

/// 指定した名前のモニター。見つからなければメインモニター。
pub fn monitor_or_primary(device_name: &str) -> Option<MonitorInfo> {
    let list = monitors();
    list.iter()
        .find(|m| !device_name.is_empty() && m.device_name == device_name)
        .or_else(|| list.iter().find(|m| m.is_primary))
        .or(list.first())
        .cloned()
}
