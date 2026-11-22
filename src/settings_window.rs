//! 設定画面。Win32 の標準コントロールで組み立てる。
//! レイアウトは 96 DPI 基準の座標で持っておき、DPI に合わせて拡大して配置する。

use std::cell::RefCell;

use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::Input::Ime::{ImmAssociateContextEx, ImmGetVirtualKey, HIMC};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::hotkey::{HotkeyBinding, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN};
use crate::settings::{PopupPosition, Settings, TaskbarLayout, TaskbarPaused, TaskbarPosition, Theme};
use crate::strings::{self, Language};
use crate::system::monitors;
use crate::win::{hiword, loword, message_font, scale, wide, window_dpi};

// コントロールのメッセージ・スタイル（commctrl.h / winuser.h）
const TBM_GETPOS: u32 = WM_USER;
const TBM_SETPOS: u32 = WM_USER + 5;
const TBM_SETRANGEMIN: u32 = WM_USER + 7;
const TBM_SETRANGEMAX: u32 = WM_USER + 8;
const TBS_NOTICKS: u32 = 0x0010;
const CB_ADDSTRING: u32 = 0x0143;
const CB_GETCURSEL: u32 = 0x0147;
const CB_SETCURSEL: u32 = 0x014E;
const CBS_DROPDOWNLIST: u32 = 0x0003;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BS_AUTORADIOBUTTON: u32 = 0x0009;
const BS_PUSHBUTTON: u32 = 0x0000;
const BS_DEFPUSHBUTTON: u32 = 0x0001;
const ES_AUTOHSCROLL: u32 = 0x0080;
const ES_READONLY: u32 = 0x0800;
const SS_RIGHT: u32 = 0x0002;
const SS_CENTERIMAGE: u32 = 0x0200;
const SS_NOPREFIX: u32 = 0x0080;
const DM_GETDEFID: u32 = WM_USER;
const DC_HASDEFID: usize = 0x534B;
const DLGC_WANTALLKEYS: isize = 0x0004;

const ID_PREVIEW: i32 = 100;
const ID_CHECK_UPDATE: i32 = 101;
const ID_SAVE: i32 = IDOK.0;
const ID_CANCEL: i32 = IDCANCEL.0;

const CLIENT_WIDTH: i32 = 1120;
const BACKGROUND: u32 = 0x00F5F5F5;
const GRAY_TEXT: u32 = 0x00666666;

