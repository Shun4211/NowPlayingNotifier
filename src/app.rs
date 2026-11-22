//! アプリ全体の制御。非表示の制御用ウィンドウでトレイ・ホットキー・イベントのメッセージを受け取る。

use std::cell::RefCell;

use windows::core::{w, Result};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::debug_log::debug_log;
use crate::hotkeys::{HotkeyAction, Hotkeys};
use crate::media::{self, AlbumArt, MediaEvent, MediaWatcher, TrackInfo, UpdateResult};
use crate::settings::Settings;
use crate::settings_window::{self, Callbacks};
use crate::strings;
use crate::system::{self, monitor_or_primary};
use crate::track::{TrackChange, TrackChangeDetector};
use crate::win::{loword, take_events, AppEvent, WM_APP_EVENT};
use crate::{main_window, popup, taskbar, tray};

/// 曲が変わった後、曲情報を取り直す間隔と回数。
/// アプリによっては、曲名より後でアルバムアートを差し替え、その通知を出さないことがあるため。
const TIMER_RECHECK: usize = 1;
const RECHECK_INTERVAL_MS: u32 = 2000;
const RECHECK_COUNT: u32 = 2;
/// 再生中でないセッションへの切り替えを保留したとき、選び直すまでの時間。
const TIMER_DEFERRED: usize = 2;
const DEFERRED_DELAY_MS: u32 = 1000;
/// 一時停止中の情報が届いたとき、本当に停止したのか確かめるまでの時間。
/// 曲の切り替わりの一瞬は「停止中」になり、Chromium 系のブラウザでは別のタブの（停止中の）曲が
/// 同じセッションに一瞬入ってくるので、すぐには表示しない。
const TIMER_CONFIRM_PAUSE: usize = 3;
const CONFIRM_PAUSE_MS: u32 = 2000;

/// 一時停止中の情報の扱い。
#[derive(Clone, Copy, PartialEq, Debug)]
enum PauseCheck {
    /// 再生中（または何も表示していない）
    None,
    /// 停止中の情報が届いたので、少し待って確かめている
    Waiting,
    /// 停止中が続いていたので、その情報を表示してよい
    Confirmed,
}

struct App {
    controller: HWND,
    settings: Settings,
    watcher: Option<MediaWatcher>,
    detector: TrackChangeDetector,
    /// 表示中の曲（アルバムアートの遅延読み込みとクリック時の前面化に使う）
    current_track: Option<TrackInfo>,
    /// 表示中のアルバムアートの元データのハッシュ。同じ画像を何度もデコードしないために使う。
    album_art_hash: Option<u64>,
    album_art_version: u64,
    rechecks_left: u32,
    pause_check: PauseCheck,
    hotkeys: Hotkeys,
    taskbar_created: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

/// アプリの状態を短い間だけ借りて処理する。
/// メニュー表示など、中でメッセージを処理する呼び出しはこの外で行うこと（二重に借りないように）。
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

pub fn start() -> Result<()> {
    let settings = Settings::load();
    strings::set_current(settings.language);

    let controller = unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: w!("OtherPlayingNotifier.Controller"),
            ..Default::default()
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("OtherPlayingNotifier.Controller"),
            w!("OtherPlayingNotifier"),
            WS_OVERLAPPED,
            0, 0, 0, 0,
            None, None, hinstance, None,
        )?
    };
    crate::win::set_event_target(controller);

    main_window::create()?;
    main_window::set_text(strings::standby(), "");
    popup::create(on_popup_clicked)?;
    popup::apply_settings(&settings);
    taskbar::create(taskbar::Callbacks { click: on_popup_clicked, menu: show_tray_menu })?;
    taskbar::apply_settings(&settings);
    tray::add(controller);

    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            controller,
            settings,
            watcher: None,
            detector: TrackChangeDetector::default(),
            current_track: None,
            album_art_hash: None,
            album_art_version: 0,
            rechecks_left: 0,
            pause_check: PauseCheck::None,
            hotkeys: Hotkeys::new(controller),
            taskbar_created,
        })
    });

    with_app(|app| {
        app.apply_startup_registration();
        app.register_hotkeys();
    });
    media::request_manager();
    Ok(())
}

impl App {
    fn balloon(&self, title: &str, message: &str) {
        tray::show_balloon(self.controller, title, message);
    }

    fn apply_startup_registration(&self) {
        if let Err(e) = system::apply_startup_registration(self.settings.start_with_windows) {
            self.balloon(strings::startup_failed_title(), &e.message());
        }
    }

