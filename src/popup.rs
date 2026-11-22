//! 曲名のポップアップ。Direct2D で描いた画像を、レイヤードウィンドウ（ピクセル単位の透過）として表示する。
//! スライドイン・フェードは、描いておいた画像をずらしたり透明度を変えたりして毎フレーム反映する。

use std::cell::RefCell;
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
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::imaging::Image;
use crate::position::{calculate_position, Rect};
use crate::settings::{Settings, Theme};
use crate::win::{scale, ui_font_face, wide};

const PADDING: f32 = 18.0;
const BORDER: f32 = 1.0;
const CORNER_RADIUS: f32 = 12.0;
const ART_CORNER_RADIUS: f32 = 6.0;
const ART_MARGIN: f32 = 14.0;
const TITLE_SIZE: f32 = 15.0;
const ARTIST_SIZE: f32 = 11.0;
const LINE_GAP: f32 = 3.0;

const SLIDE_DISTANCE: i32 = 80;
const SLIDE_IN: Duration = Duration::from_millis(500);
const FADE_IN: Duration = Duration::from_millis(400);
const FADE_OUT: Duration = Duration::from_millis(1000);

const TIMER_FRAME: usize = 1;
const TIMER_HOLD: usize = 2;
const FRAME_MS: u32 = 15;

pub const ALBUM_ART_DECODE_WIDTH: u32 = 256;

#[derive(Clone, Copy)]
struct Colors {
    background: D2D1_COLOR_F,
    border: D2D1_COLOR_F,
    title: D2D1_COLOR_F,
    artist: D2D1_COLOR_F,
}

fn rgb(v: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((v >> 16) & 0xFF) as f32 / 255.0,
        g: ((v >> 8) & 0xFF) as f32 / 255.0,
        b: (v & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

fn colors(theme: Theme) -> Colors {
    match theme {
        Theme::Dark => Colors { background: rgb(0x1E1E1E), border: rgb(0x333333), title: rgb(0xFFFFFF), artist: rgb(0xCCCCCC) },
        Theme::Light => Colors { background: rgb(0xF5F5F5), border: rgb(0xCCCCCC), title: rgb(0x1E1E1E), artist: rgb(0x555555) },
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Hidden,
    /// スライドイン・フェードイン中、または表示し続けている。
    Showing { since: Instant },
    FadingOut { since: Instant },
}

/// 32bit のメモリ上のビットマップと、それを選択した DC。
pub(crate) struct Dib {
    pub(crate) dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

impl Dib {
    pub(crate) fn new(width: i32, height: i32) -> Option<Self> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height, // 上から下
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            let old = SelectObject(dc, bitmap);
            Some(Self { dc, bitmap, old, bits: bits as *mut u8, width, height })
        }
    }

    fn pixels(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.bits, (self.width * self.height * 4) as usize) }
    }

    fn pixels_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.width * self.height * 4) as usize) }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.dc);
        }
    }
}

struct Popup {
    hwnd: HWND,
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,

    title: String,
    artist: String,
    album_art: Option<Image>,

    width_dip: i32,
    height_dip: i32,
    opacity: f64,
    colors: Colors,
    show_album_art: bool,

    dpi: u32,
    origin: POINT,
    content: Option<Dib>,
    frame: Option<Dib>,
    phase: Phase,
}

thread_local! {
    static POPUP: RefCell<Option<Popup>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Popup) -> R) -> Option<R> {
    POPUP.with(|p| p.try_borrow_mut().ok()?.as_mut().map(f))
}

/// ポップアップのウィンドウを作る。`on_click` はクリックされたときに（ポップアップを閉じた後で）呼ばれる。
pub fn create(on_click: fn()) -> Result<()> {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_HAND)?,
            lpszClassName: w!("NowPlayingNotifier.Popup"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("NowPlayingNotifier.Popup"),
            w!("Now Playing"),
            WS_POPUP,
            0, 0, 0, 0,
            None, None, hinstance, None,
        )?;
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        CLICK_HANDLER.with(|h| h.set(Some(on_click)));

        let defaults = Settings::default();
        POPUP.with(|p| {
            *p.borrow_mut() = Some(Popup {
                hwnd,
                d2d,
                dwrite,
                title: String::new(),
                artist: String::new(),
                album_art: None,
                width_dip: defaults.popup_width,
                height_dip: defaults.popup_height,
                opacity: defaults.popup_opacity,
                colors: colors(defaults.theme),
                show_album_art: defaults.show_album_art,
                dpi: 96,
                origin: POINT::default(),
                content: None,
                frame: None,
                phase: Phase::Hidden,
            })
        });
    }
    Ok(())
}

thread_local! {
    static CLICK_HANDLER: std::cell::Cell<Option<fn()>> = const { std::cell::Cell::new(None) };
}