fn colorref(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

#[derive(Clone, Copy, PartialEq)]
enum FontKind {
    Normal,
    Header,
    Small,
}

/// 96 DPI 基準の配置。
struct Item {
    hwnd: HWND,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font: FontKind,
}

#[derive(Default)]
struct Controls {
    position: [HWND; 4],
    monitor: HWND,
    width: HWND,
    height: HWND,
    margin: HWND,
    duration: HWND,
    opacity: HWND,
    width_value: HWND,
    height_value: HWND,
    margin_value: HWND,
    duration_value: HWND,
    opacity_value: HWND,
    theme_dark: HWND,
    theme_light: HWND,
    album_art: HWND,
    fullscreen: HWND,
    startup: HWND,
    check_updates: HWND,
    taskbar: HWND,
    taskbar_position: [HWND; 2],
    taskbar_layout: HWND,
    taskbar_paused: [HWND; 2],
    taskbar_width: HWND,
    taskbar_font: HWND,
    taskbar_offset: HWND,
    taskbar_width_value: HWND,
    taskbar_font_value: HWND,
    taskbar_offset_value: HWND,
    taskbar_artist: HWND,
    taskbar_art: HWND,
    hint: HWND,
    hotkey_boxes: [HWND; 3],
    hotkey_error: HWND,
    language: HWND,
    version: HWND,
}

pub struct Callbacks {
    pub preview: fn(Settings),
    /// 保存したら Some(新しい設定)、キャンセルしたら None。
    pub closed: fn(Option<Settings>),
    /// 「更新を確認」が押された。
    pub check_update: fn(),
}

struct State {
    hwnd: HWND,
    owner: Option<HWND>,
    original: Settings,
    hotkeys: [HotkeyBinding; 3],
    monitor_names: Vec<String>,
    controls: Controls,
    items: Vec<Item>,
    fonts: Vec<(FontKind, HFONT)>,
    background: HBRUSH,
    callbacks: Callbacks,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub fn hwnd() -> Option<HWND> {
    STATE.with(|s| s.try_borrow().ok().and_then(|s| s.as_ref().map(|s| s.hwnd)))
}

/// 設定画面を開く。既に開いていれば前面に出す。
pub fn open(owner: Option<HWND>, settings: Settings, callbacks: Callbacks) -> Result<()> {
    if let Some(hwnd) = hwnd() {
        unsafe {
            let _ = SetForegroundWindow(hwnd);
        }
        return Ok(());
    }

    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hIcon: LoadIconW(hinstance, crate::win::APP_ICON).unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: w!("NowPlayingNotifier.Settings"),
            ..Default::default()
        };
        RegisterClassW(&class);

        let title = wide(strings::settings_title());
        let style = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT | WS_EX_DLGMODALFRAME,
            w!("NowPlayingNotifier.Settings"),
            title.pcwstr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            100,
            100,
            owner.unwrap_or_default(),
            None,
            hinstance,
            None,
        )?;

        let mut state = State {
            hwnd,
            owner,
            hotkeys: [settings.play_pause_hotkey, settings.next_track_hotkey, settings.previous_track_hotkey],
            original: settings,
            monitor_names: Vec::new(),
            controls: Controls::default(),
            items: Vec::new(),
            fonts: Vec::new(),
            background: CreateSolidBrush(COLORREF(BACKGROUND)),
            callbacks,
        };
        let content_height = build_controls(&mut state, hinstance);
        load_values(&mut state);
        STATE.with(|s| *s.borrow_mut() = Some(state));

        let dpi = window_dpi(hwnd);
        apply_layout(dpi);
        update_value_labels();
        update_hotkey_boxes();

        // ウィンドウの大きさと位置（オーナーの中央、なければ画面中央）
        let (w, h) = window_size(dpi, content_height);
        let center = match owner.filter(|o| IsWindowVisible(*o).as_bool()) {
            Some(o) => {
                let mut r = RECT::default();
                let _ = GetWindowRect(o, &mut r);
                ((r.left + r.right) / 2, (r.top + r.bottom) / 2)
            }
            None => {
                let mut r = RECT::default();
                let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut r as *mut _ as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
                ((r.left + r.right) / 2, (r.top + r.bottom) / 2)
            }
        };
        let _ = SetWindowPos(hwnd, None, center.0 - w / 2, (center.1 - h / 2).max(0), w, h, SWP_NOZORDER);
        CONTENT_HEIGHT.with(|c| c.set(content_height));

        if let Some(o) = owner {
            let _ = EnableWindow(o, false);
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    Ok(())
}

thread_local! {
    static CONTENT_HEIGHT: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

fn window_size(dpi: u32, content_height: i32) -> (i32, i32) {
    let mut r = RECT { left: 0, top: 0, right: scale(CLIENT_WIDTH, dpi), bottom: scale(content_height, dpi) };
    unsafe {
        let _ = AdjustWindowRectExForDpi(
            &mut r,
            WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            false,
            WS_EX_CONTROLPARENT | WS_EX_DLGMODALFRAME,
            dpi,
        );
    }
    (r.right - r.left, r.bottom - r.top)
}

// ---------------------------------------------------------------------------
// コントロールの作成

struct Builder<'a> {
    state: &'a mut State,
    hinstance: HINSTANCE,
}

impl Builder<'_> {
    #[allow(clippy::too_many_arguments)]
    fn add(&mut self, class: PCWSTR, text: &str, style: u32, id: i32, x: i32, y: i32, w: i32, h: i32, font: FontKind) -> HWND {
        let text = wide(text);
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                text.pcwstr(),
                WINDOW_STYLE(style) | WS_CHILD | WS_VISIBLE,
                0, 0, 0, 0,
                self.state.hwnd,
                HMENU(id as isize as *mut _),
                self.hinstance,
                None,
            )
            .unwrap_or_default()
        };
        self.state.items.push(Item { hwnd, x, y, w, h, font });
        hwnd
    }

    fn header(&mut self, text: &str, x: i32, y: &mut i32, w: i32) {
        self.add(w!("STATIC"), text, SS_NOPREFIX, -1, x, *y, w, 22, FontKind::Header);
        *y += 28;
    }

    #[allow(clippy::too_many_arguments)]
    fn label(&mut self, text: &str, x: i32, y: i32, w: i32, h: i32, extra: u32, font: FontKind) -> HWND {
        self.add(w!("STATIC"), text, SS_NOPREFIX | extra, -1, x, y, w, h, font)
    }

    fn radio(&mut self, text: &str, first: bool, x: i32, y: i32, w: i32) -> HWND {
        let group = if first { WS_GROUP.0 | WS_TABSTOP.0 } else { 0 };
        self.add(w!("BUTTON"), text, BS_AUTORADIOBUTTON | group, -1, x, y, w, 24, FontKind::Normal)
    }

    fn check(&mut self, text: &str, x: i32, y: i32, w: i32) -> HWND {
        self.add(w!("BUTTON"), text, BS_AUTOCHECKBOX | WS_GROUP.0 | WS_TABSTOP.0, -1, x, y, w, 24, FontKind::Normal)
    }

    fn slider(&mut self, x: i32, y: i32, w: i32, min: i32, max: i32) -> HWND {
        let hwnd = self.add(w!("msctls_trackbar32"), "", TBS_NOTICKS | WS_GROUP.0 | WS_TABSTOP.0, -1, x, y, w, 28, FontKind::Normal);
        unsafe {
            SendMessageW(hwnd, TBM_SETRANGEMIN, WPARAM(0), LPARAM(min as isize));
            SendMessageW(hwnd, TBM_SETRANGEMAX, WPARAM(1), LPARAM(max as isize));
        }
        hwnd
    }

    fn combo(&mut self, x: i32, y: i32, w: i32, items: &[String]) -> HWND {
        let hwnd = self.add(
            w!("COMBOBOX"),
            "",
            CBS_DROPDOWNLIST | WS_VSCROLL.0 | WS_GROUP.0 | WS_TABSTOP.0,
            -1,
            x,
            y,
            w,
            240,
            FontKind::Normal,
        );
        for item in items {
            let text = wide(item);
            unsafe {
                SendMessageW(hwnd, CB_ADDSTRING, WPARAM(0), LPARAM(text.pcwstr().0 as isize));
            }
        }
        hwnd
    }

    fn button(&mut self, text: &str, id: i32, default: bool, x: i32, y: i32, w: i32) -> HWND {
        let style = if default { BS_DEFPUSHBUTTON } else { BS_PUSHBUTTON } | WS_GROUP.0 | WS_TABSTOP.0;
        self.add(w!("BUTTON"), text, style, id, x, y, w, 32, FontKind::Normal)
    }
}

