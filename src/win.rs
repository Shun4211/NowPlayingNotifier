//! Win32 まわりの小さな共通処理と、別スレッドからメインスレッドへのイベント受け渡し。

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::LOGFONTW;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, SystemParametersInfoForDpi};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS, WM_APP};

use crate::media::MediaEvent;

/// exe に埋め込んだアイコンのリソース ID（MAKEINTRESOURCE(1)）。
pub const APP_ICON: PCWSTR = PCWSTR(std::ptr::without_provenance(1));

/// UTF-16 の null 終端文字列。`PCWSTR` を作るあいだ生きている必要がある。
pub struct WideString(Vec<u16>);

impl WideString {
    pub fn new(s: &str) -> Self {
        Self(s.encode_utf16().chain(std::iter::once(0)).collect())
    }

    pub fn pcwstr(&self) -> PCWSTR {
        PCWSTR(self.0.as_ptr())
    }

    /// 終端の null を除いた文字列部分。長さ・ポインタの組で渡す API（DrawTextW、CreateTextLayout など）用。
    /// 空文字列でもバッファ自体は null 1文字分あるので、ポインタは常に有効なメモリを指す。
    /// （空の `Vec<u16>` のポインタは 0x2 などのダミー値で、API によってはそれを読みに行って落ちる）
    pub fn as_slice(&self) -> &[u16] {
        &self.0[..self.0.len() - 1]
    }

    pub fn as_mut_slice(&mut self) -> &mut [u16] {
        let len = self.0.len() - 1;
        &mut self.0[..len]
    }
}

pub fn wide(s: &str) -> WideString {
    WideString::new(s)
}

/// 固定長の UTF-16 バッファ（NOTIFYICONDATAW の szTip など）に、収まる分だけ書き込む。
pub fn copy_to_buffer(buf: &mut [u16], s: &str) {
    let max = buf.len().saturating_sub(1);
    let mut n = 0;
    for (i, c) in s.encode_utf16().take(max).enumerate() {
        buf[i] = c;
        n = i + 1;
    }
    // サロゲートペアの途中で切れたら、その半端な文字を落とす
    if n > 0 && (0xD800..0xDC00).contains(&buf[n - 1]) {
        n -= 1;
    }
    buf[n..].fill(0);
}

/// 96 DPI 基準の値を、指定した DPI のピクセル数に変換する。
pub fn scale(value: i32, dpi: u32) -> i32 {
    (value as i64 * dpi as i64 / 96) as i32
}

pub fn window_dpi(hwnd: HWND) -> u32 {
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => 96,
        dpi => dpi,
    }
}

/// OS の UI フォント（日本語環境なら Yu Gothic UI、英語環境なら Segoe UI など）の名前。
pub fn ui_font_face() -> String {
    message_font(96)
        .map(|lf| {
            let len = lf.lfFaceName.iter().position(|&c| c == 0).unwrap_or(lf.lfFaceName.len());
            String::from_utf16_lossy(&lf.lfFaceName[..len])
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Segoe UI".to_string())
}

/// 指定した DPI でのメッセージボックス用フォント（OS の標準 UI フォント）。
pub fn message_font(dpi: u32) -> Option<LOGFONTW> {
    let mut metrics = NONCLIENTMETRICSW { cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
    unsafe {
        SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            metrics.cbSize,
            Some(&mut metrics as *mut _ as *mut _),
            0,
            dpi,
        )
        .ok()?;
    }
    Some(metrics.lfMessageFont)
}

pub fn loword(v: usize) -> u32 {
    (v & 0xFFFF) as u32
}

pub fn hiword(v: usize) -> u32 {
    ((v >> 16) & 0xFFFF) as u32
}

// ---------------------------------------------------------------------------
// 別スレッド（WinRT のイベント、ワーカースレッド、二重起動の通知）からメインスレッドへのイベント

pub const WM_APP_EVENT: u32 = WM_APP + 1;

pub enum AppEvent {
    Media(MediaEvent),
    /// 後から起動されたインスタンスがウィンドウの表示を依頼した。
    ActivateRequested,
}

static EVENT_TARGET: AtomicIsize = AtomicIsize::new(0);
static EVENTS: Mutex<Vec<AppEvent>> = Mutex::new(Vec::new());

/// イベントを受け取るウィンドウ（メインスレッドのメッセージループで処理される）を設定する。
pub fn set_event_target(hwnd: HWND) {
    EVENT_TARGET.store(hwnd.0 as isize, Ordering::SeqCst);
}

/// どのスレッドからでも呼べる。メインスレッドに WM_APP_EVENT を送って処理させる。
pub fn post_event(event: AppEvent) {
    let Ok(mut queue) = EVENTS.lock() else { return };
    queue.push(event);
    let target = EVENT_TARGET.load(Ordering::SeqCst);
    // まだ処理されていない通知がある場合は送らなくてよい
    if queue.len() == 1 && target != 0 {
        unsafe {
            let _ = PostMessageW(HWND(target as *mut _), WM_APP_EVENT, WPARAM(0), LPARAM(0));
        }
    }
}

pub fn take_events() -> Vec<AppEvent> {
    EVENTS.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_to_buffer_truncates_and_terminates() {
        let mut buf = [0xFFFFu16; 4];
        copy_to_buffer(&mut buf, "abcdef");
        assert_eq!(buf, [b'a' as u16, b'b' as u16, b'c' as u16, 0]);
    }

    #[test]
    fn copy_to_buffer_drops_half_surrogate_pair() {
        let mut buf = [0xFFFFu16; 3];
        copy_to_buffer(&mut buf, "a🎵");
        assert_eq!(buf, [b'a' as u16, 0, 0]);
    }

    #[test]
    fn wide_slice_excludes_nul_and_points_to_real_memory_when_empty() {
        let w = wide("ab");
        assert_eq!(w.as_slice(), &[b'a' as u16, b'b' as u16]);
        let empty = wide("");
        assert!(empty.as_slice().is_empty());
        // 空でもバッファ（null 1文字）を指していて、ダミーのポインタではない
        assert_eq!(empty.as_slice().as_ptr(), empty.pcwstr().0);
    }

    #[test]
    fn scale_converts_from_96_dpi() {
        assert_eq!(scale(20, 96), 20);
        assert_eq!(scale(20, 144), 30);
        assert_eq!(scale(-20, 192), -40);
    }
}
