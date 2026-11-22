//! タスクバーの上に、再生中の曲名を小さく表示する。
//! タスクバー (Shell_TrayWnd) の通知領域 (TrayNotifyWnd) の左隣、またはタスクバーの左端に、
//! 最前面のレイヤードウィンドウとして重ねる。
//! タスクバーをクリックするとタスクバーが手前に来るので、前面のウィンドウが変わるたびに前に出し直す。

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use windows::core::{w, Result};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, SetWinEventHook, TreeScope_Descendants, UnhookWinEvent, UIA_ButtonControlTypeId,
    HWINEVENTHOOK,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::imaging::Image;
use crate::popup::Dib;
use crate::settings::{Settings, TaskbarLayout, TaskbarPaused, TaskbarPosition};
use crate::strings;
use crate::win::{scale, ui_font_face, wide, WideString};

const PADDING_X: f32 = 6.0;
const PADDING_Y: f32 = 5.0;
const ART_GAP: f32 = 7.0;
const ART_CORNER_RADIUS: f32 = 3.0;
const CORNER_RADIUS: f32 = 4.0;
/// 2行表示のとき、アーティスト名を曲名よりどれだけ小さくするか。
const ARTIST_SIZE_DIFF: f32 = 1.0;
/// 一時停止中の文字とアルバムアートの不透明度。
const PAUSED_OPACITY: f32 = 0.55;
/// 一時停止マークと曲名の間隔。
const PAUSE_MARK_GAP: f32 = 5.0;

const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_REFRESH: usize = 1;
const REFRESH_MS: u32 = 1000;

/// 設定のうち、タスクバー表示に関わるもの。
#[derive(Clone, PartialEq)]
struct Config {
    enabled: bool,
    position: TaskbarPosition,
    layout: TaskbarLayout,
    max_width: i32,
    font_size: f32,
    offset: i32,
    show_artist: bool,
    show_album_art: bool,
    paused: TaskbarPaused,
}

impl Config {
    fn from(s: &Settings) -> Self {
        Self {
            enabled: s.show_in_taskbar,
            position: s.taskbar_position,
            layout: s.taskbar_layout,
            max_width: s.taskbar_max_width,
            font_size: s.taskbar_font_size as f32,
            offset: s.taskbar_offset,
            show_artist: s.taskbar_show_artist,
            show_album_art: s.taskbar_show_album_art,
            paused: s.taskbar_paused,
        }
    }
}

/// タスクバーの位置と DPI（px）。
#[derive(Clone, Copy, PartialEq)]
struct Placement {
    /// タスクバーの左右と上下（画面内に見えている範囲）
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    /// 通知領域の左端
    tray_left: i32,
    /// スタートボタンの右端（Windows 10 など、左端にスタートボタンがある場合）
    start_right: i32,
    /// アプリのボタンが並んでいる範囲（左端, 右端）。調べられなかったときは None。
    buttons: Option<(i32, i32)>,
    dpi: u32,
    light: bool,
}

struct Bar {
    hwnd: HWND,
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    hook: HWINEVENTHOOK,

    config: Config,
    /// 設定画面のプレビュー中（曲が流れていなくても見本を出す）
    preview: bool,
    /// 再生中か（一時停止中なら false）
    playing: bool,
    title: String,
    artist: String,
    album_art: Option<Image>,

    placement: Option<Placement>,
    content: Option<Dib>,
    origin: POINT,
    /// 内容を描き直す必要がある
    dirty: bool,
    hover: bool,
}

#[derive(Clone, Copy)]
pub struct Callbacks {
    pub click: fn(),
    pub menu: fn(),
}

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
    static CALLBACKS: Cell<Option<Callbacks>> = const { Cell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
    BAR.with(|b| b.try_borrow_mut().ok()?.as_mut().map(f))
}

