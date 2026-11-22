//! タスクトレイのアイコン・メニュー・バルーン通知。

use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{LoadIconMetric, LIM_SMALL};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::strings;
use crate::win::{copy_to_buffer, wide};

pub const WM_TRAY: u32 = WM_APP + 2;
const TRAY_ID: u32 = 1;
const TOOLTIP: &str = "Now Playing Notifier";

pub const CMD_SHOW: usize = 1;
pub const CMD_SETTINGS: usize = 2;
pub const CMD_EXIT: usize = 3;

fn base_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    }
}

fn load_icon() -> HICON {
    unsafe {
        // exe に埋め込んだアイコン（リソース ID 1）。無ければ標準のアプリアイコン。
        GetModuleHandleW(None)
            .ok()
            .and_then(|m| LoadIconMetric(HINSTANCE(m.0), crate::win::APP_ICON, LIM_SMALL).ok())
            .or_else(|| LoadIconW(None, IDI_APPLICATION).ok())
            .unwrap_or_default()
    }
}

/// トレイにアイコンを追加する（エクスプローラーの再起動後に呼び直してもよい）。
pub fn add(hwnd: HWND) {
    let mut data = base_data(hwnd);
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = WM_TRAY;
    data.hIcon = load_icon();
    copy_to_buffer(&mut data.szTip, TOOLTIP);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &data);
    }
}

pub fn remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &base_data(hwnd));
    }
}

/// トレイからバルーン通知でエラーなどを知らせる。
pub fn show_balloon(hwnd: HWND, title: &str, message: &str) {
    let mut data = base_data(hwnd);
    data.uFlags = NIF_INFO;
    data.dwInfoFlags = NIIF_WARNING;
    copy_to_buffer(&mut data.szInfoTitle, title);
    copy_to_buffer(&mut data.szInfo, if message.trim().is_empty() { title } else { message });
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

/// 右クリックメニューを表示し、選ばれたコマンドを返す。
pub fn show_menu(hwnd: HWND) -> Option<usize> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let items = [
            (CMD_SHOW, strings::tray_show()),
            (CMD_SETTINGS, strings::tray_settings()),
            (0, ""),
            (CMD_EXIT, strings::tray_exit()),
        ];
        for (id, text) in items {
            if id == 0 {
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            } else {
                let text = wide(text);
                let _ = AppendMenuW(menu, MF_STRING, id, text.pcwstr());
            }
        }

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // メニューの外をクリックしたときに閉じるよう、先に前面にしておく
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, None);
        let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        (cmd.0 != 0).then_some(cmd.0 as usize)
    }
}
