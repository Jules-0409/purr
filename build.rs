fn main() {
    println!("cargo:rerun-if-changed=dist/purr.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let ico_path = manifest_dir.join("dist/purr.ico");
        if ico_path.exists() {
            let local_ico = out_dir.join("purr.ico");
            std::fs::copy(&ico_path, &local_ico).expect("copy purr.ico to OUT_DIR");
            let rc_path = out_dir.join("purr.rc");
            let res_path = out_dir.join("purr.res");
            let rc_content = r#"1 ICON "purr.ico"
IDI_ICON1 ICON "purr.ico"

1 VERSIONINFO
FILEVERSION 0,1,0,0
PRODUCTVERSION 0,1,0,0
FILEOS 0x40004
FILETYPE 0x1
{
    BLOCK "StringFileInfo"
    {
        BLOCK "040904B0"
        {
            VALUE "CompanyName", "PURR"
            VALUE "FileDescription", "PURR Native Proxy Client"
            VALUE "FileVersion", "0.1.0"
            VALUE "InternalName", "Purr.exe"
            VALUE "OriginalFilename", "Purr.exe"
            VALUE "ProductName", "PURR"
            VALUE "ProductVersion", "0.1.0"
        }
    }
    BLOCK "VarFileInfo"
    {
        VALUE "Translation", 0x0409, 1200
    }
}
"#;
            std::fs::write(&rc_path, rc_content).unwrap();
            let rc_bin = if std::path::Path::new("/opt/homebrew/opt/llvm/bin/llvm-rc").exists() {
                "/opt/homebrew/opt/llvm/bin/llvm-rc"
            } else {
                "llvm-rc"
            };
            let status = std::process::Command::new(rc_bin)
                .current_dir(&out_dir)
                .env("DYLD_LIBRARY_PATH", "/opt/homebrew/opt/llvm/lib")
                .args(["-fo", "purr.res", "--", "purr.rc"])
                .status()
                .expect("failed to execute llvm-rc");
            assert!(status.success() && res_path.exists(), "llvm-rc failed to compile purr.res");
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
        }
    }
}