    fn register_hotkeys(&mut self) {
        let failed = self.hotkeys.register(&self.settings);
        if !failed.is_empty() {
            self.balloon(strings::hotkey_failed_title(), &strings::hotkey_failed_message(&failed.join(", ")));
        }
    }

    fn on_media_event(&mut self, event: MediaEvent) {
        match event {
            MediaEvent::ManagerReady(Ok(manager)) => match MediaWatcher::new(manager) {
                Ok(watcher) => {
                    self.watcher = Some(watcher);
                    self.update_media();
                }
                Err(e) => self.media_error(&e.message()),
            },
            MediaEvent::ManagerReady(Err(message)) => self.media_error(&message),
            MediaEvent::SessionChanged => {
                debug_log!("event: SessionChanged");
                self.update_media();
            }
            MediaEvent::PropertiesChanged => {
                debug_log!("event: PropertiesChanged");
                self.update_media();
            }
            MediaEvent::TrackFetched { version, result } => {
                let latest = self.watcher.as_ref().is_some_and(|w| w.is_latest(version));
                match &result {
                    Ok(t) => debug_log!(
                        "fetched v{version} latest={latest}: title={:?} artist={:?} playing={} thumb={}",
                        t.title,
                        t.artist,
                        t.playing,
                        t.thumbnail.is_some()
                    ),
                    Err(e) => debug_log!("fetched v{version} latest={latest}: error {e}"),
                }
                if !latest {
                    return; // 待っている間に新しい更新が始まっていた
                }
                match result {
                    Ok(track) => self.on_track(Some(track)),
                    Err(e) => {
                        eprintln!("failed to get media properties: {e}");
                        main_window::set_text(strings::error(), "");
                    }
                }
            }
            MediaEvent::AlbumArtLoaded { version, result } => {
                debug_log!(
                    "art v{version} (latest v{}): {}",
                    self.album_art_version,
                    match &result {
                        AlbumArt::Loaded { hash, .. } => format!("loaded {hash:x}"),
                        AlbumArt::Unchanged => "unchanged".into(),
                        AlbumArt::Failed => "failed".into(),
                    }
                );
                // 読み込み中に別の曲に変わった・新しい読み込みが始まっていたら使わない
                if version != self.album_art_version || self.current_track.is_none() {
                    return;
                }
                // 読み込めなかった・同じ画像だったときは、表示中のものをそのまま使う
                // （次に曲情報の更新が届いたときにもう一度読み込む）
                if let AlbumArt::Loaded { image, hash } = result {
                    self.album_art_hash = Some(hash);
                    popup::set_album_art(image.clone());
                    taskbar::set_album_art(image);
                }
            }
        }
    }

    fn media_error(&self, message: &str) {
        main_window::set_text(strings::media_error_title(), message);
        self.balloon(strings::media_error_title(), message);
    }

    fn update_media(&mut self) {
        let Some(watcher) = &mut self.watcher else { return };
        match watcher.update() {
            (UpdateResult::NoSession, _) => self.on_track(None),
            (UpdateResult::Deferred, _) => unsafe {
                SetTimer(self.controller, TIMER_DEFERRED, DEFERRED_DELAY_MS, None);
            },
            (UpdateResult::Fetching, _) => {}
        }
    }

    fn on_track(&mut self, track: Option<TrackInfo>) {
        let Some(track) = track else {
            self.set_pause_check(PauseCheck::None);
            self.current_track = None;
            self.album_art_version += 1;
            self.detector.reset();
            main_window::set_text(strings::standby(), "");
            taskbar::clear();
            return;
        };

        if track.playing {
            self.set_pause_check(PauseCheck::None);
        } else if self.current_track.is_some() {
            // 表示中の曲があるときに届いた停止中の情報は、続いているのを確かめてから使う
            match self.pause_check {
                PauseCheck::Confirmed => {}
                PauseCheck::Waiting => return,
                PauseCheck::None => {
                    self.set_pause_check(PauseCheck::Waiting);
                    return;
                }
            }
        }

        let title = if track.title.is_empty() { strings::no_title() } else { &track.title };
        main_window::set_text(title, &track.artist);
        taskbar::set_track(title, &track.artist);
        taskbar::set_playing(track.playing);

        let change = self.detector.evaluate(&track.title, &track.artist);
        debug_log!("track change: {change:?}");
        match change {
            TrackChange::Empty => {}
            TrackChange::NewTrack => {
                self.current_track = Some(track.clone());
                // ポップアップとタスクバーは曲が変わるとアルバムアートを消すので、同じ画像でも必ず差し込む
                self.album_art_hash = None;
                self.show_track_popup(&track);
                self.load_album_art(&track);
                self.start_rechecks();
            }
            TrackChange::SameTrack => {
                self.current_track = Some(track.clone());
                // 曲名が先に変わり、アルバムアートは少し遅れて（前の曲のものから）差し替わることがある。
                // 更新のたびに読み直し、中身が変わっていれば表示し直す。
                self.load_album_art(&track);
            }
        }
    }