/// コントロールを作り、内容の高さ（96 DPI 基準）を返す。
fn build_controls(state: &mut State, hinstance: HINSTANCE) -> i32 {
    // 先に選択肢を作っておく
    let mut monitor_items = vec![strings::primary_monitor().to_string()];
    let mut monitor_names = vec![String::new()];
    for (i, m) in monitors().iter().enumerate() {
        monitor_items.push(strings::monitor_item(
            i + 1,
            m.bounds.right - m.bounds.left,
            m.bounds.bottom - m.bounds.top,
            m.is_primary,
        ));
        monitor_names.push(m.device_name.clone());
    }
    state.monitor_names = monitor_names;

    let mut b = Builder { state, hinstance };
    let mut c = Controls::default();

    // 左の列
    let x = 20;
    let mut y = 16;
    b.header(strings::position_header(), x, &mut y, 340);
    c.position[0] = b.radio(strings::top_left(), true, x, y, 160);
    c.position[1] = b.radio(strings::top_right(), false, x + 170, y, 160);
    c.position[2] = b.radio(strings::bottom_left(), false, x, y + 28, 160);
    c.position[3] = b.radio(strings::bottom_right(), false, x + 170, y + 28, 160);
    y += 28 * 2 + 12;

    b.header(strings::monitor_header(), x, &mut y, 340);
    c.monitor = b.combo(x, y, 330, &monitor_items);
    y += 28 + 16;

    b.header(strings::size_header(), x, &mut y, 340);
    let rows: [(&str, i32, i32); 3] = [
        (strings::width(), settings_range::WIDTH.0, settings_range::WIDTH.1),
        (strings::height(), settings_range::HEIGHT.0, settings_range::HEIGHT.1),
        (strings::margin(), settings_range::MARGIN.0, settings_range::MARGIN.1),
    ];
    let mut sliders = [HWND::default(); 3];
    let mut values = [HWND::default(); 3];
    for (i, (label, min, max)) in rows.into_iter().enumerate() {
        b.label(label, x, y, 110, 28, SS_CENTERIMAGE, FontKind::Normal);
        sliders[i] = b.slider(x + 110, y, 170, min, max);
        values[i] = b.label("", x + 280, y, 60, 28, SS_RIGHT | SS_CENTERIMAGE, FontKind::Normal);
        y += 34;
    }
    [c.width, c.height, c.margin] = sliders;
    [c.width_value, c.height_value, c.margin_value] = values;
    y += 10;

    b.header(strings::duration_header(), x, &mut y, 340);
    c.duration = b.slider(x, y, 280, settings_range::DURATION.0, settings_range::DURATION.1);
    c.duration_value = b.label("", x + 280, y, 60, 28, SS_RIGHT | SS_CENTERIMAGE, FontKind::Normal);
    y += 34 + 10;

    b.header(strings::opacity_header(), x, &mut y, 340);
    c.opacity = b.slider(x, y, 280, settings_range::OPACITY.0, settings_range::OPACITY.1);
    c.opacity_value = b.label("", x + 280, y, 60, 28, SS_RIGHT | SS_CENTERIMAGE, FontKind::Normal);
    y += 34;
    let left_bottom = y;

    // 右の列
    let x = 390;
    let mut y = 16;
    b.header(strings::theme_header(), x, &mut y, 350);
    c.theme_dark = b.radio(strings::theme_dark(), true, x, y, 130);
    c.theme_light = b.radio(strings::theme_light(), false, x + 140, y, 130);
    y += 28 + 16;

    b.header(strings::behavior_header(), x, &mut y, 350);
    c.album_art = b.check(strings::show_album_art(), x, y, 350);
    c.fullscreen = b.check(strings::suppress_in_fullscreen(), x, y + 28, 350);
    c.startup = b.check(strings::start_with_windows(), x, y + 56, 350);
    c.check_updates = b.check(strings::check_for_updates(), x, y + 84, 350);
    y += 28 * 4 + 16;

    b.header(strings::hotkeys_header(), x, &mut y, 350);
    c.hint = b.label(strings::hotkey_hint(), x, y, 350, 34, 0, FontKind::Small);
    y += 40;
    for (i, label) in [strings::hotkey_play_pause(), strings::hotkey_next(), strings::hotkey_previous()].into_iter().enumerate() {
        b.label(label, x, y, 115, 26, SS_CENTERIMAGE, FontKind::Normal);
        c.hotkey_boxes[i] = b.add(
            w!("EDIT"),
            "",
            ES_READONLY | ES_AUTOHSCROLL | WS_BORDER.0 | WS_GROUP.0 | WS_TABSTOP.0,
            -1,
            x + 115,
            y,
            220,
            26,
            FontKind::Normal,
        );
        y += 32;
    }
    c.hotkey_error = b.label("", x, y, 350, 34, 0, FontKind::Small);
    y += 38;

    b.header(strings::language_header(), x, &mut y, 350);
    c.language = b.combo(x, y, 200, &["日本語".to_string(), "English".to_string()]);
    y += 28;
    let right_bottom = y;

    // 3列目: タスクバー表示
    let x = 760;
    let mut y = 16;
    b.header(strings::taskbar_header(), x, &mut y, 340);
    c.taskbar = b.check(strings::show_in_taskbar(), x, y, 340);
    y += 34;
    b.label(strings::taskbar_position(), x, y, 110, 24, SS_CENTERIMAGE, FontKind::Normal);
    c.taskbar_position[0] = b.radio(strings::taskbar_near_tray(), true, x + 110, y, 110);
    c.taskbar_position[1] = b.radio(strings::taskbar_left_edge(), false, x + 225, y, 110);
    y += 32;
    b.label(strings::taskbar_layout(), x, y, 110, 28, SS_CENTERIMAGE, FontKind::Normal);
    let layouts =
        [strings::taskbar_layout_auto(), strings::taskbar_layout_one(), strings::taskbar_layout_two()].map(str::to_string);
    c.taskbar_layout = b.combo(x + 110, y, 150, &layouts);
    y += 36;
    b.label(strings::taskbar_paused(), x, y, 110, 24, SS_CENTERIMAGE, FontKind::Normal);
    c.taskbar_paused[0] = b.radio(strings::taskbar_paused_hide(), true, x + 110, y, 90);
    c.taskbar_paused[1] = b.radio(strings::taskbar_paused_indicate(), false, x + 205, y, 135);
    y += 34;
    let rows: [(&str, i32, i32); 3] = [
        (strings::taskbar_max_width(), settings_range::TASKBAR_WIDTH.0, settings_range::TASKBAR_WIDTH.1),
        (strings::taskbar_font_size(), settings_range::TASKBAR_FONT.0, settings_range::TASKBAR_FONT.1),
        (strings::taskbar_offset(), settings_range::TASKBAR_OFFSET.0, settings_range::TASKBAR_OFFSET.1),
    ];
    let mut sliders = [HWND::default(); 3];
    let mut values = [HWND::default(); 3];
    for (i, (label, min, max)) in rows.into_iter().enumerate() {
        b.label(label, x, y, 110, 28, SS_CENTERIMAGE, FontKind::Normal);
        sliders[i] = b.slider(x + 110, y, 170, min, max);
        values[i] = b.label("", x + 280, y, 60, 28, SS_RIGHT | SS_CENTERIMAGE, FontKind::Normal);
        y += 34;
    }
    [c.taskbar_width, c.taskbar_font, c.taskbar_offset] = sliders;
    [c.taskbar_width_value, c.taskbar_font_value, c.taskbar_offset_value] = values;
    y += 6;
    c.taskbar_artist = b.check(strings::taskbar_show_artist(), x, y, 340);
    c.taskbar_art = b.check(strings::taskbar_show_album_art(), x, y + 28, 340);
    y += 28 * 2;
    let third_bottom = y;

    // 下のボタン
    let y = left_bottom.max(right_bottom).max(third_bottom) + 20;
    c.version = b.label(&strings::version_label(crate::APP_VERSION), 20, y, 150, 32, SS_CENTERIMAGE, FontKind::Small);
    b.button(strings::check_now(), ID_CHECK_UPDATE, false, 175, y, 160);
    let cancel_x = CLIENT_WIDTH - 20 - 100;
    let save_x = cancel_x - 12 - 100;
    let preview_x = save_x - 12 - 120;
    b.button(strings::preview(), ID_PREVIEW, false, preview_x, y, 120);
    b.button(strings::save(), ID_SAVE, true, save_x, y, 100);
    b.button(strings::cancel(), ID_CANCEL, false, cancel_x, y, 100);

    for (i, edit) in c.hotkey_boxes.iter().enumerate() {
        unsafe {
            let _ = SetWindowSubclass(*edit, Some(hotkey_box_proc), 1, i);
            // ホットキー欄では IME を使わない
            let _ = ImmAssociateContextEx(*edit, HIMC::default(), 0);
        }
    }

    b.state.controls = c;
    y + 32 + 18
}

