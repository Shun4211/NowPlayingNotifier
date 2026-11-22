//! 設定の保存と読み込み。ファイルの場所と JSON の形式は C# 版と同じなので、そのまま引き継げる。

use std::path::PathBuf;

use serde_json::{json, Map, Value};

use crate::hotkey::HotkeyBinding;
use crate::strings::Language;

pub const MIN_POPUP_WIDTH: i32 = 200;
pub const MAX_POPUP_WIDTH: i32 = 600;
pub const MIN_POPUP_HEIGHT: i32 = 80;
pub const MAX_POPUP_HEIGHT: i32 = 300;
pub const MIN_DISPLAY_DURATION: i32 = 1;
pub const MAX_DISPLAY_DURATION: i32 = 15;
pub const MIN_POPUP_OPACITY: f64 = 0.2;
pub const MAX_POPUP_OPACITY: f64 = 1.0;
pub const DEFAULT_POPUP_OPACITY: f64 = 0.67;
pub const MIN_POPUP_MARGIN: i32 = 0;
pub const MAX_POPUP_MARGIN: i32 = 100;
pub const DEFAULT_POPUP_MARGIN: i32 = 20;
pub const DEFAULT_DISPLAY_DURATION: i32 = 5;
pub const MIN_TASKBAR_WIDTH: i32 = 120;
pub const MAX_TASKBAR_WIDTH: i32 = 400;
pub const DEFAULT_TASKBAR_WIDTH: i32 = 240;
pub const MIN_TASKBAR_FONT_SIZE: i32 = 9;
pub const MAX_TASKBAR_FONT_SIZE: i32 = 16;
pub const DEFAULT_TASKBAR_FONT_SIZE: i32 = 12;
pub const MIN_TASKBAR_OFFSET: i32 = 0;
pub const MAX_TASKBAR_OFFSET: i32 = 300;
pub const DEFAULT_TASKBAR_OFFSET: i32 = 6;

/// タスクバーのどこに曲名を表示するか。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarPosition {
    /// 通知領域（時計など）の左隣
    NearTray = 0,
    /// タスクバーの左端
    LeftEdge = 1,
}

impl TaskbarPosition {
    pub const ALL: [Self; 2] = [Self::NearTray, Self::LeftEdge];

    fn from_i64(v: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|p| *p as i64 == v)
    }
}

/// 一時停止中のタスクバー表示。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarPaused {
    /// 表示しない
    Hide = 0,
    /// 薄くして一時停止マークを付ける
    Indicate = 1,
}

impl TaskbarPaused {
    pub const ALL: [Self; 2] = [Self::Hide, Self::Indicate];

    fn from_i64(v: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|p| *p as i64 == v)
    }
}

/// タスクバー表示の行数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskbarLayout {
    /// タスクバーの高さに収まれば2行、収まらなければ1行
    Auto = 0,
    OneLine = 1,
    TwoLines = 2,
}

impl TaskbarLayout {
    pub const ALL: [Self; 3] = [Self::Auto, Self::OneLine, Self::TwoLines];