pub fn create(callbacks: Callbacks) -> Result<()> {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_HAND)?,
            lpszClassName: w!("NowPlayingNotifier.Taskbar"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("NowPlayingNotifier.Taskbar"),
            w!("Now Playing (Taskbar)"),
            WS_POPUP,
            0, 0, 0, 0,
            None, None, hinstance, None,
        )?;
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        CALLBACKS.with(|c| c.set(Some(callbacks)));

        // 前面のウィンドウが変わったら（タスクバーのクリックや全画面アプリの起動など）すぐに状態を見直す
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        SetTimer(hwnd, TIMER_REFRESH, REFRESH_MS, None);

        BAR.with(|b| {
            *b.borrow_mut() = Some(Bar {
                hwnd,
                d2d,
                dwrite,
                hook,
                config: Config::from(&Settings::default()),
                preview: false,
                playing: true,
                title: String::new(),
                artist: String::new(),
                album_art: None,
                placement: None,
                content: None,
                origin: POINT::default(),
                dirty: true,
                hover: false,
            })
        });
    }
    Ok(())
}

pub fn destroy() {
    if let Some(b) = BAR.with(|b| b.borrow_mut().take()) {
        unsafe {
            if !b.hook.is_invalid() {
                let _ = UnhookWinEvent(b.hook);
            }
            let _ = DestroyWindow(b.hwnd);
        }
    }
}

pub fn apply_settings(settings: &Settings) {
    with(|b| {
        let config = Config::from(settings);
        if b.config == config {
            return;
        }
        b.config = config;
        if !b.config.show_album_art {
            b.album_art = None;
        }
        b.dirty = true;
        b.refresh();
    });
}

/// 設定画面のプレビュー。曲が流れていないときは見本の文字を表示する。
pub fn set_preview(on: bool) {
    with(|b| {
        if b.preview != on {
            b.preview = on;
            b.dirty = true;
            b.refresh();
        }
    });
}

/// 表示する曲を設定する。曲が変わったらアルバムアートは消える（後から `set_album_art` で差し込む）。
pub fn set_track(title: &str, artist: &str) {
    with(|b| {
        if b.title == title && b.artist == artist {
            return;
        }
        b.title = title.to_string();
        b.artist = artist.to_string();
        b.album_art = None;
        b.dirty = true;
        b.refresh();
    });
}

/// 再生中か一時停止中かを伝える。
pub fn set_playing(playing: bool) {
    with(|b| {
        if b.playing != playing {
            b.playing = playing;
            b.dirty = true;
            b.refresh();
        }
    });
}

/// 再生中の曲がなくなった。
pub fn clear() {
    set_track("", "");
}

pub fn set_album_art(image: Image) {
    with(|b| {
        if !b.config.show_album_art || b.title.is_empty() {
            return;
        }
        b.album_art = Some(image);
        b.dirty = true;
        b.refresh();
    });
}

/// タスクバーの位置などを確認し直す（エクスプローラーの再起動時など）。
pub fn refresh() {
    with(Bar::refresh);
}

