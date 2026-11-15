//! Windows only: embed the app icon and version info (VERSIONINFO) into `psg-academic-pdf.exe`,
//! so Explorer, the taskbar, the Start menu shortcut, Alt-Tab and the `.tex`/`.docx` "Open with"
//! entries (which the installer points at `psg-academic-pdf.exe,0`) show PSG Academic PDF's icon
//! and publisher instead of a generic program.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `PDFCRAFT_REQUIRE_WINRES=1` turns it
//! into an error (packaging/windows/package.ps1 sets it for release builds).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon-academic/psg-academic-pdf.ico");
    println!("cargo:rerun-if-env-changed=PDFCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon-academic/psg-academic-pdf.ico")
        .set("ProductName", "PSG Academic PDF")
        .set("FileDescription", "PSG Academic PDF")
        .set("CompanyName", "Panorama Scholarly Group")
        .set("LegalCopyright", "Copyright (c) Panorama Scholarly Group and the PSG PDF contributors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "psg-academic-pdf.exe")
        .set("InternalName", "psg-academic-pdf");
    if let Err(e) = res.compile() {
        if std::env::var_os("PDFCRAFT_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources failed: {e}");
            return;
        }
        println!("cargo:warning=psg-academic-pdf.exe built without icon/version resources: {e}");
    }
}