/// スライダーの範囲（スライダー上の値）。幅は 10px、高さは 5px 刻み。
mod settings_range {
    pub const WIDTH: (i32, i32) = (20, 60);
    pub const HEIGHT: (i32, i32) = (16, 60);
    pub const MARGIN: (i32, i32) = (0, 100);
    pub const DURATION: (i32, i32) = (1, 15);
    pub const OPACITY: (i32, i32) = (20, 100);
    /// 10px 刻み
    pub const TASKBAR_WIDTH: (i32, i32) = (12, 40);
    pub const TASKBAR_FONT: (i32, i32) = (9, 16);
    /// 2px 刻み
    pub const TASKBAR_OFFSET: (i32, i32) = (0, 150);
}

fn set_check(hwnd: HWND, checked: bool) {
    unsafe {
        SendMessageW(hwnd, BM_SETCHECK, WPARAM(checked as usize), LPARAM(0));
    }
}

fn is_checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1 }
}

fn set_slider(hwnd: HWND, value: i32) {
    unsafe {
        SendMessageW(hwnd, TBM_SETPOS, WPARAM(1), LPARAM(value as isize));
    }
}

fn slider(hwnd: HWND) -> i32 {
    unsafe { SendMessageW(hwnd, TBM_GETPOS, WPARAM(0), LPARAM(0)).0 as i32 }
}

