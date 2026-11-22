//! 再生中の曲を表示するウィンドウ（トレイの「表示」で開く）。× や最小化ではトレイに隠すだけ。

use std::cell::RefCell;

use windows::core::{w, Result};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::win::{message_font, scale, wide, window_dpi};

const WIDTH: i32 = 400;
const HEIGHT: i32 = 200;
const MARGIN: i32 = 20;
const TITLE_SIZE: i32 = 24;
const ARTIST_SIZE: i32 = 18;
const LINE_GAP: i32 = 10;

struct State {
    hwnd: HWND,
    title: String,
    artist: String,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn create() -> Result<HWND> {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hIcon: LoadIconW(hinstance, crate::win::APP_ICON).unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize as *mut _),
            lpszClassName: w!("OtherPlayingNotifier.Main"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let dpi = GetDpiForSystem();
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("OtherPlayingNotifier.Main"),
            w!("Now Playing"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            scale(WIDTH, dpi),
            scale(HEIGHT, dpi),
            None,
            None,
            hinstance,
            None,
        )?;
        STATE.with(|s| *s.borrow_mut() = Some(State { hwnd, title: String::new(), artist: String::new() }));
        Ok(hwnd)
    }
}

pub fn hwnd() -> Option<HWND> {
    STATE.with(|s| s.try_borrow().ok()?.as_ref().map(|s| s.hwnd))
}

pub fn set_text(title: &str, artist: &str) {
    let hwnd = STATE.with(|s| {
        let mut s = s.try_borrow_mut().ok()?;
        let s = s.as_mut()?;
        s.title = title.to_string();
        s.artist = artist.to_string();
        Some(s.hwnd)
    });
    if let Some(hwnd) = hwnd {
        unsafe {
            let _ = InvalidateRect(hwnd, None, TRUE);
        }
    }
}

pub fn show() {
    if let Some(hwnd) = hwnd() {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            } else {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

pub fn is_visible() -> bool {
    hwnd().is_some_and(|h| unsafe { IsWindowVisible(h).as_bool() })
}

pub fn destroy() {
    if let Some(s) = STATE.with(|s| s.borrow_mut().take()) {
        unsafe {
            let _ = DestroyWindow(s.hwnd);
        }
    }
}

fn font(dpi: u32, size: i32, bold: bool) -> HFONT {
    let mut lf = message_font(dpi).unwrap_or_default();
    lf.lfHeight = -scale(size, dpi);
    lf.lfWeight = if bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 };
    unsafe { CreateFontIndirectW(&lf) }
}

unsafe fn paint(hwnd: HWND, hdc: HDC) {
    let (title, artist) = STATE.with(|s| {
        s.try_borrow().ok().and_then(|s| s.as_ref().map(|s| (s.title.clone(), s.artist.clone()))).unwrap_or_default()
    });
    let dpi = window_dpi(hwnd);
    let mut client = RECT::default();
    let _ = GetClientRect(hwnd, &mut client);
    let margin = scale(MARGIN, dpi);
    let left = client.left + margin;
    let right = client.right - margin;

    let title_font = font(dpi, TITLE_SIZE, true);
    let artist_font = font(dpi, ARTIST_SIZE, false);
    let original_font = SelectObject(hdc, title_font);
    let line_height = |f: HFONT| {
        SelectObject(hdc, f);
        let mut tm = TEXTMETRICW::default();
        let _ = GetTextMetricsW(hdc, &mut tm);
        tm.tmHeight
    };
    let th = line_height(title_font);
    let ah = line_height(artist_font);
    let gap = scale(LINE_GAP, dpi);
    let top = client.top + ((client.bottom - client.top) - (th + gap + ah)) / 2;

    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, COLORREF(GetSysColor(COLOR_WINDOWTEXT)));
    let flags = DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX | DT_LEFT;

    // 空文字列は描かない（アーティスト名がない曲・待機中など）
    let draw = |text: &str, font: HFONT, mut r: RECT| {
        if text.is_empty() {
            return;
        }
        SelectObject(hdc, font);
        DrawTextW(hdc, wide(text).as_mut_slice(), &mut r, flags);
    };
    draw(&title, title_font, RECT { left, top, right, bottom: top + th });
    draw(&artist, artist_font, RECT { left, top: top + th + gap, right, bottom: top + th + gap + ah });

    // 選択中のフォントは削除できないので、元のフォントに戻してから削除する
    SelectObject(hdc, original_font);
    let _ = DeleteObject(title_font);
    let _ = DeleteObject(artist_font);
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            paint(hwnd, hdc);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_PRINTCLIENT => {
            let hdc = HDC(wparam.0 as *mut _);
            let mut r = RECT::default();
            let _ = GetClientRect(hwnd, &mut r);
            FillRect(hdc, &r, GetSysColorBrush(COLOR_WINDOW));
            paint(hwnd, hdc);
            LRESULT(0)
        }
        // × ボタンではアプリを終了せず、トレイに隠すだけにする
        WM_CLOSE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_SIZE if wparam.0 as u32 == SIZE_MINIMIZED => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let r = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
