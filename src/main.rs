//! NowPlayingNotifier: 再生中の曲が変わったときにポップアップで知らせる常駐アプリ。
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod crash;
mod debug_log;
mod hotkey;
mod hotkeys;
mod imaging;
mod main_window;
mod media;
mod popup;
mod position;
mod settings;
mod settings_window;
mod strings;
mod system;
mod taskbar;
mod track;
mod tray;
mod update;
mod win;

use windows::core::{w, HSTRING};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Controls::{InitCommonControlsEx, ICC_BAR_CLASSES, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR};

pub const APP_VERSION: &str = env!("APP_VERSION");

fn main() {
    crash::install();

    // 二重起動時は、起動済みのインスタンスにウィンドウ表示を頼んで終了する
    let Some(mut instance) = system::SingleInstance::acquire() else { return };

    unsafe {
        // マニフェストでも宣言しているが、マニフェストなしでビルドした場合に備えて
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // UI スレッドは STA（ShellExecute や IME などは STA を前提にしている）。
        // WinRT の待ち合わせ（.get()）はワーカースレッドで MTA にして行う。
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES | ICC_STANDARD_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);
    }

    if let Err(e) = app::start() {
        unsafe {
            MessageBoxW(None, &HSTRING::from(e.message()), w!("NowPlayingNotifier"), MB_ICONERROR);
        }
        return;
    }
    instance.listen();
    app::run_message_loop();

    // 更新のために終了した場合は、二重起動防止の Mutex を手放してからインストーラーを実行する
    drop(instance);
    update::launch_pending_installer();
}
