//! グローバルホットキーの登録（RegisterHotKey）。押されると WM_HOTKEY が届く。

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};

use crate::hotkey::HotkeyBinding;
use crate::settings::Settings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyAction {
    PlayPause = 1,
    NextTrack = 2,
    PreviousTrack = 3,
}

impl HotkeyAction {
    pub fn from_id(id: usize) -> Option<Self> {
        match id {
            1 => Some(Self::PlayPause),
            2 => Some(Self::NextTrack),
            3 => Some(Self::PreviousTrack),
            _ => None,
        }
    }
}

pub struct Hotkeys {
    hwnd: HWND,
    registered: Vec<HotkeyAction>,
}

impl Hotkeys {
    pub fn new(hwnd: HWND) -> Self {
        Self { hwnd, registered: Vec::new() }
    }

    /// 設定のホットキーを登録し直し、登録に失敗したキーの表示名を返す。
    pub fn register(&mut self, settings: &Settings) -> Vec<String> {
        self.unregister_all();
        let mut failed = Vec::new();
        for (binding, action) in [
            (settings.play_pause_hotkey, HotkeyAction::PlayPause),
            (settings.next_track_hotkey, HotkeyAction::NextTrack),
            (settings.previous_track_hotkey, HotkeyAction::PreviousTrack),
        ] {
            if !binding.is_enabled() {
                continue;
            }
            if self.register_one(binding, action) {
                self.registered.push(action);
            } else {
                failed.push(binding.to_display_string(""));
            }
        }
        failed
    }

    fn register_one(&self, binding: HotkeyBinding, action: HotkeyAction) -> bool {
        unsafe {
            RegisterHotKey(self.hwnd, action as i32, HOT_KEY_MODIFIERS(binding.modifiers), binding.virtual_key).is_ok()
        }
    }

    /// すべてのホットキーを一時的に解除する（設定画面でキー入力を受け取るため）。
    pub fn unregister_all(&mut self) {
        for action in self.registered.drain(..) {
            unsafe {
                let _ = UnregisterHotKey(self.hwnd, action as i32);
            }
        }
    }
}

impl Drop for Hotkeys {
    fn drop(&mut self) {
        self.unregister_all();
    }
}
