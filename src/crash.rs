//! 異常終了の原因を調べるための記録。
//! %LOCALAPPDATA%\NowPlayingNotifier\crash.log に、パニックと致命的な例外（アクセス違反など）を書き出す。

use std::io::Write;
use std::path::PathBuf;

use windows::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_STACK_OVERFLOW, NTSTATUS};
use windows::Win32::System::Diagnostics::Debug::{AddVectoredExceptionHandler, EXCEPTION_POINTERS};
use windows::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
};

const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
/// ユーザーコールバック中の未処理例外（user32 が元の例外を包み直したもの）
const STATUS_FATAL_USER_CALLBACK_EXCEPTION: NTSTATUS = NTSTATUS(0xC000041Du32 as i32);

fn log_path() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("NowPlayingNotifier");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("crash.log"))
}

fn write_log(text: &str) {
    let Some(path) = log_path() else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let _ = writeln!(f, "[{secs}] v{} {text}", crate::APP_VERSION);
    }
}

/// アドレスが属するモジュール名とモジュール内のオフセット。
fn describe_address(addr: usize) -> String {
    unsafe {
        let mut module = Default::default();
        let flags = GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;
        if GetModuleHandleExW(flags, windows::core::PCWSTR(addr as *const u16), &mut module).is_ok() {
            let mut buf = [0u16; 260];
            let len = GetModuleFileNameW(module, &mut buf) as usize;
            let name = String::from_utf16_lossy(&buf[..len]);
            let name = name.rsplit('\\').next().unwrap_or(&name).to_string();
            return format!("{name}+0x{:X}", addr - module.0 as usize);
        }
    }
    format!("0x{addr:X}")
}

unsafe extern "system" fn on_exception(info: *mut EXCEPTION_POINTERS) -> i32 {
    let Some(record) = info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) else {
        return EXCEPTION_CONTINUE_SEARCH;
    };
    let code = record.ExceptionCode;
    if code == EXCEPTION_ACCESS_VIOLATION
        || code == EXCEPTION_ILLEGAL_INSTRUCTION
        || code == EXCEPTION_STACK_OVERFLOW
        || code == STATUS_FATAL_USER_CALLBACK_EXCEPTION
    {
        let addr = record.ExceptionAddress as usize;
        let mut text = format!("exception 0x{:08X} at {}", code.0 as u32, describe_address(addr));
        if code == EXCEPTION_ACCESS_VIOLATION && record.NumberParameters >= 2 {
            let kind = match record.ExceptionInformation[0] { 0 => "read", 1 => "write", 8 => "execute", _ => "access" };
            text.push_str(&format!(" ({kind} 0x{:X})", record.ExceptionInformation[1]));
        }
        write_log(&text);
    }
    EXCEPTION_CONTINUE_SEARCH
}

pub fn install() {
    std::panic::set_hook(Box::new(|info| {
        write_log(&format!("panic: {info}"));
    }));
    unsafe {
        AddVectoredExceptionHandler(1, Some(on_exception));
    }
}
