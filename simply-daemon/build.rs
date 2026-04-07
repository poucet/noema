//! Build script — builds the admin UI (Astro) before compiling the daemon.
//!
//! Runs `npm install` (if needed) and `npm run build` in the admin/ directory.
//! The built files end up in admin/dist/ and are served by the daemon at runtime.
//!
//! Set SKIP_ADMIN_BUILD=1 to skip (useful for CI or when iterating on Rust-only changes).

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Always re-run — Astro's own build is fast when nothing changed,
    // and cargo's rerun-if-changed doesn't handle directories recursively.
    println!("cargo:rerun-if-changed=build.rs");

    if std::env::var("SKIP_ADMIN_BUILD").is_ok() {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let admin_dir = manifest_dir.join("admin");

    if !admin_dir.join("package.json").exists() {
        println!("cargo:warning=admin/package.json not found, skipping admin UI build");
        return;
    }

    // npm install if needed
    if !admin_dir.join("node_modules").exists() {
        let status = Command::new("npm")
            .args(["install"])
            .current_dir(&admin_dir)
            .status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => panic!("npm install failed with {s}"),
            Err(e) => panic!("npm not found ({e}) — install Node.js to build the admin UI"),
        }
    }

    // npm run build — fail the cargo build if this fails
    let status = Command::new("npm")
        .args(["run", "build"])
        .current_dir(&admin_dir)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => panic!("admin UI build failed with {s}"),
        Err(e) => panic!("admin UI build failed: {e}"),
    }
}
