//! UI に表示する文字列（日本語 / 英語）。

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Japanese = 0,
    English = 1,
}

impl Language {
    pub fn from_i64(v: i64) -> Option<Self> {
        match v {
            0 => Some(Self::Japanese),
            1 => Some(Self::English),
            _ => None,
        }
    }

    /// OS の表示言語が日本語なら日本語、それ以外は英語。
    pub fn system_default() -> Self {
        #[cfg(windows)]
        {
            let lang = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
            if lang & 0x3FF == 0x11 {
                return Self::Japanese;
            }
        }
        Self::English
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Language::English as u8);

pub fn current() -> Language {
    if CURRENT.load(Ordering::Relaxed) == Language::Japanese as u8 {
        Language::Japanese
    } else {
        Language::English
    }
}

pub fn set_current(lang: Language) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

fn t(ja: &'static str, en: &'static str) -> &'static str {
    match current() {
        Language::Japanese => ja,
        Language::English => en,
    }
}

fn tf(ja: String, en: String) -> String {
    match current() {
        Language::Japanese => ja,
        Language::English => en,
    }
}

// トレイメニュー
pub fn tray_show() -> &'static str { t("表示", "Show") }
pub fn tray_settings() -> &'static str { t("設定", "Settings") }
pub fn tray_exit() -> &'static str { t("終了", "Exit") }

// メインウィンドウ
pub fn standby() -> &'static str { t("待機中...", "Standby...") }
pub fn no_title() -> &'static str { t("（タイトルなし）", "(No Title)") }
pub fn error() -> &'static str { t("エラー", "Error") }

// バルーン通知
pub fn media_error_title() -> &'static str { t("メディア情報を取得できません", "Cannot read media information") }
pub fn hotkey_failed_title() -> &'static str { t("ホットキーを登録できませんでした", "Failed to register hotkeys") }
pub fn hotkey_failed_message(keys: &str) -> String {
    tf(
        format!("{keys} は他のアプリが使用している可能性があります。設定から変更できます。"),
        format!("{keys} may be in use by another app. You can change them in Settings."),
    )
}
pub fn startup_failed_title() -> &'static str { t("自動起動の設定に失敗しました", "Failed to update startup setting") }

// 更新
pub fn update_caption() -> &'static str { t("アップデート", "Update") }
pub fn tray_update(version: &str) -> String {
    tf(format!("バージョン {version} に更新..."), format!("Update to version {version}..."))
}
pub fn update_available_title(version: &str) -> String {
    tf(format!("新しいバージョン {version} があります"), format!("Version {version} is available"))
}
pub fn update_available_message() -> &'static str {
    t("ここをクリックすると更新できます。", "Click here to update.")
}
pub fn update_confirm(new: &str, current: &str) -> String {
    tf(
        format!("バージョン {new} に更新しますか？（今のバージョン: {current}）\n\n更新のあいだアプリは一度終了し、終わると自動で起動し直します。"),
        format!("Update to version {new}? (Current version: {current})\n\nThe app will close during the update and start again when it finishes."),
    )
}
pub fn update_open_page(new: &str, current: &str) -> String {
    tf(
        format!("バージョン {new} が公開されています（今のバージョン: {current}）。\n\nダウンロードページを開きますか？"),
        format!("Version {new} is available (current version: {current}).\n\nOpen the download page?"),
    )
}
pub fn update_up_to_date(current: &str) -> String {
    tf(format!("最新のバージョンです（{current}）。"), format!("You have the latest version ({current})."))
}
pub fn update_downloading() -> &'static str { t("更新をダウンロードしています...", "Downloading the update...") }
pub fn update_check_failed() -> &'static str { t("更新を確認できませんでした", "Could not check for updates") }
pub fn update_download_failed() -> &'static str { t("更新をダウンロードできませんでした", "Could not download the update") }