pub fn destroy() {
    if let Some(p) = POPUP.with(|p| p.borrow_mut().take()) {
        unsafe {
            let _ = DestroyWindow(p.hwnd);
        }
    }
}

/// 設定のサイズ・テーマ・透明度を反映する（表示中なら描き直す）。
pub fn apply_settings(settings: &Settings) {
    with(|p| {
        p.width_dip = settings.popup_width;
        p.height_dip = settings.popup_height;
        p.opacity = settings.popup_opacity;
        p.colors = colors(settings.theme);
        p.show_album_art = settings.show_album_art;
        if !p.show_album_art {
            p.album_art = None;
        }
        if p.phase != Phase::Hidden {
            p.render();
            p.present();
        }
    });
}

/// 指定したモニターの作業領域の隅にポップアップを表示する。
pub fn show(title: &str, artist: &str, duration_seconds: i32, settings: &Settings, work_area: Rect, dpi: u32) {
    with(|p| {
        p.title = title.to_string();
        p.artist = artist.to_string();
        p.album_art = None;
        p.dpi = dpi;

        let (w, h) = (scale(p.width_dip, dpi), scale(p.height_dip, dpi));
        let (x, y) = calculate_position(settings.position, w, h, work_area, scale(settings.popup_margin, dpi));
        p.origin = POINT { x, y };

        p.render();
        p.phase = Phase::Showing { since: Instant::now() };
        p.present();
        unsafe {
            let _ = ShowWindow(p.hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(p.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            SetTimer(p.hwnd, TIMER_FRAME, FRAME_MS, None);
            SetTimer(p.hwnd, TIMER_HOLD, (duration_seconds.max(1) as u32) * 1000, None);
        }
    });
}

/// 表示中のポップアップにアルバムアートを差し込む。
pub fn set_album_art(image: Image) {
    with(|p| {
        if !p.show_album_art || p.phase == Phase::Hidden {
            return;
        }
        p.album_art = Some(image);
        p.render();
        p.present();
    });
}

pub fn hide() {
    with(Popup::hide);
}

impl Popup {
    fn hide(&mut self) {
        self.phase = Phase::Hidden;
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_FRAME);
            let _ = KillTimer(self.hwnd, TIMER_HOLD);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        self.album_art = None;
        self.content = None;
        self.frame = None;
    }

    fn size_px(&self) -> (i32, i32) {
        (scale(self.width_dip, self.dpi).max(1), scale(self.height_dip, self.dpi).max(1))
    }

    /// 現在の内容を `content` に描く。
    fn render(&mut self) {
        let (w, h) = self.size_px();
        if self.content.as_ref().map(|d| (d.width, d.height)) != Some((w, h)) {
            self.content = Dib::new(w, h);
            self.frame = Dib::new(w, h);
        }
        let Some(dib) = &self.content else { return };
        if let Err(e) = self.draw(dib.dc, w, h) {
            eprintln!("popup render failed: {e}");
        }
    }

    fn draw(&self, dc: HDC, w: i32, h: i32) -> Result<()> {
        unsafe {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: self.dpi as f32,
                dpiY: self.dpi as f32,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let rt = self.d2d.CreateDCRenderTarget(&props)?;
            rt.BindDC(dc, &RECT { left: 0, top: 0, right: w, bottom: h })?;
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);

            let width = self.width_dip as f32;
            let height = self.height_dip as f32;

            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));

            // 背景と枠線
            let half = BORDER / 2.0;
            let outline = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F { left: half, top: half, right: width - half, bottom: height - half },
                radiusX: CORNER_RADIUS - half,
                radiusY: CORNER_RADIUS - half,
            };
            let bg = rt.CreateSolidColorBrush(&self.colors.background, None)?;
            let border = rt.CreateSolidColorBrush(&self.colors.border, None)?;
            rt.FillRoundedRectangle(&outline, &bg);
            rt.DrawRoundedRectangle(&outline, &border, BORDER, None);

            // アルバムアート（内側の高さに合わせた正方形、中央を切り抜いて敷き詰める）
            let inner_left = BORDER + PADDING;
            let mut text_left = inner_left;
            if let (true, Some(img)) = (self.show_album_art, &self.album_art) {
                let size = (height - PADDING * 2.0 - BORDER * 2.0).max(0.0);
                let top = (height - size) / 2.0;
                if size > 0.0 {
                    self.draw_album_art(&rt, img, inner_left, top, size)?;
                    text_left = inner_left + size + ART_MARGIN;
                }
            }

            // 曲名とアーティスト（縦中央、はみ出す分は … で省略）
            let text_width = (width - BORDER - PADDING - text_left).max(1.0);
            let face = wide(&ui_font_face());
            let locale = wide(match crate::strings::current() {
                crate::strings::Language::Japanese => "ja-jp",
                crate::strings::Language::English => "en-us",
            });
            let title = self.text_layout(&self.title, &face, &locale, TITLE_SIZE, DWRITE_FONT_WEIGHT_SEMI_BOLD, text_width)?;
            let artist = self.text_layout(&self.artist, &face, &locale, ARTIST_SIZE, DWRITE_FONT_WEIGHT_NORMAL, text_width)?;
            let th = layout_height(&title)?;
            let ah = layout_height(&artist)?;
            let top = ((height - (th + LINE_GAP + ah)) / 2.0).max(0.0);

            let title_brush = rt.CreateSolidColorBrush(&self.colors.title, None)?;
            let artist_brush = rt.CreateSolidColorBrush(&self.colors.artist, None)?;
            rt.DrawTextLayout(D2D_POINT_2F { x: text_left, y: top }, &title, &title_brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            rt.DrawTextLayout(D2D_POINT_2F { x: text_left, y: top + th + LINE_GAP }, &artist, &artist_brush, D2D1_DRAW_TEXT_OPTIONS_NONE);

            rt.EndDraw(None, None)
        }
    }

    unsafe fn draw_album_art(&self, rt: &ID2D1DCRenderTarget, img: &Image, left: f32, top: f32, size: f32) -> Result<()> {
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

        let rect = D2D1_ROUNDED_RECT {
            rect: D2D_RECT_F { left, top, right: left + size, bottom: top + size },
            radiusX: ART_CORNER_RADIUS,
            radiusY: ART_CORNER_RADIUS,
        };
        rt.FillRoundedRectangle(&rect, &brush);
        Ok(())
    }

    unsafe fn text_layout(
        &self,
        text: &str,
        face: &crate::win::WideString,
        locale: &crate::win::WideString,
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

    /// 現在のアニメーションの状態で、画面上のウィンドウを更新する。
    fn present(&mut self) {
        let now = Instant::now();
        let (offset, fade) = match self.phase {
            Phase::Hidden => return,
            Phase::Showing { since } => {
                let t = now - since;
                let slide = progress(t, SLIDE_IN);
                let eased = 1.0 - (1.0 - slide).powi(3); // CubicEase (EaseOut)
                let offset = (scale(SLIDE_DISTANCE, self.dpi) as f64 * (1.0 - eased)).round() as i32;
                (offset, progress(t, FADE_IN))
            }
            Phase::FadingOut { since } => (0, 1.0 - progress(now - since, FADE_OUT)),
        };

        let (Some(content), Some(frame)) = (&self.content, &mut self.frame) else { return };
        compose_shifted(content, frame, offset);

        let alpha = (self.opacity * fade * 255.0).round().clamp(0.0, 255.0) as u8;
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: alpha, AlphaFormat: AC_SRC_ALPHA as u8 };
        let size = SIZE { cx: frame.width, cy: frame.height };
        unsafe {
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&self.origin),
                Some(&size),
                frame.dc,
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    fn on_timer(&mut self, id: usize) {
        match (id, self.phase) {
            (TIMER_HOLD, Phase::Showing { .. }) => {
                unsafe {
                    let _ = KillTimer(self.hwnd, TIMER_HOLD);
                    SetTimer(self.hwnd, TIMER_FRAME, FRAME_MS, None);
                }
                self.phase = Phase::FadingOut { since: Instant::now() };
            }
            (TIMER_FRAME, Phase::Showing { since }) => {
                if since.elapsed() >= SLIDE_IN.max(FADE_IN) {
                    // 表示し終わったら、フェードアウトまでフレーム更新は不要
                    unsafe {
                        let _ = KillTimer(self.hwnd, TIMER_FRAME);
                    }
                }
            }
            (TIMER_FRAME, Phase::FadingOut { since }) if since.elapsed() >= FADE_OUT => {
                self.hide();
                return;
            }
            _ => {}
        }
        self.present();
    }
}

fn progress(elapsed: Duration, total: Duration) -> f64 {
    (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
}

fn layout_height(layout: &IDWriteTextLayout) -> Result<f32> {
    let mut m = DWRITE_TEXT_METRICS::default();
    unsafe { layout.GetMetrics(&mut m)? };
    Ok(m.height)
}

/// `content` を右に `offset` ピクセルずらして `frame` に写す（はみ出した分は切り落とし、空いた所は透明）。
fn compose_shifted(content: &Dib, frame: &mut Dib, offset: i32) {
    let w = content.width as usize;
    let offset = (offset.max(0) as usize).min(w);
    let src = content.pixels();
    let dst = frame.pixels_mut();
    for (src_row, dst_row) in src.chunks_exact(w * 4).zip(dst.chunks_exact_mut(w * 4)) {
        dst_row[..offset * 4].fill(0);
        dst_row[offset * 4..].copy_from_slice(&src_row[..(w - offset) * 4]);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            with(|p| p.on_timer(wparam.0));
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONUP => {
            hide();
            if let Some(handler) = CLICK_HANDLER.with(|h| h.get()) {
                handler();
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