fn set_combo(hwnd: HWND, index: usize) {
    unsafe {
        SendMessageW(hwnd, CB_SETCURSEL, WPARAM(index), LPARAM(0));
    }
}

fn combo(hwnd: HWND) -> Option<usize> {
    let i = unsafe { SendMessageW(hwnd, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 };
    (i >= 0).then_some(i as usize)
}

fn set_text(hwnd: HWND, text: &str) {
    let text = wide(text);
    unsafe {
        let _ = SetWindowTextW(hwnd, text.pcwstr());
    }
}

fn load_values(state: &mut State) {
    let s = &state.original;
    let c = &state.controls;
    let pos_index = PopupPosition::ALL.iter().position(|p| *p == s.position).unwrap_or(1);
    set_check(c.position[pos_index], true);

    // 保存されたモニターが今は接続されていなければ「メインモニター」を選ぶ
    let monitor_index = state.monitor_names.iter().position(|n| !n.is_empty() && *n == s.monitor_device_name).unwrap_or(0);
    set_combo(c.monitor, monitor_index);

    set_slider(c.width, (s.popup_width + 5) / 10);
    set_slider(c.height, (s.popup_height + 2) / 5);
    set_slider(c.margin, s.popup_margin);
    set_slider(c.duration, s.display_duration_seconds);
    set_slider(c.opacity, (s.popup_opacity * 100.0).round() as i32);

    set_check(c.theme_dark, s.theme == Theme::Dark);
    set_check(c.theme_light, s.theme == Theme::Light);
    set_check(c.album_art, s.show_album_art);
    set_check(c.fullscreen, s.suppress_in_fullscreen);
    set_check(c.taskbar, s.show_in_taskbar);
    let tp = TaskbarPosition::ALL.iter().position(|p| *p == s.taskbar_position).unwrap_or(0);
    set_check(c.taskbar_position[tp], true);
    set_combo(c.taskbar_layout, TaskbarLayout::ALL.iter().position(|l| *l == s.taskbar_layout).unwrap_or(0));
    let paused = TaskbarPaused::ALL.iter().position(|p| *p == s.taskbar_paused).unwrap_or(1);
    set_check(c.taskbar_paused[paused], true);
    set_slider(c.taskbar_width, (s.taskbar_max_width + 5) / 10);
    set_slider(c.taskbar_font, s.taskbar_font_size);
    set_slider(c.taskbar_offset, (s.taskbar_offset + 1) / 2);
    set_check(c.taskbar_artist, s.taskbar_show_artist);
    set_check(c.taskbar_art, s.taskbar_show_album_art);
    set_check(c.startup, s.start_with_windows);
    set_check(c.check_updates, s.check_for_updates);
    set_combo(c.language, if s.language == Language::English { 1 } else { 0 });
}

/// 画面の入力内容から設定を作る。
fn build_settings() -> Option<Settings> {
    STATE.with(|s| {
        let s = s.try_borrow().ok()?;
        let s = s.as_ref()?;
        let c = &s.controls;
        let mut r = s.original.clone();

        if let Some(i) = c.position.iter().position(|h| is_checked(*h)) {
            r.position = PopupPosition::ALL[i];
        }
        r.monitor_device_name = combo(c.monitor).and_then(|i| s.monitor_names.get(i).cloned()).unwrap_or_default();
        r.popup_width = slider(c.width) * 10;
        r.popup_height = slider(c.height) * 5;
        r.popup_margin = slider(c.margin);
        r.display_duration_seconds = slider(c.duration);
        r.popup_opacity = slider(c.opacity) as f64 / 100.0;
        r.theme = if is_checked(c.theme_light) { Theme::Light } else { Theme::Dark };
        r.show_album_art = is_checked(c.album_art);
        r.suppress_in_fullscreen = is_checked(c.fullscreen);
        r.show_in_taskbar = is_checked(c.taskbar);
        if let Some(i) = c.taskbar_position.iter().position(|h| is_checked(*h)) {
            r.taskbar_position = TaskbarPosition::ALL[i];
        }
        if let Some(layout) = combo(c.taskbar_layout).and_then(|i| TaskbarLayout::ALL.get(i)) {
            r.taskbar_layout = *layout;
        }
        if let Some(i) = c.taskbar_paused.iter().position(|h| is_checked(*h)) {
            r.taskbar_paused = TaskbarPaused::ALL[i];
        }
        r.taskbar_max_width = slider(c.taskbar_width) * 10;
        r.taskbar_font_size = slider(c.taskbar_font);
        r.taskbar_offset = slider(c.taskbar_offset) * 2;
        r.taskbar_show_artist = is_checked(c.taskbar_artist);
        r.taskbar_show_album_art = is_checked(c.taskbar_art);
        r.start_with_windows = is_checked(c.startup);
        r.check_for_updates = is_checked(c.check_updates);
        [r.play_pause_hotkey, r.next_track_hotkey, r.previous_track_hotkey] = s.hotkeys;
        r.language = if combo(c.language) == Some(1) { Language::English } else { Language::Japanese };
        r.normalize();
        Some(r)
    })
}

fn controls() -> Option<ControlsCopy> {
    STATE.with(|s| {
        let s = s.try_borrow().ok()?;
        let s = s.as_ref()?;
        let c = &s.controls;
        Some(ControlsCopy {
            sliders: [c.width, c.height, c.margin, c.duration, c.opacity, c.taskbar_width, c.taskbar_font, c.taskbar_offset],
            values: [
                c.width_value,
                c.height_value,
                c.margin_value,
                c.duration_value,
                c.opacity_value,
                c.taskbar_width_value,
                c.taskbar_font_value,
                c.taskbar_offset_value,
            ],
            hotkey_boxes: c.hotkey_boxes,
            hotkey_error: c.hotkey_error,
            hint: c.hint,
            version: c.version,
        })
    })
}

struct ControlsCopy {
    sliders: [HWND; 8],
    values: [HWND; 8],
    hotkey_boxes: [HWND; 3],
    hotkey_error: HWND,
    hint: HWND,
    version: HWND,
}

fn update_value_labels() {
    let Some(c) = controls() else { return };
    let v: Vec<i32> = c.sliders.iter().map(|h| slider(*h)).collect();
    let texts = [
        format!("{}px", v[0] * 10),
        format!("{}px", v[1] * 5),
        format!("{}px", v[2]),
        strings::seconds(v[3]),
        format!("{}%", v[4]),
        format!("{}px", v[5] * 10),
        format!("{}px", v[6]),
        format!("{}px", v[7] * 2),
    ];
    for (hwnd, text) in c.values.iter().zip(texts) {
        set_text(*hwnd, &text);
    }
}

fn update_hotkey_boxes() {
    let Some(c) = controls() else { return };
    let Some(hotkeys) = STATE.with(|s| s.try_borrow().ok()?.as_ref().map(|s| s.hotkeys)) else { return };
    for (hwnd, key) in c.hotkey_boxes.iter().zip(hotkeys) {
        set_text(*hwnd, &key.to_display_string(strings::hotkey_none()));
    }
}

fn show_hotkey_error(message: Option<&str>) {
    if let Some(c) = controls() {
        set_text(c.hotkey_error, message.unwrap_or(""));
    }
}

// ---------------------------------------------------------------------------
// レイアウト

fn apply_layout(dpi: u32) {
    // フォントを作り直してから、各コントロールを DPI に合わせて配置する
    let base = message_font(dpi).unwrap_or_default();
    let make = |size: i32, bold: bool| {
        let mut lf = base;
        lf.lfHeight = -scale(size, dpi);
        lf.lfWeight = if bold { FW_BOLD.0 as i32 } else { FW_NORMAL.0 as i32 };
        unsafe { CreateFontIndirectW(&lf) }
    };
    let fonts = vec![(FontKind::Normal, make(14, false)), (FontKind::Header, make(15, true)), (FontKind::Small, make(12, false))];

    let (items, old_fonts): (Vec<(HWND, RECT, FontKind)>, Vec<HFONT>) = STATE.with(|s| {
        let Ok(mut s) = s.try_borrow_mut() else { return (Vec::new(), Vec::new()) };
        let Some(s) = s.as_mut() else { return (Vec::new(), Vec::new()) };
        let items = s
            .items
            .iter()
            .map(|i| {
                let r = RECT { left: scale(i.x, dpi), top: scale(i.y, dpi), right: scale(i.w, dpi), bottom: scale(i.h, dpi) };
                (i.hwnd, r, i.font)
            })
            .collect();
        let old = std::mem::replace(&mut s.fonts, fonts.clone()).into_iter().map(|(_, f)| f).collect();
        (items, old)
    });

    for (hwnd, r, kind) in items {
        let font = fonts.iter().find(|(k, _)| *k == kind).map(|(_, f)| *f).unwrap_or_default();
        unsafe {
            let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right, r.bottom, SWP_NOZORDER | SWP_NOACTIVATE);
            SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }
    }
    for f in old_fonts {
        unsafe {
            let _ = DeleteObject(f);
        }
    }
}