// 設定画面
pub fn settings_title() -> &'static str { t("設定", "Settings") }
pub fn position_header() -> &'static str { t("表示位置", "Popup Position") }
pub fn top_left() -> &'static str { t("左上", "Top Left") }
pub fn top_right() -> &'static str { t("右上", "Top Right") }
pub fn bottom_left() -> &'static str { t("左下", "Bottom Left") }
pub fn bottom_right() -> &'static str { t("右下", "Bottom Right") }
pub fn monitor_header() -> &'static str { t("表示するモニター", "Display Monitor") }
pub fn primary_monitor() -> &'static str { t("メインモニター", "Primary monitor") }
pub fn monitor_item(index: usize, width: i32, height: i32, primary: bool) -> String {
    tf(
        format!("モニター {index}（{width}×{height}）{}", if primary { "［メイン］" } else { "" }),
        format!("Monitor {index} ({width}×{height}){}", if primary { " [Primary]" } else { "" }),
    )
}
pub fn size_header() -> &'static str { t("サイズ", "Popup Size") }
pub fn width() -> &'static str { t("幅:", "Width:") }
pub fn height() -> &'static str { t("高さ:", "Height:") }
pub fn margin() -> &'static str { t("画面端の余白:", "Edge margin:") }
pub fn duration_header() -> &'static str { t("表示時間", "Display Duration") }
pub fn seconds(value: i32) -> String { tf(format!("{value} 秒"), format!("{value} sec")) }
pub fn opacity_header() -> &'static str { t("透明度", "Opacity") }
pub fn theme_header() -> &'static str { t("テーマ", "Theme") }
pub fn theme_dark() -> &'static str { t("ダーク", "Dark") }
pub fn theme_light() -> &'static str { t("ライト", "Light") }
pub fn behavior_header() -> &'static str { t("動作", "Behavior") }
pub fn show_album_art() -> &'static str { t("アルバムアートを表示する", "Show album art") }
pub fn suppress_in_fullscreen() -> &'static str {
    t("全画面表示中は通知しない", "Don't notify while a full-screen app is running")
}

// 設定画面: タスクバー表示
pub fn taskbar_header() -> &'static str { t("タスクバー表示", "Taskbar Display") }
pub fn show_in_taskbar() -> &'static str { t("タスクバーに曲名を表示する", "Show the current track on the taskbar") }
pub fn taskbar_position() -> &'static str { t("位置:", "Position:") }
pub fn taskbar_near_tray() -> &'static str { t("時計の左", "Next to clock") }
pub fn taskbar_left_edge() -> &'static str { t("左端", "Left edge") }
pub fn taskbar_layout() -> &'static str { t("行数:", "Lines:") }
pub fn taskbar_layout_auto() -> &'static str { t("自動", "Auto") }
pub fn taskbar_layout_one() -> &'static str { t("1行", "One line") }
pub fn taskbar_layout_two() -> &'static str { t("2行", "Two lines") }
pub fn taskbar_paused() -> &'static str { t("停止中:", "When paused:") }
pub fn taskbar_paused_hide() -> &'static str { t("非表示", "Hide") }
pub fn taskbar_paused_indicate() -> &'static str { t("停止中と表示", "Show as paused") }
pub fn taskbar_max_width() -> &'static str { t("最大幅:", "Max width:") }
pub fn taskbar_font_size() -> &'static str { t("文字サイズ:", "Font size:") }
pub fn taskbar_offset() -> &'static str { t("間隔:", "Offset:") }
pub fn taskbar_show_artist() -> &'static str { t("アーティスト名を表示する", "Show artist") }
pub fn taskbar_show_album_art() -> &'static str { t("アルバムアートを表示する", "Show album art") }
pub fn start_with_windows() -> &'static str { t("Windows の起動時に自動で起動する", "Start with Windows") }
pub fn check_for_updates() -> &'static str { t("更新を自動で確認する", "Check for updates automatically") }
pub fn check_now() -> &'static str { t("更新を確認", "Check for updates") }
pub fn hotkeys_header() -> &'static str { t("ホットキー", "Hotkeys") }
pub fn hotkey_hint() -> &'static str {
    t("欄をクリックしてキーを押してください（Delete で無効化）", "Click a box and press keys (Delete to disable)")
}
pub fn hotkey_play_pause() -> &'static str { t("再生/一時停止:", "Play/Pause:") }
pub fn hotkey_next() -> &'static str { t("次の曲:", "Next track:") }
pub fn hotkey_previous() -> &'static str { t("前の曲:", "Previous track:") }
pub fn hotkey_none() -> &'static str { t("（なし）", "(None)") }
pub fn hotkey_needs_modifier() -> &'static str {
    t("Ctrl / Alt / Shift / Win のいずれかと組み合わせてください", "Combine with Ctrl / Alt / Shift / Win")
}
pub fn hotkey_duplicate() -> &'static str {
    t("同じホットキーが複数の操作に割り当てられています。", "The same hotkey is assigned to more than one action.")
}
pub fn language_header() -> &'static str { t("言語 / Language", "Language / 言語") }
pub fn preview() -> &'static str { t("プレビュー", "Preview") }
pub fn preview_title() -> &'static str { t("プレビュー表示", "Preview") }
pub fn preview_artist() -> &'static str { "Now Playing Notifier" }
pub fn save() -> &'static str { t("保存", "Save") }
pub fn cancel() -> &'static str { t("キャンセル", "Cancel") }
pub fn version_label(version: &str) -> String {
    tf(format!("バージョン {version}"), format!("Version {version}"))
}