    fn from_i64(v: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|p| *p as i64 == v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopupPosition {
    TopLeft = 0,
    TopRight = 1,
    BottomLeft = 2,
    BottomRight = 3,
}

impl PopupPosition {
    pub const ALL: [Self; 4] = [Self::TopLeft, Self::TopRight, Self::BottomLeft, Self::BottomRight];

    fn from_i64(v: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|p| *p as i64 == v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Dark = 0,
    Light = 1,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    // 表示
    pub position: PopupPosition,
    /// 表示先モニターのデバイス名。空ならメインモニター。
    pub monitor_device_name: String,
    pub popup_width: i32,
    pub popup_height: i32,
    /// 画面端からポップアップまでの余白（上下左右共通）。
    pub popup_margin: i32,
    pub display_duration_seconds: i32,
    pub popup_opacity: f64,
    pub theme: Theme,

    // 動作
    pub show_album_art: bool,
    pub suppress_in_fullscreen: bool,
    pub start_with_windows: bool,
    /// 起動時と1日1回、新しいバージョンがあるか確認する。
    pub check_for_updates: bool,
    pub language: Language,

    // タスクバー表示
    /// タスクバーに再生中の曲名を小さく表示する。
    pub show_in_taskbar: bool,
    pub taskbar_position: TaskbarPosition,
    pub taskbar_layout: TaskbarLayout,
    pub taskbar_paused: TaskbarPaused,
    /// 表示の最大幅（曲名が短ければ縮む）。
    pub taskbar_max_width: i32,
    pub taskbar_font_size: i32,
    /// 基準の位置（通知領域の左端、またはタスクバーの左端）からの間隔。
    pub taskbar_offset: i32,
    pub taskbar_show_artist: bool,
    pub taskbar_show_album_art: bool,

    // ホットキー
    pub play_pause_hotkey: HotkeyBinding,
    pub next_track_hotkey: HotkeyBinding,
    pub previous_track_hotkey: HotkeyBinding,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            position: PopupPosition::TopRight,
            monitor_device_name: String::new(),
            popup_width: 320,
            popup_height: 120,
            popup_margin: DEFAULT_POPUP_MARGIN,
            display_duration_seconds: DEFAULT_DISPLAY_DURATION,
            popup_opacity: DEFAULT_POPUP_OPACITY,
            theme: Theme::Dark,
            show_album_art: true,
            suppress_in_fullscreen: true,
            start_with_windows: false,
            check_for_updates: true,
            language: Language::system_default(),
            show_in_taskbar: true,
            taskbar_position: TaskbarPosition::NearTray,
            taskbar_layout: TaskbarLayout::Auto,
            taskbar_paused: TaskbarPaused::Indicate,
            taskbar_max_width: DEFAULT_TASKBAR_WIDTH,
            taskbar_font_size: DEFAULT_TASKBAR_FONT_SIZE,
            taskbar_offset: DEFAULT_TASKBAR_OFFSET,
            taskbar_show_artist: true,
            taskbar_show_album_art: true,
            play_pause_hotkey: HotkeyBinding::DEFAULT_PLAY_PAUSE,
            next_track_hotkey: HotkeyBinding::DEFAULT_NEXT_TRACK,
            previous_track_hotkey: HotkeyBinding::DEFAULT_PREVIOUS_TRACK,
        }
    }
}

fn settings_path_for(app_name: &str) -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(app_name).join("settings.json"))
}

fn settings_path() -> Option<PathBuf> {
    settings_path_for(crate::system::APP_NAME)
}