// ---------------------------------------------------------------------------
// 閉じる

fn close(save: bool) {
    let result = if save {
        let Some(mut settings) = build_settings() else { return };
        if settings.has_duplicate_hotkeys() {
            show_hotkey_error(Some(strings::hotkey_duplicate()));
            return;
        }
        if let Err(e) = settings.save() {
            eprintln!("failed to save settings: {e}");
        }
        Some(settings)
    } else {
        None
    };

    let Some(state) = STATE.with(|s| s.try_borrow_mut().ok()?.take()) else { return };
    unsafe {
        // オーナーを先に有効に戻しておくと、閉じた後にオーナーが前面に戻る
        if let Some(o) = state.owner {
            let _ = EnableWindow(o, true);
        }
        let _ = DestroyWindow(state.hwnd);
        for (_, f) in &state.fonts {
            let _ = DeleteObject(*f);
        }
        let _ = DeleteObject(state.background);
    }
    (state.callbacks.closed)(result);
}

// ---------------------------------------------------------------------------
// ホットキー欄のキー入力

unsafe fn modifiers_down() -> u32 {
    let down = |vk: VIRTUAL_KEY| GetKeyState(vk.0 as i32) < 0;
    let mut m = 0;
    if down(VK_CONTROL) { m |= MOD_CONTROL; }
    if down(VK_MENU) { m |= MOD_ALT; }
    if down(VK_SHIFT) { m |= MOD_SHIFT; }
    if down(VK_LWIN) || down(VK_RWIN) { m |= MOD_WIN; }
    m
}