impl Bar {
    fn hide(&mut self) {
        unsafe {
            if IsWindowVisible(self.hwnd).as_bool() {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    /// 表示する曲名とアーティスト。曲がなくプレビュー中でもなければ None。
    fn text(&self) -> Option<(String, String)> {
        if !self.title.is_empty() {
            Some((self.title.clone(), self.artist.clone()))
        } else if self.preview {
            Some((strings::preview_title().to_string(), strings::preview_artist().to_string()))
        } else {
            None
        }
    }

    /// 曲が一時停止中か（プレビューの見本は再生中として扱う）。
    fn is_paused(&self) -> bool {
        !self.playing && !self.title.is_empty()
    }

    fn refresh(&mut self) {
        let hidden_while_paused = self.is_paused() && self.config.paused == TaskbarPaused::Hide;
        if !self.config.enabled || self.text().is_none() || hidden_while_paused {
            self.hide();
            return;
        }
        let Some(placement) = find_placement() else {
            self.hide();
            return;
        };
        if self.placement != Some(placement) {
            self.placement = Some(placement);
            self.dirty = true;
        }
        if self.dirty {
            self.dirty = false;
            self.content = None;
            if let Err(e) = self.render(placement) {
                eprintln!("taskbar render failed: {e}");
            }
            self.present();
        }
        if self.content.is_none() {
            self.hide();
            return;
        }
        unsafe {
            if !IsWindowVisible(self.hwnd).as_bool() {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
            }
            // タスクバーより手前に出す
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    fn colors(&self, light: bool) -> (D2D1_COLOR_F, D2D1_COLOR_F, D2D1_COLOR_F) {
        let c = |r: f32, g: f32, b: f32, a: f32| D2D1_COLOR_F { r, g, b, a };
        if light {
            (c(0.10, 0.10, 0.10, 1.0), c(0.33, 0.33, 0.33, 1.0), c(0.0, 0.0, 0.0, 0.06))
        } else {
            (c(1.0, 1.0, 1.0, 1.0), c(0.78, 0.78, 0.78, 1.0), c(1.0, 1.0, 1.0, 0.08))
        }
    }

    /// 内容を `content` に描き、表示位置 `origin` を決める。幅は曲名の長さに合わせる。
    fn render(&mut self, p: Placement) -> Result<()> {
        let Some((title, artist)) = self.text() else { return Ok(()) };
        let artist = if self.config.show_artist { artist } else { String::new() };
        let c = &self.config;

        let dpi = p.dpi as f32;
        let to_dip = |px: i32| px as f32 * 96.0 / dpi;
        let height = to_dip(p.bottom - p.top);
        let offset = scale(c.offset, p.dpi);
        // 置く場所の基準（px）。タスクバーの中央を越えて広がらないようにする。
        let center = p.left + (p.right - p.left) / 2;
        // ボタンの範囲が分かるときは、ボタンに重ならない所までを使う（タスクバーが端まで埋まっていれば何も出さない）。
        let (anchor, available) = match c.position {
            TaskbarPosition::NearTray => {
                let right = p.tray_left - offset;
                let limit = p.buttons.map_or(center, |(_, buttons_right)| buttons_right + offset);
                (right, right - limit)
            }
            TaskbarPosition::LeftEdge => {
                let left = p.start_right.max(p.left) + offset;
                let limit = p.buttons.map_or(center, |(buttons_left, _)| buttons_left - offset);
                (left, limit - left)
            }
        };
        let max_width = to_dip(available).min(c.max_width as f32);

        let art = self.album_art.as_ref().filter(|_| c.show_album_art && !self.title.is_empty());
        let art_size = (height - PADDING_Y * 2.0).max(0.0);
        let art_right = PADDING_X + if art.is_some() && art_size > 0.0 { art_size + ART_GAP } else { 0.0 };
        // 一時停止中は、曲名の前に一時停止マーク（縦棒2本）を付ける
        let paused = self.is_paused();
        let bar_width = (c.font_size * 0.2).max(2.0).round();
        let mark_width = bar_width * 3.0;
        let text_left = art_right + if paused { mark_width + PAUSE_MARK_GAP } else { 0.0 };
        let text_max = max_width - text_left - PADDING_X;
        if text_max < 24.0 {
            return Ok(()); // 狭すぎて表示できない
        }

        unsafe {
            let face = wide(&ui_font_face());
            let locale = wide(match strings::current() {
                strings::Language::Japanese => "ja-jp",
                strings::Language::English => "en-us",
            });
            let one_line = |this: &Self| -> Result<Vec<IDWriteTextLayout>> {
                let text = if artist.is_empty() { title.clone() } else { format!("{title} – {artist}") };
                Ok(vec![this.text_layout(&text, &face, &locale, c.font_size, DWRITE_FONT_WEIGHT_NORMAL, text_max)?])
            };
            let mut lines = if artist.is_empty() || c.layout == TaskbarLayout::OneLine {
                one_line(self)?
            } else {
                let artist_size = (c.font_size - ARTIST_SIZE_DIFF).max(1.0);
                vec![
                    self.text_layout(&title, &face, &locale, c.font_size, DWRITE_FONT_WEIGHT_SEMI_BOLD, text_max)?,
                    self.text_layout(&artist, &face, &locale, artist_size, DWRITE_FONT_WEIGHT_NORMAL, text_max)?,
                ]
            };
            let measure = |lines: &[IDWriteTextLayout]| -> Vec<DWRITE_TEXT_METRICS> {
                lines
                    .iter()
                    .map(|l| {
                        let mut m = DWRITE_TEXT_METRICS::default();
                        let _ = l.GetMetrics(&mut m);
                        m
                    })
                    .collect()
            };
            let mut metrics = measure(&lines);
            // 自動のときは、2行がタスクバーの高さに収まらなければ1行にする
            if c.layout == TaskbarLayout::Auto && lines.len() == 2 {
                let total: f32 = metrics.iter().map(|m| m.height).sum();
                if total > height - 4.0 {
                    lines = one_line(self)?;
                    metrics = measure(&lines);
                }
            }
            let text_width = metrics.iter().map(|m| m.width).fold(0.0f32, f32::max).min(text_max);
            let width = (text_left + text_width + PADDING_X).ceil();

            let (w, h) = (scale(width as i32, p.dpi).max(1), (p.bottom - p.top).max(1));
            let Some(dib) = Dib::new(w, h) else { return Ok(()) };
            let x = match c.position {
                TaskbarPosition::NearTray => anchor - w,
                TaskbarPosition::LeftEdge => anchor,
            };
            self.origin = POINT { x, y: p.top };

            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: dpi,
                dpiY: dpi,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = self.d2d.CreateDCRenderTarget(&props)?;
            rt.BindDC(dib.dc, &RECT { left: 0, top: 0, right: w, bottom: h })?;
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);

            let (mut title_color, mut artist_color, hover_color) = self.colors(p.light);
            let dim = if paused { PAUSED_OPACITY } else { 1.0 };
            title_color.a *= dim;
            artist_color.a *= dim;
            rt.BeginDraw();
            // 完全に透明だとクリックが下のタスクバーに抜けるので、ほぼ透明な色で塗っておく
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 1.0 / 255.0 }));
            if self.hover {
                let brush = rt.CreateSolidColorBrush(&hover_color, None)?;
                let rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F { left: 0.0, top: 2.0, right: width, bottom: height - 2.0 },
                    radiusX: CORNER_RADIUS,
                    radiusY: CORNER_RADIUS,
                };
                rt.FillRoundedRectangle(&rect, &brush);
            }
            if let (Some(img), true) = (art, art_size > 0.0) {
                draw_album_art(&rt, img, PADDING_X, (height - art_size) / 2.0, art_size, dim)?;
            }
            if paused {
                let brush = rt.CreateSolidColorBrush(&title_color, None)?;
                let mark_height = (c.font_size * 0.8).round();
                let top = ((height - mark_height) / 2.0).round();
                for left in [art_right, art_right + bar_width * 2.0] {
                    let rect = D2D1_ROUNDED_RECT {
                        rect: D2D_RECT_F { left, top, right: left + bar_width, bottom: top + mark_height },
                        radiusX: bar_width / 3.0,
                        radiusY: bar_width / 3.0,
                    };
                    rt.FillRoundedRectangle(&rect, &brush);
                }
            }

            let total: f32 = metrics.iter().map(|m| m.height).sum();
            let mut y = ((height - total) / 2.0).max(0.0);
            for (i, (layout, m)) in lines.iter().zip(&metrics).enumerate() {
                let color = if i == 0 { &title_color } else { &artist_color };
                let brush = rt.CreateSolidColorBrush(color, None)?;
                rt.DrawTextLayout(D2D_POINT_2F { x: text_left, y }, layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                y += m.height;
            }
            rt.EndDraw(None, None)?;
            self.content = Some(dib);
        }
        Ok(())
    }

    unsafe fn text_layout(
        &self,
        text: &str,
        face: &WideString,
        locale: &WideString,
        size: f32,
        weight: DWRITE_FONT_WEIGHT,
        max_width: f32,
    ) -> Result<IDWriteTextLayout> {
        let format = self.dwrite.CreateTextFormat(
            face.pcwstr(),
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            locale.pcwstr(),
        )?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        let sign = self.dwrite.CreateEllipsisTrimmingSign(&format)?;
        let trimming = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
        format.SetTrimming(&trimming, &sign)?;
        self.dwrite.CreateTextLayout(wide(text).as_slice(), &format, max_width, 1000.0)
    }

    fn present(&self) {
        let Some(dib) = &self.content else { return };
        let origin = self.origin;
        let size = SIZE { cx: dib.width, cy: dib.height };
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        unsafe {
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&origin),
                Some(&size),
                dib.dc,
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    fn set_hover(&mut self, hover: bool) {
        if self.hover != hover {
            self.hover = hover;
            self.dirty = true;
            self.refresh();
        }
    }
}

unsafe fn draw_album_art(rt: &ID2D1DCRenderTarget, img: &Image, left: f32, top: f32, size: f32, opacity: f32) -> Result<()> {
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
    };
    let bitmap = rt.CreateBitmap(
        D2D_SIZE_U { width: img.width, height: img.height },
        Some(img.pixels.as_ptr() as *const _),
        img.width * 4,
        &props,
    )?;
    let brush_props = D2D1_BITMAP_BRUSH_PROPERTIES {
        extendModeX: D2D1_EXTEND_MODE_CLAMP,
        extendModeY: D2D1_EXTEND_MODE_CLAMP,
        interpolationMode: D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
    };
    let brush = rt.CreateBitmapBrush(&bitmap, Some(&brush_props), None)?;
    let (iw, ih) = (img.width as f32, img.height as f32);
    let s = (size / iw).max(size / ih);
    let dx = left + (size - iw * s) / 2.0;
    let dy = top + (size - ih * s) / 2.0;
    brush.SetTransform(&Matrix3x2 { M11: s, M12: 0.0, M21: 0.0, M22: s, M31: dx, M32: dy });
    brush.SetOpacity(opacity);
    let rect = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F { left, top, right: left + size, bottom: top + size },
        radiusX: ART_CORNER_RADIUS,
        radiusY: ART_CORNER_RADIUS,
    };
    rt.FillRoundedRectangle(&rect, &brush);
    Ok(())
}