impl Settings {
    pub fn load() -> Self {
        let read = |p: PathBuf| std::fs::read_to_string(p).ok();
        if let Some(json) = settings_path().and_then(read) {
            return Self::from_json(&json).unwrap_or_default();
        }
        // 旧名（OtherPlayingNotifier。C# 版も同じ場所）の設定があれば、新しい場所にコピーして使う
        let Some(json) = settings_path_for(crate::system::LEGACY_APP_NAME).and_then(read) else {
            return Self::default();
        };
        if let Some(path) = settings_path() {
            let copied = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&path, &json));
            if let Err(e) = copied {
                eprintln!("failed to migrate settings: {e}");
            }
        }
        Self::from_json(&json).unwrap_or_default()
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        self.normalize();
        let path = settings_path().ok_or_else(|| std::io::Error::other("APPDATA is not set"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_json())
    }

    /// JSON から読み込む。足りない項目は既定値、範囲外の値は範囲内に収める。
    /// JSON として壊れている・オブジェクトでない場合は None。
    pub fn from_json(json: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(json.trim_start_matches('\u{feff}')).ok()?;
        let obj = value.as_object()?;
        let mut s = Self::default();

        let int = |k: &str| obj.get(k).and_then(Value::as_i64);
        let boolean = |k: &str| obj.get(k).and_then(Value::as_bool);
        let hotkey = |k: &str| {
            obj.get(k).and_then(|v| serde_json::from_value::<HotkeyBinding>(v.clone()).ok())
        };

        // 範囲外の列挙値は既定値に戻す（C# 版の Normalize と同じ）
        if let Some(v) = int("Position") {
            s.position = PopupPosition::from_i64(v).unwrap_or(PopupPosition::TopRight);
        }
        if let Some(v) = int("Theme") {
            s.theme = if v == Theme::Light as i64 { Theme::Light } else { Theme::Dark };
        }
        if let Some(v) = int("Language") {
            s.language = Language::from_i64(v).unwrap_or_else(Language::system_default);
        }
        if let Some(v) = obj.get("MonitorDeviceName").and_then(Value::as_str) {
            s.monitor_device_name = v.to_string();
        }
        let clamp_i32 = |v: i64| v.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        if let Some(v) = int("PopupWidth") { s.popup_width = clamp_i32(v); }
        if let Some(v) = int("PopupHeight") { s.popup_height = clamp_i32(v); }
        if let Some(v) = int("PopupMargin") { s.popup_margin = clamp_i32(v); }
        if let Some(v) = int("DisplayDurationSeconds") { s.display_duration_seconds = clamp_i32(v); }
        if let Some(v) = obj.get("PopupOpacity").and_then(Value::as_f64) { s.popup_opacity = v; }
        if let Some(v) = boolean("ShowAlbumArt") { s.show_album_art = v; }
        if let Some(v) = boolean("SuppressInFullscreen") { s.suppress_in_fullscreen = v; }
        if let Some(v) = boolean("ShowInTaskbar") { s.show_in_taskbar = v; }
        if let Some(v) = int("TaskbarPosition") {
            s.taskbar_position = TaskbarPosition::from_i64(v).unwrap_or(TaskbarPosition::NearTray);
        }
        if let Some(v) = int("TaskbarLayout") {
            s.taskbar_layout = TaskbarLayout::from_i64(v).unwrap_or(TaskbarLayout::Auto);
        }
        if let Some(v) = int("TaskbarPaused") {
            s.taskbar_paused = TaskbarPaused::from_i64(v).unwrap_or(TaskbarPaused::Indicate);
        }
        if let Some(v) = int("TaskbarMaxWidth") { s.taskbar_max_width = clamp_i32(v); }
        if let Some(v) = int("TaskbarFontSize") { s.taskbar_font_size = clamp_i32(v); }
        if let Some(v) = int("TaskbarOffset") { s.taskbar_offset = clamp_i32(v); }
        if let Some(v) = boolean("TaskbarShowArtist") { s.taskbar_show_artist = v; }
        if let Some(v) = boolean("TaskbarShowAlbumArt") { s.taskbar_show_album_art = v; }
        if let Some(v) = boolean("StartWithWindows") { s.start_with_windows = v; }
        if let Some(v) = boolean("CheckForUpdates") { s.check_for_updates = v; }
        if let Some(v) = hotkey("PlayPauseHotkey") { s.play_pause_hotkey = v; }
        if let Some(v) = hotkey("NextTrackHotkey") { s.next_track_hotkey = v; }
        if let Some(v) = hotkey("PreviousTrackHotkey") { s.previous_track_hotkey = v; }

        s.normalize();
        Some(s)
    }

    pub fn to_json(&self) -> String {
        let mut m = Map::new();
        m.insert("Position".into(), json!(self.position as i64));
        m.insert("MonitorDeviceName".into(), json!(self.monitor_device_name));
        m.insert("PopupWidth".into(), json!(self.popup_width));
        m.insert("PopupHeight".into(), json!(self.popup_height));
        m.insert("PopupMargin".into(), json!(self.popup_margin));
        m.insert("DisplayDurationSeconds".into(), json!(self.display_duration_seconds));
        m.insert("PopupOpacity".into(), json!(self.popup_opacity));
        m.insert("Theme".into(), json!(self.theme as i64));
        m.insert("ShowAlbumArt".into(), json!(self.show_album_art));
        m.insert("SuppressInFullscreen".into(), json!(self.suppress_in_fullscreen));
        m.insert("StartWithWindows".into(), json!(self.start_with_windows));
        m.insert("CheckForUpdates".into(), json!(self.check_for_updates));
        m.insert("Language".into(), json!(self.language as i64));
        m.insert("ShowInTaskbar".into(), json!(self.show_in_taskbar));
        m.insert("TaskbarPosition".into(), json!(self.taskbar_position as i64));
        m.insert("TaskbarLayout".into(), json!(self.taskbar_layout as i64));
        m.insert("TaskbarPaused".into(), json!(self.taskbar_paused as i64));
        m.insert("TaskbarMaxWidth".into(), json!(self.taskbar_max_width));
        m.insert("TaskbarFontSize".into(), json!(self.taskbar_font_size));
        m.insert("TaskbarOffset".into(), json!(self.taskbar_offset));
        m.insert("TaskbarShowArtist".into(), json!(self.taskbar_show_artist));
        m.insert("TaskbarShowAlbumArt".into(), json!(self.taskbar_show_album_art));
        m.insert("PlayPauseHotkey".into(), json!(self.play_pause_hotkey));
        m.insert("NextTrackHotkey".into(), json!(self.next_track_hotkey));
        m.insert("PreviousTrackHotkey".into(), json!(self.previous_track_hotkey));
        serde_json::to_string_pretty(&Value::Object(m)).unwrap_or_default()
    }

    /// 手編集などで範囲外になった値を許容範囲に収める。
    pub fn normalize(&mut self) {
        self.popup_width = self.popup_width.clamp(MIN_POPUP_WIDTH, MAX_POPUP_WIDTH);
        self.popup_height = self.popup_height.clamp(MIN_POPUP_HEIGHT, MAX_POPUP_HEIGHT);
        self.popup_margin = self.popup_margin.clamp(MIN_POPUP_MARGIN, MAX_POPUP_MARGIN);
        self.display_duration_seconds =
            self.display_duration_seconds.clamp(MIN_DISPLAY_DURATION, MAX_DISPLAY_DURATION);
        self.popup_opacity = if self.popup_opacity.is_finite() {
            self.popup_opacity.clamp(MIN_POPUP_OPACITY, MAX_POPUP_OPACITY)
        } else {
            DEFAULT_POPUP_OPACITY
        };
        self.taskbar_max_width = self.taskbar_max_width.clamp(MIN_TASKBAR_WIDTH, MAX_TASKBAR_WIDTH);
        self.taskbar_font_size = self.taskbar_font_size.clamp(MIN_TASKBAR_FONT_SIZE, MAX_TASKBAR_FONT_SIZE);
        self.taskbar_offset = self.taskbar_offset.clamp(MIN_TASKBAR_OFFSET, MAX_TASKBAR_OFFSET);

        if !self.play_pause_hotkey.is_valid() {
            self.play_pause_hotkey = HotkeyBinding::DEFAULT_PLAY_PAUSE;
        }
        if !self.next_track_hotkey.is_valid() {
            self.next_track_hotkey = HotkeyBinding::DEFAULT_NEXT_TRACK;
        }
        if !self.previous_track_hotkey.is_valid() {
            self.previous_track_hotkey = HotkeyBinding::DEFAULT_PREVIOUS_TRACK;
        }
    }

    /// 有効なホットキーの中に同じ組み合わせがあるか。
    pub fn has_duplicate_hotkeys(&self) -> bool {
        let keys: Vec<_> = [self.play_pause_hotkey, self.next_track_hotkey, self.previous_track_hotkey]
            .into_iter()
            .filter(HotkeyBinding::is_enabled)
            .collect();
        (0..keys.len()).any(|i| keys[i + 1..].contains(&keys[i]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::{MOD_CONTROL, MOD_SHIFT, MOD_WIN};

    #[test]
    fn normalize_clamps_out_of_range_values() {
        let mut s = Settings {
            popup_width: 10,
            popup_height: 10_000,
            display_duration_seconds: 0,
            popup_opacity: 0.0,
            popup_margin: 500,
            ..Default::default()
        };
        s.normalize();
        assert_eq!(s.popup_margin, MAX_POPUP_MARGIN);
        assert_eq!(s.popup_width, MIN_POPUP_WIDTH);
        assert_eq!(s.popup_height, MAX_POPUP_HEIGHT);
        assert_eq!(s.display_duration_seconds, MIN_DISPLAY_DURATION);
        assert_eq!(s.popup_opacity, MIN_POPUP_OPACITY);
    }

    #[test]
    fn taskbar_values_are_clamped_and_reset() {
        let s = Settings::from_json(
            r#"{ "TaskbarPosition": 9, "TaskbarLayout": -1, "TaskbarPaused": 7, "TaskbarMaxWidth": 5000, "TaskbarFontSize": 1, "TaskbarOffset": -10 }"#,
        )
        .unwrap();
        assert_eq!(s.taskbar_position, TaskbarPosition::NearTray);
        assert_eq!(s.taskbar_layout, TaskbarLayout::Auto);
        assert_eq!(s.taskbar_paused, TaskbarPaused::Indicate);
        assert_eq!(s.taskbar_max_width, MAX_TASKBAR_WIDTH);
        assert_eq!(s.taskbar_font_size, MIN_TASKBAR_FONT_SIZE);
        assert_eq!(s.taskbar_offset, MIN_TASKBAR_OFFSET);
    }

    #[test]
    fn normalize_replaces_nan_opacity_with_default() {
        let mut s = Settings { popup_opacity: f64::NAN, ..Default::default() };
        s.normalize();
        assert_eq!(s.popup_opacity, DEFAULT_POPUP_OPACITY);
    }

    #[test]
    fn undefined_enum_values_are_reset() {
        let s = Settings::from_json(r#"{ "Position": 99, "Theme": 99 }"#).unwrap();
        assert_eq!(s.position, PopupPosition::TopRight);
        assert_eq!(s.theme, Theme::Dark);
    }

    #[test]
    fn invalid_hotkey_is_reset_to_default() {
        // 修飾キーなしの "A" は登録できない
        let s = Settings::from_json(r#"{ "PlayPauseHotkey": { "Modifiers": 0, "VirtualKey": 65 } }"#).unwrap();
        assert_eq!(s.play_pause_hotkey, HotkeyBinding::DEFAULT_PLAY_PAUSE);
    }

    #[test]
    fn disabled_hotkey_is_kept() {
        let mut s = Settings { next_track_hotkey: HotkeyBinding::NONE, ..Default::default() };
        s.normalize();
        assert_eq!(s.next_track_hotkey, HotkeyBinding::NONE);
    }

    #[test]
    fn json_round_trips_all_values() {
        let original = Settings {
            position: PopupPosition::BottomLeft,
            monitor_device_name: r"\\.\DISPLAY2".into(),
            popup_width: 400,
            popup_height: 150,
            popup_margin: 0,
            display_duration_seconds: 8,
            popup_opacity: 0.9,
            theme: Theme::Light,
            show_album_art: false,
            suppress_in_fullscreen: false,
            start_with_windows: true,
            check_for_updates: false,
            language: Language::English,
            show_in_taskbar: false,
            taskbar_position: TaskbarPosition::LeftEdge,
            taskbar_layout: TaskbarLayout::TwoLines,
            taskbar_paused: TaskbarPaused::Hide,
            taskbar_max_width: 300,
            taskbar_font_size: 14,
            taskbar_offset: 40,
            taskbar_show_artist: false,
            taskbar_show_album_art: false,
            play_pause_hotkey: HotkeyBinding::new(MOD_SHIFT | MOD_WIN, 0x50),
            next_track_hotkey: HotkeyBinding::NONE,
            previous_track_hotkey: HotkeyBinding::DEFAULT_PREVIOUS_TRACK,
        };
        assert_eq!(Settings::from_json(&original.to_json()).unwrap(), original);
    }

    #[test]
    fn old_format_without_new_fields_uses_defaults() {
        let json = r#"{ "Position": 2, "PopupWidth": 350, "PopupHeight": 100, "DisplayDurationSeconds": 3, "PopupOpacity": 0.5, "Theme": 1 }"#;
        let s = Settings::from_json(json).unwrap();
        assert_eq!(s.position, PopupPosition::BottomLeft);
        assert_eq!(s.popup_width, 350);
        assert!(s.show_album_art);
        assert!(s.show_in_taskbar);
        assert!(s.check_for_updates);
        assert_eq!(s.popup_margin, DEFAULT_POPUP_MARGIN);
        assert_eq!(s.monitor_device_name, "");
        assert_eq!(s.play_pause_hotkey, HotkeyBinding::DEFAULT_PLAY_PAUSE);
    }

    #[test]
    fn reads_file_written_by_csharp_version() {
        // C# 版 (System.Text.Json, WriteIndented) が書き出す形式
        let json = "\u{feff}{\n  \"Position\": 3,\n  \"MonitorDeviceName\": \"\\\\\\\\.\\\\DISPLAY1\",\n  \"PopupWidth\": 320,\n  \"PopupHeight\": 120,\n  \"PopupMargin\": 20,\n  \"DisplayDurationSeconds\": 5,\n  \"PopupOpacity\": 0.67,\n  \"Theme\": 0,\n  \"ShowAlbumArt\": true,\n  \"SuppressInFullscreen\": true,\n  \"StartWithWindows\": false,\n  \"Language\": 0,\n  \"PlayPauseHotkey\": {\n    \"Modifiers\": 3,\n    \"VirtualKey\": 32,\n    \"IsEnabled\": true\n  },\n  \"NextTrackHotkey\": {\n    \"Modifiers\": 3,\n    \"VirtualKey\": 39,\n    \"IsEnabled\": true\n  },\n  \"PreviousTrackHotkey\": {\n    \"Modifiers\": 0,\n    \"VirtualKey\": 0,\n    \"IsEnabled\": false\n  }\n}";
        let s = Settings::from_json(json).unwrap();
        assert_eq!(s.position, PopupPosition::BottomRight);
        assert_eq!(s.monitor_device_name, r"\\.\DISPLAY1");
        assert_eq!(s.language, Language::Japanese);
        assert_eq!(s.next_track_hotkey, HotkeyBinding::DEFAULT_NEXT_TRACK);
        assert_eq!(s.previous_track_hotkey, HotkeyBinding::NONE);
    }

    #[test]
    fn broken_json_is_rejected() {
        assert!(Settings::from_json("{ not json").is_none());
        assert!(Settings::from_json("[]").is_none());
    }

    #[test]
    fn duplicate_hotkeys_are_detected_but_disabled_ones_ignored() {
        let mut s = Settings::default();
        assert!(!s.has_duplicate_hotkeys());
        s.next_track_hotkey = s.play_pause_hotkey;
        assert!(s.has_duplicate_hotkeys());
        s.next_track_hotkey = HotkeyBinding::NONE;
        s.previous_track_hotkey = HotkeyBinding::NONE;
        assert!(!s.has_duplicate_hotkeys());
        let _ = MOD_CONTROL;
    }
}
