//! 開発用のログ（デバッグビルドのみ）。%LOCALAPPDATA%\NowPlayingNotifier\debug.log に追記する。
//! リリースビルドでは何もしない。

#[cfg(debug_assertions)]
pub fn write(message: std::fmt::Arguments) {
    use std::io::Write;
    use std::sync::OnceLock;
    use std::time::Instant;

    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    let Some(dir) = std::env::var_os("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("NowPlayingNotifier")) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("debug.log")) {
        let _ = writeln!(f, "[{:>9.3}] {}", start.elapsed().as_secs_f64(), message);
    }
}

#[cfg(not(debug_assertions))]
pub fn write(_: std::fmt::Arguments) {}

macro_rules! debug_log {
    ($($arg:tt)*) => { $crate::debug_log::write(format_args!($($arg)*)) };
}
pub(crate) use debug_log;