fn is_modifier_key(vk: u32) -> bool {
    matches!(vk, 0x10..=0x12 | 0xA0..=0xA5 | 0x5B | 0x5C)
}

/// キー入力を処理したら true（既定の処理をさせない）。
unsafe fn capture_key(edit: HWND, index: usize, mut vk: u32) -> bool {
    if vk == VK_PROCESSKEY.0 as u32 {
        vk = ImmGetVirtualKey(edit);
    }
    let mods = modifiers_down();

    // Tab（フォーカス移動）と Esc（キャンセル）は通常どおり処理させる
    if mods == 0 && (vk == VK_TAB.0 as u32 || vk == VK_ESCAPE.0 as u32) {
        return false;
    }
    // 修飾キー単体の押下は無視して、続くキーを待つ
    if is_modifier_key(vk) || vk == 0 {
        return true;
    }

    let binding = if mods == 0 && (vk == VK_DELETE.0 as u32 || vk == VK_BACK.0 as u32) {
        HotkeyBinding::NONE
    } else {
        let b = HotkeyBinding::new(mods, vk);
        if !b.is_valid() {
            show_hotkey_error(Some(strings::hotkey_needs_modifier()));
            return true;
        }
        b
    };

    STATE.with(|s| {
        if let Some(s) = s.try_borrow_mut().ok().as_mut().and_then(|s| s.as_mut()) {
            s.hotkeys[index] = binding;
        }
    });
    show_hotkey_error(None);
    update_hotkey_boxes();
    true
}

