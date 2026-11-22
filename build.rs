// アイコン・マニフェスト・バージョン情報を exe に埋め込む。
// リソースコンパイラ (rc.exe) は MSVC ツールチェーンにしか無いので、それ以外ではスキップする。
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let icon = manifest_dir.join("res/appicon.ico");
    let manifest = manifest_dir.join("res/app.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-env-changed=APP_VERSION");

    // リリース時は APP_VERSION（タグから）で上書きする
    let version = std::env::var("APP_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=APP_VERSION={version}");

    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        println!("cargo:warning=MSVC 以外のツールチェーンのため、アイコンとマニフェストを埋め込みません");
        return;
    }

    // "1.2.3" -> 1,2,3,0
    let mut parts: Vec<u16> = version.split(['.', '-', '+']).filter_map(|p| p.parse().ok()).take(4).collect();
    parts.resize(4, 0);
    let numeric = parts.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(",");

    let rc_path = |p: &PathBuf| p.display().to_string().replace('\\', "/");
    let rc = format!(
        r#"#pragma code_page(65001)
#include <winres.h>

1 ICON "{icon}"
CREATEPROCESS_MANIFEST_RESOURCE_ID RT_MANIFEST "{manifest}"

VS_VERSION_INFO VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "000004b0"
        BEGIN
            VALUE "CompanyName", "Shun4211"
            VALUE "FileDescription", "NowPlayingNotifier"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "NowPlayingNotifier"
            VALUE "OriginalFilename", "NowPlayingNotifier.exe"
            VALUE "ProductName", "NowPlayingNotifier"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0000, 0x04b0
    END
END
"#,
        icon = rc_path(&icon),
        manifest = rc_path(&manifest),
    );

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app.rc");
    std::fs::write(&out, rc).unwrap();
    embed_resource::compile(&out, embed_resource::NONE);
}
