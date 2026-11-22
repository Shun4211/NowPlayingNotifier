//! グローバルホットキーの組み合わせ。

use serde::{Deserialize, Serialize};

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;
pub const MODIFIER_MASK: u32 = MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN;

pub const VK_SPACE: u32 = 0x20;
pub const VK_LEFT: u32 = 0x25;
pub const VK_RIGHT: u32 = 0x27;

/// 修飾キーとキー（仮想キーコード）の組み合わせ。`virtual_key` が 0 のときは無効。
/// JSON の形式は C# 版（`{"Modifiers":3,"VirtualKey":32}`）と同じ。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct HotkeyBinding {
    pub modifiers: u32,
    pub virtual_key: u32,
}

impl HotkeyBinding {
    pub const NONE: Self = Self::new(0, 0);
    pub const DEFAULT_PLAY_PAUSE: Self = Self::new(MOD_CONTROL | MOD_ALT, VK_SPACE);
    pub const DEFAULT_NEXT_TRACK: Self = Self::new(MOD_CONTROL | MOD_ALT, VK_RIGHT);
    pub const DEFAULT_PREVIOUS_TRACK: Self = Self::new(MOD_CONTROL | MOD_ALT, VK_LEFT);

    pub const fn new(modifiers: u32, virtual_key: u32) -> Self {
        Self { modifiers, virtual_key }
    }

    pub fn is_enabled(&self) -> bool {
        self.virtual_key != 0
    }

    /// 修飾キーなしでも登録してよいキー（ファンクションキー・メディアキー）か。
    pub fn allows_no_modifier(vk: u32) -> bool {
        (0x70..=0x87).contains(&vk) || (0xB0..=0xB3).contains(&vk)
    }

    /// 登録可能な値か（無効化された状態も有効な値として扱う）。
    pub fn is_valid(&self) -> bool {
        if !self.is_enabled() {
            return self.modifiers == 0;
        }
        if self.virtual_key > 0xFE || self.modifiers & !MODIFIER_MASK != 0 {
            return false;
        }
        if self.modifiers & MODIFIER_MASK == 0 {
            return Self::allows_no_modifier(self.virtual_key);
        }
        true
    }

    pub fn to_display_string(self, none_text: &str) -> String {
        if !self.is_enabled() {
            return none_text.to_string();
        }
        let mut parts: Vec<String> = Vec::new();
        for (flag, name) in [(MOD_CONTROL, "Ctrl"), (MOD_ALT, "Alt"), (MOD_SHIFT, "Shift"), (MOD_WIN, "Win")] {
            if self.modifiers & flag != 0 {
                parts.push(name.to_string());
            }
        }
        parts.push(key_name(self.virtual_key));
        parts.join("+")
    }
}

/// 仮想キーコードの表示名。
pub fn key_name(vk: u32) -> String {
    let fixed = match vk {
        0x25 => "←",
        0x27 => "→",
        0x26 => "↑",
        0x28 => "↓",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x2D => "Insert",
        0x2E => "Delete",
        0x08 => "Back",
        0x09 => "Tab",
        0x0D => "Enter",
        0x1B => "Escape",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x2C => "PrintScreen",
        0x90 => "NumLock",
        0x91 => "ScrollLock",
        0x6A => "Multiply",
        0x6B => "Add",
        0x6C => "Separator",
        0x6D => "Subtract",
        0x6E => "Decimal",
        0x6F => "Divide",
        0xAD => "VolumeMute",
        0xAE => "VolumeDown",
        0xAF => "VolumeUp",
        0xB0 => "MediaNextTrack",
        0xB1 => "MediaPreviousTrack",
        0xB2 => "MediaStop",
        0xB3 => "MediaPlayPause",
        _ => "",
    };
    if !fixed.is_empty() {
        return fixed.to_string();
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from(vk as u8).to_string(),
        0x60..=0x69 => format!("NumPad{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => oem_key_name(vk).unwrap_or_else(|| format!("0x{vk:02X}")),
    }
}

/// 記号キーは現在のキーボード配列での文字を表示する（例: 0xBB -> "+" や ";"）。
#[cfg(windows)]
fn oem_key_name(vk: u32) -> Option<String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{MapVirtualKeyW, MAPVK_VK_TO_CHAR};
    let ch = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_CHAR) } & 0x7FFF_FFFF;
    char::from_u32(ch).filter(|c| !c.is_control() && *c != '\0').map(|c| c.to_string())
}

#[cfg(not(windows))]
fn oem_key_name(_vk: u32) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_ctrl_alt_with_space_and_arrows() {
        assert_eq!(HotkeyBinding::DEFAULT_PLAY_PAUSE.to_display_string("-"), "Ctrl+Alt+Space");
        assert_eq!(HotkeyBinding::DEFAULT_NEXT_TRACK.to_display_string("-"), "Ctrl+Alt+→");
        assert_eq!(HotkeyBinding::DEFAULT_PREVIOUS_TRACK.to_display_string("-"), "Ctrl+Alt+←");
    }

    #[test]
    fn none_displays_placeholder_and_is_valid() {
        assert_eq!(HotkeyBinding::NONE.to_display_string("(none)"), "(none)");
        assert!(!HotkeyBinding::NONE.is_enabled());
        assert!(HotkeyBinding::NONE.is_valid());
    }

    #[test]
    fn modifiers_and_letter_display() {
        let b = HotkeyBinding::new(MOD_CONTROL | MOD_SHIFT, 0x50);
        assert_eq!(b.to_display_string("-"), "Ctrl+Shift+P");
        assert!(b.is_valid());
    }

    #[test]
    fn digit_keys_display_as_numbers() {
        assert_eq!(HotkeyBinding::new(MOD_ALT, 0x35).to_display_string("-"), "Alt+5");
    }

    #[test]
    fn function_keys_display_names() {
        assert_eq!(HotkeyBinding::new(0, 0x78).to_display_string("-"), "F9");
        assert_eq!(HotkeyBinding::new(MOD_WIN, 0x87).to_display_string("-"), "Win+F24");
    }

    #[test]
    fn without_modifier_only_function_and_media_keys_are_valid() {
        for (vk, expected) in [(0x41, false), (0x20, false), (0x78, true), (0xB3, true), (0xB0, true)] {
            assert_eq!(HotkeyBinding::new(0, vk).is_valid(), expected, "vk={vk:#x}");
        }
    }

    #[test]
    fn unknown_modifier_bits_are_invalid() {
        assert!(!HotkeyBinding::new(0x4000 | MOD_CONTROL, 0x41).is_valid());
    }

    #[test]
    fn disabled_key_with_modifiers_is_invalid() {
        assert!(!HotkeyBinding::new(MOD_CONTROL, 0).is_valid());
    }

    #[test]
    fn json_matches_csharp_format() {
        let json = serde_json::to_string(&HotkeyBinding::DEFAULT_PLAY_PAUSE).unwrap();
        assert_eq!(json, r#"{"Modifiers":3,"VirtualKey":32}"#);
    }
}