    fn set_pause_check(&mut self, state: PauseCheck) {
        if self.pause_check == state {
            return;
        }
        debug_log!("pause check: {:?} -> {state:?}", self.pause_check);
        self.pause_check = state;
        unsafe {
            if state == PauseCheck::Waiting {
                SetTimer(self.controller, TIMER_CONFIRM_PAUSE, CONFIRM_PAUSE_MS, None);
            } else {
                let _ = KillTimer(self.controller, TIMER_CONFIRM_PAUSE);
            }
        }
    }

    /// 停止中の情報が届いてから少し経った。まだ停止中なら、その情報を表示する。
    fn on_confirm_pause_timer(&mut self) {
        unsafe {
            let _ = KillTimer(self.controller, TIMER_CONFIRM_PAUSE);
        }
        if self.pause_check == PauseCheck::Waiting {
            self.pause_check = PauseCheck::Confirmed;
            debug_log!("pause check: Waiting -> Confirmed");
            self.update_media();
        }
    }

    fn start_rechecks(&mut self) {
        self.rechecks_left = RECHECK_COUNT;
        unsafe {
            SetTimer(self.controller, TIMER_RECHECK, RECHECK_INTERVAL_MS, None);
        }
    }

    fn on_recheck_timer(&mut self) {
        self.rechecks_left = self.rechecks_left.saturating_sub(1);
        if self.rechecks_left == 0 {
            unsafe {
                let _ = KillTimer(self.controller, TIMER_RECHECK);
            }
        }
        if self.current_track.is_some() {
            self.update_media();
        }
    }

    fn show_track_popup(&mut self, track: &TrackInfo) {
        if self.settings.suppress_in_fullscreen && system::is_fullscreen_app_running() {
            return;
        }
        show_popup(&self.settings, &track.title, &track.artist);
    }

    fn load_album_art(&mut self, track: &TrackInfo) {
        if !wants_album_art(&self.settings) {
            return;
        }
        let Some(thumbnail) = track.thumbnail.clone() else { return };
        self.album_art_version += 1;
        media::load_album_art(thumbnail, self.album_art_version, popup::ALBUM_ART_DECODE_WIDTH, self.album_art_hash);
    }

    fn on_hotkey(&self, action: HotkeyAction) {
        let Some(w) = &self.watcher else { return };
        match action {
            HotkeyAction::PlayPause => w.play_pause(),
            HotkeyAction::NextTrack => w.next_track(),
            HotkeyAction::PreviousTrack => w.previous_track(),
        }
    }

    fn apply_settings(&mut self, settings: Settings) {
        let language_changed = settings.language != self.settings.language;
        let startup_changed = settings.start_with_windows != self.settings.start_with_windows;
        let reload_art = (settings.show_album_art && !self.settings.show_album_art)
            || (wants_taskbar_art(&settings) && !wants_taskbar_art(&self.settings));
        self.settings = settings;
        popup::apply_settings(&self.settings);
        taskbar::apply_settings(&self.settings);
        // アルバムアートやタスクバー表示を有効にしたら、今の曲のアルバムアートを読み込む
        if reload_art {
            // 表示側はアルバムアートを捨てているので、同じ画像でも差し込み直す
            self.album_art_hash = None;
            if let Some(track) = self.current_track.clone() {
                self.load_album_art(&track);
            }
        }

        if language_changed {
            strings::set_current(self.settings.language);
            if self.current_track.is_none() {
                main_window::set_text(strings::standby(), "");
            }
        }
        if startup_changed {
            self.apply_startup_registration();
        }
    }
}

fn wants_taskbar_art(s: &Settings) -> bool {
    s.show_in_taskbar && s.taskbar_show_album_art
}

/// ポップアップかタスクバーのどちらかでアルバムアートを使うか。
fn wants_album_art(s: &Settings) -> bool {
    s.show_album_art || wants_taskbar_art(s)
}

