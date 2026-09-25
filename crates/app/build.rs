// SPDX-License-Identifier: AGPL-3.0-or-later

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    // Tauri embeds its Windows manifest (Common Controls v6) only into the
    // binary, so test executables linking Tauri crash at startup with
    // STATUS_ENTRYPOINT_NOT_FOUND. Embed the manifest through the linker
    // instead, which applies to every target of this crate (bins and tests).
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))?;

    let target_os = std::env::var("CARGO_CFG_TARGET_OS")?;
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV")?;
    if target_os == "windows" && target_env == "msvc" {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR")?)
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    Ok(())
}