#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<String> {
        let mut v: Vec<String> = [
            tray_show(), tray_settings(), tray_exit(), standby(), no_title(), error(),
            media_error_title(), hotkey_failed_title(), startup_failed_title(), settings_title(),
            position_header(), top_left(), top_right(), bottom_left(), bottom_right(), monitor_header(),
            primary_monitor(), size_header(), width(), height(), margin(), duration_header(),
            opacity_header(), theme_header(), theme_dark(), theme_light(), behavior_header(),
            show_album_art(), suppress_in_fullscreen(), show_in_taskbar(), start_with_windows(), hotkeys_header(),
            hotkey_hint(), hotkey_play_pause(), hotkey_next(), hotkey_previous(), hotkey_none(),
            hotkey_needs_modifier(), hotkey_duplicate(), language_header(), preview(), preview_title(),
            preview_artist(), save(), cancel(), taskbar_header(), taskbar_position(), taskbar_near_tray(),
            taskbar_left_edge(), taskbar_layout(), taskbar_layout_auto(), taskbar_layout_one(),
            taskbar_layout_two(), taskbar_max_width(), taskbar_font_size(), taskbar_offset(),
            taskbar_show_artist(), taskbar_show_album_art(), taskbar_paused(), taskbar_paused_hide(),
            taskbar_paused_indicate(), update_caption(), update_available_message(), update_downloading(),
            update_check_failed(), update_download_failed(), check_for_updates(), check_now(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        v.push(hotkey_failed_message("Ctrl+A"));
        v.push(monitor_item(1, 1920, 1080, true));
        v.push(seconds(5));
        v.push(version_label("1.0.0"));
        v.push(tray_update("1.0.0"));
        v.push(update_available_title("1.0.0"));
        v.push(update_confirm("1.1.0", "1.0.0"));
        v.push(update_open_page("1.1.0", "1.0.0"));
        v.push(update_up_to_date("1.0.0"));
        v
    }

    #[test]
    fn all_strings_are_non_empty_in_both_languages() {
        let _guard = TEST_LOCK.lock().unwrap();
        for lang in [Language::Japanese, Language::English] {
            set_current(lang);
            for s in all() {
                assert!(!s.trim().is_empty(), "empty string for {lang:?}");
            }
        }
    }

    #[test]
    fn switching_language_changes_text() {
        let _guard = TEST_LOCK.lock().unwrap();
        set_current(Language::Japanese);
        assert_eq!(tray_exit(), "終了");
        set_current(Language::English);
        assert_eq!(tray_exit(), "Exit");
    }
}