fn show_popup(settings: &Settings, title: &str, artist: &str) {
    let Some(monitor) = monitor_or_primary(&settings.monitor_device_name) else { return };
    popup::show(title, artist, settings.display_duration_seconds, settings, monitor.work_area, monitor.dpi);
}

fn on_popup_clicked() {
    let app_id = with_app(|a| a.current_track.as_ref().map(|t| t.source_app_id.clone())).flatten();
    if let Some(id) = app_id {
        system::activate_source_app(&id);
    }
}

fn show_tray_menu() {
    let Some(controller) = with_app(|a| a.controller) else { return };
    if let Some(cmd) = tray::show_menu(controller) {
        run_command(cmd);
    }
}

fn open_settings() {
    let Some(settings) = with_app(|a| {
        // 設定画面でキーを入力できるよう、開いている間はホットキーを解除する
        a.hotkeys.unregister_all();
        a.settings.clone()
    }) else {
        return;
    };
    let owner = main_window::is_visible().then(main_window::hwnd).flatten();
    let result = settings_window::open(owner, settings, Callbacks { preview: on_preview, closed: on_settings_closed });
    if result.is_err() {
        with_app(App::register_hotkeys);
    }
}

fn on_preview(preview: Settings) {
    popup::apply_settings(&preview);
    show_popup(&preview, strings::preview_title(), strings::preview_artist());
    // タスクバー表示も、設定画面を閉じるまで変更後の見た目にする（曲がなければ見本を出す）
    taskbar::apply_settings(&preview);
    taskbar::set_preview(true);
}

fn on_settings_closed(result: Option<Settings>) {
    taskbar::set_preview(false);
    with_app(|a| {
        match result {
            Some(settings) => a.apply_settings(settings),
            None => {
                // キャンセル時はプレビューで変えた見た目を元に戻す
                popup::apply_settings(&a.settings);
                popup::hide();
                taskbar::apply_settings(&a.settings);
            }
        }
        a.register_hotkeys();
    });
}

fn run_command(cmd: usize) {
    match cmd {
        tray::CMD_SHOW => main_window::show(),
        tray::CMD_SETTINGS => open_settings(),
        tray::CMD_EXIT => exit(),
        _ => {}
    }
}

fn exit() {
    if let Some(hwnd) = with_app(|a| a.controller) {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

fn shutdown(controller: HWND) {
    tray::remove(controller);
    // 監視やホットキーを先に止める
    let app = APP.with(|a| a.borrow_mut().take());
    drop(app);
    popup::destroy();
    taskbar::destroy();
    main_window::destroy();
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_APP_EVENT => {
            for event in take_events() {
                match event {
                    AppEvent::Media(e) => {
                        with_app(|a| a.on_media_event(e));
                    }
                    AppEvent::ActivateRequested => main_window::show(),
                }
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_RECHECK => {
            with_app(App::on_recheck_timer);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_CONFIRM_PAUSE => {
            with_app(App::on_confirm_pause_timer);
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TIMER_DEFERRED => {
            let _ = KillTimer(hwnd, TIMER_DEFERRED);
            with_app(App::update_media);
            LRESULT(0)
        }
        WM_HOTKEY => {
            if let Some(action) = HotkeyAction::from_id(wparam.0) {
                with_app(|a| a.on_hotkey(action));
            }
            LRESULT(0)
        }
        tray::WM_TRAY => {
            match loword(lparam.0 as usize) {
                WM_LBUTTONDBLCLK => main_window::show(),
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    if let Some(cmd) = tray::show_menu(hwnd) {
                        run_command(cmd);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        // トレイメニューと同じコマンドを WM_COMMAND でも受け付ける（動作確認の自動化用）
        WM_COMMAND if lparam.0 == 0 => {
            run_command(loword(wparam.0) as usize);
            LRESULT(0)
        }
        WM_ENDSESSION if wparam.0 != 0 => {
            shutdown(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            shutdown(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => {
            // エクスプローラーが再起動したらトレイアイコンを登録し直す
            if msg != 0 && with_app(|a| a.taskbar_created) == Some(msg) {
                tray::add(hwnd);
                taskbar::refresh();
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

/// メッセージループ。設定画面が開いていれば Tab などのキー操作をそちらで処理させる。
pub fn run_message_loop() {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            if let Some(dlg) = settings_window::hwnd() {
                if IsDialogMessageW(dlg, &msg).as_bool() {
                    continue;
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
