// SPDX-License-Identifier: AGPL-3.0-or-later

//! Compiles `shim/*.c`, our guarded wrappers for MuPDF calls that mupdf-sys
//! does not wrap (ADR 0004). mupdf-sys publishes no include path, so we ask
//! Cargo where its sources are.

use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

fn mupdf_sys_dir() -> Result<PathBuf, Box<dyn Error>> {
    let cargo = env::var("CARGO")?;
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("Cargo.toml");
    let run = |offline: bool| {
        let mut cmd = Command::new(&cargo);
        cmd.args(["metadata", "--format-version", "1", "--manifest-path"])
            .arg(&manifest);
        if offline {
            cmd.arg("--offline");
        }
        cmd.output()
    };
    let out = match run(true) {
        Ok(o) if o.status.success() => o,
        _ => run(false)?,
    };
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let packages = meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?;
    let pkg = packages
        .iter()
        .find(|p| p["name"] == "mupdf-sys")
        .ok_or("cargo metadata: mupdf-sys not found")?;
    let path = PathBuf::from(pkg["manifest_path"].as_str().ok_or("no manifest_path")?);
    Ok(path.parent().ok_or("bad manifest_path")?.to_path_buf())
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed=shim");
    let sys = mupdf_sys_dir()?;
    cc::Build::new()
        .file("shim/layers.c")
        .include(sys.join("mupdf/include"))
        .include(sys.join("wrapper"))
        .warnings(false)
        .compile("ofp_shim");
    Ok(())
}