unsafe extern "system" fn hotkey_box_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    index: usize,
) -> LRESULT {
    match msg {
        WM_GETDLGCODE => {
            // Tab と Esc 以外のキーはすべてこの欄で受け取る
            let m = lparam.0 as *const MSG;
            if !m.is_null() {
                let m = &*m;
                let vk = m.wParam.0 as u32;
                if m.message == WM_KEYDOWN && modifiers_down() == 0 && (vk == VK_TAB.0 as u32 || vk == VK_ESCAPE.0 as u32) {
                    return DefSubclassProc(hwnd, msg, wparam, lparam);
                }
            }
            LRESULT(DefSubclassProc(hwnd, msg, wparam, lparam).0 | DLGC_WANTALLKEYS)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if capture_key(hwnd, index, wparam.0 as u32) {
                LRESULT(0)
            } else {
                DefSubclassProc(hwnd, msg, wparam, lparam)
            }
        }
        WM_CHAR | WM_SYSCHAR | WM_DEADCHAR | WM_SYSDEADCHAR | WM_KEYUP | WM_SYSKEYUP => LRESULT(0),
        WM_NCDESTROY => {
            let _ = RemoveWindowSubclass(hwnd, Some(hotkey_box_proc), 1);
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

// ---------------------------------------------------------------------------

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = loword(wparam.0) as i32;
            let code = hiword(wparam.0);
            if code == BN_CLICKED || lparam.0 == 0 {
                match id {
                    ID_PREVIEW => {
                        if let (Some(settings), Some(cb)) =
                            (build_settings(), STATE.with(|s| s.try_borrow().ok()?.as_ref().map(|s| s.callbacks.preview)))
                        {
                            cb(settings);
                        }
                    }
                    ID_CHECK_UPDATE => {
                        if let Some(cb) = STATE.with(|s| s.try_borrow().ok()?.as_ref().map(|s| s.callbacks.check_update)) {
                            cb();
                        }
                    }
                    ID_SAVE => close(true),
                    ID_CANCEL => close(false),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_HSCROLL => {
            update_value_labels();
            LRESULT(0)
        }
        DM_GETDEFID => LRESULT(((DC_HASDEFID << 16) | ID_SAVE as usize) as isize),
        WM_CLOSE => {
            close(false);
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            let mut r = RECT::default();
            let _ = GetClientRect(hwnd, &mut r);
            let brush = STATE.with(|s| s.try_borrow().ok().and_then(|s| s.as_ref().map(|s| s.background)));
            FillRect(HDC(wparam.0 as *mut _), &r, brush.unwrap_or(HBRUSH(GetStockObject(WHITE_BRUSH).0)));
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let hdc = HDC(wparam.0 as *mut _);
            let control = HWND(lparam.0 as *mut _);
            let Some(c) = controls() else { return DefWindowProcW(hwnd, msg, wparam, lparam) };
            // 読み取り専用のホットキー欄は白地のままにする
            if c.hotkey_boxes.contains(&control) {
                SetBkColor(hdc, COLORREF(GetSysColor(COLOR_WINDOW)));
                return LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize);
            }
            SetBkMode(hdc, TRANSPARENT);
            if control == c.hotkey_error {
                SetTextColor(hdc, colorref(0xC6, 0x28, 0x28));
            } else if control == c.hint || control == c.version {
                SetTextColor(hdc, COLORREF(GRAY_TEXT));
            }
            let brush = STATE.with(|s| s.try_borrow().ok().and_then(|s| s.as_ref().map(|s| s.background)));
            LRESULT(brush.map(|b| b.0 as isize).unwrap_or(0))
        }
        WM_DPICHANGED => {
            let dpi = hiword(wparam.0);
            apply_layout(dpi);
            let r = &*(lparam.0 as *const RECT);
            let (w, h) = window_size(dpi, CONTENT_HEIGHT.with(|c| c.get()));
            let _ = SetWindowPos(hwnd, None, r.left, r.top, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