/// メインモニターのタスクバーで、表示できる場所を探す。
/// 横置きでないとき・自動的に隠れているとき・全画面アプリがあるときは None。
fn find_placement() -> Option<Placement> {
    unsafe {
        let tray = FindWindowW(w!("Shell_TrayWnd"), None).ok()?;
        if !IsWindowVisible(tray).as_bool() {
            return None;
        }
        let mut bar = RECT::default();
        GetWindowRect(tray, &mut bar).ok()?;
        let (bw, bh) = (bar.right - bar.left, bar.bottom - bar.top);
        if bh <= 0 || bw <= bh {
            return None; // 縦置きのタスクバーには対応しない
        }

        let monitor = MonitorFromWindow(tray, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let screen = info.rcMonitor;
        // 自動的に隠す設定で引っ込んでいる間は出さない
        let visible = bar.bottom.min(screen.bottom) - bar.top.max(screen.top);
        if visible < bh * 3 / 4 {
            return None;
        }
        if foreground_is_fullscreen(tray, screen) {
            return None;
        }

        let child_rect = |class: windows::core::PCWSTR| -> Option<RECT> {
            let hwnd = FindWindowExW(tray, None, class, None).ok()?;
            if !IsWindowVisible(hwnd).as_bool() {
                return None;
            }
            let mut r = RECT::default();
            GetWindowRect(hwnd, &mut r).ok()?;
            (r.right > r.left).then_some(r)
        };
        let tray_left = child_rect(w!("TrayNotifyWnd")).map(|r| r.left).filter(|&l| l > bar.left).unwrap_or(bar.right);
        let start_right = child_rect(w!("Start")).map(|r| r.right).filter(|&r| r < bar.right).unwrap_or(bar.left);

        let (mut dpi, mut _y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut _y);
        let dpi = dpi.max(96);
        Some(Placement {
            left: bar.left.max(screen.left),
            right: bar.right.min(screen.right),
            tray_left,
            start_right,
            buttons: task_buttons(tray, bar, start_right, tray_left),
            top: bar.top.max(screen.top),
            bottom: bar.bottom.min(screen.bottom),
            dpi,
            light: taskbar_is_light(),
        })
    }
}

/// タスクバー上のボタン（スタート・ピン留め・実行中のアプリ）が並ぶ範囲 `(左端, 右端)` を UI Automation で調べる。
/// `start_right` より右、`tray_left` より左にあるボタンだけを数える。ボタンがなければ範囲は空（左端 > 右端）。
/// 調べるのに少し時間がかかるので、結果を短時間使い回す。
fn task_buttons(tray: HWND, bar: RECT, start_right: i32, tray_left: i32) -> Option<(i32, i32)> {
    const CACHE: Duration = Duration::from_millis(700);
    thread_local! {
        static LAST: RefCell<Option<(Instant, (i32, i32, i32, i32), Option<(i32, i32)>)>> = const { RefCell::new(None) };
    }
    let key = (bar.left, bar.right, start_right, tray_left);
    if let Some(hit) = LAST.with(|l| l.borrow().as_ref().filter(|(t, k, _)| *k == key && t.elapsed() < CACHE).map(|(_, _, r)| *r)) {
        return hit;
    }
    let result = unsafe { scan_task_buttons(tray, bar, start_right, tray_left) };
    LAST.with(|l| *l.borrow_mut() = Some((Instant::now(), key, result)));
    result
}

unsafe fn scan_task_buttons(tray: HWND, bar: RECT, start_right: i32, tray_left: i32) -> Option<(i32, i32)> {
    let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
    let root = uia.ElementFromHandle(tray).ok()?;
    let all = root.FindAll(TreeScope_Descendants, &uia.CreateTrueCondition().ok()?).ok()?;
    let (mut min_left, mut max_right) = (i32::MAX, i32::MIN);
    for i in 0..all.Length().ok()? {
        let Ok(e) = all.GetElement(i) else { continue };
        if e.CurrentControlType().ok() != Some(UIA_ButtonControlTypeId) {
            continue;
        }
        let Ok(r) = e.CurrentBoundingRectangle() else { continue };
        let in_bar = r.right > r.left && r.top < bar.bottom && r.bottom > bar.top;
        if in_bar && r.left >= start_right && r.right <= tray_left {
            min_left = min_left.min(r.left);
            max_right = max_right.max(r.right);
        }
    }
    // ボタンが1つも見つからなかった（UIA が使えない）ときは、従来どおりの位置決めにする
    (min_left <= max_right).then_some((min_left, max_right))
}

/// タスクバーのあるモニターを、前面のウィンドウ（ゲームや動画など）が覆っているか。
unsafe fn foreground_is_fullscreen(tray: HWND, screen: RECT) -> bool {
    let fg = GetForegroundWindow();
    if fg.is_invalid() || fg == tray {
        return false;
    }
    let mut class = [0u16; 64];
    let len = GetClassNameW(fg, &mut class) as usize;
    let class = String::from_utf16_lossy(&class[..len]);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return false;
    }
    let mut r = RECT::default();
    if GetWindowRect(fg, &mut r).is_err() {
        return false;
    }
    r.left <= screen.left && r.top <= screen.top && r.right >= screen.right && r.bottom >= screen.bottom
}

/// タスクバーがライトテーマか（Windows の「既定のアプリモード」ではなく「Windows モード」の設定）。
fn taskbar_is_light() -> bool {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut _ as *mut _),
            Some(&mut size),
        )
    };
    result.is_ok() && value != 0
}

unsafe extern "system" fn on_foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    refresh();
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wparam.0 == TIMER_REFRESH => {
            refresh();
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            let mut track = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut track);
            with(|b| b.set_hover(true));
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with(|b| b.set_hover(false));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Some(cb) = CALLBACKS.with(|c| c.get()) {
                (cb.click)();
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            if let Some(cb) = CALLBACKS.with(|c| c.get()) {
                (cb.menu)();
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED => {
            // テーマや解像度が変わったら描き直す
            with(|b| {
                b.dirty = true;
                b.refresh();
            });
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
