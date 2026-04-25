#[cfg(target_os = "windows")]
fn main() {
    use std::{env, fs, path::PathBuf};

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=resourses/app.ico");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    let icon_path = manifest_dir.join("resourses").join("app.ico");
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_owned());
    let (major, minor, patch, build) = parse_version_parts(&version);

    let rc_source = format!(
        r#"#pragma code_page(65001)
#include <winver.h>

1 ICON "{}"

VS_VERSION_INFO VERSIONINFO
 FILEVERSION {},{},{},{}
 PRODUCTVERSION {},{},{},{}
 FILEFLAGSMASK 0x3fL
#ifdef _DEBUG
 FILEFLAGS 0x1L
#else
 FILEFLAGS 0x0L
#endif
 FILEOS 0x40004L
 FILETYPE 0x1L
 FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "000004b0"
        BEGIN
            VALUE "CompanyName", "CzXieDdan\0"
            VALUE "FileDescription", "RGMR App\0"
            VALUE "FileVersion", "{}\0"
            VALUE "InternalName", "RGMR\0"
            VALUE "OriginalFilename", "rgmr.exe\0"
            VALUE "ProductName", "RGMR\0"
            VALUE "ProductVersion", "{}\0"
            VALUE "LegalCopyright", "Copyright © 2026  CzXieDdan\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0, 1200
    END
END
"#,
        escape_rc_path(&icon_path),
        major,
        minor,
        patch,
        build,
        major,
        minor,
        patch,
        build,
        version,
        version,
    );

    let rc_path = out_dir.join("rgmr-resource.rc");
    fs::write(&rc_path, rc_source).expect("write windows resource script");
    let _ = embed_resource::compile(rc_path, embed_resource::NONE);
}

#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn parse_version_parts(version: &str) -> (u16, u16, u16, u16) {
    let mut parts = version.split('.');
    let major = parse_version_part(parts.next());
    let minor = parse_version_part(parts.next());
    let patch = parse_version_part(parts.next());
    let build = parse_version_part(parts.next());
    (major, minor, patch, build)
}

#[cfg(target_os = "windows")]
fn parse_version_part(part: Option<&str>) -> u16 {
    part.and_then(|value| {
        let digits: String = value.chars().take_while(|ch| ch.is_ascii_digit()).collect();
        if digits.is_empty() {
            None
        } else {
            digits.parse::<u16>().ok()
        }
    })
    .unwrap_or(0)
}

#[cfg(target_os = "windows")]
fn escape_rc_path(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "\\\\")
}
