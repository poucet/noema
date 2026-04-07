//! Build script — builds the admin UI (Astro) before compiling the daemon.
//!
//! Runs `npm install` (if needed) and `npm run build` in the admin/ directory.
//! The built files end up in admin/dist/ and are served by the daemon at runtime.
//!
//! Set SKIP_ADMIN_BUILD=1 to skip (useful for CI or when iterating on Rust-only changes).

use std::path::Path;
use std::process::Command;

fn main() {
    let admin_dir = Path::new("admin");

    // Only rebuild when admin sources change
    println!("cargo:rerun-if-changed=admin/src/");
    println!("cargo:rerun-if-changed=admin/package.json");
    println!("cargo:rerun-if-changed=admin/astro.config.mjs");

    if std::env::var("SKIP_ADMIN_BUILD").is_ok() {
        println!("cargo:warning=SKIP_ADMIN_BUILD set, skipping admin UI build");
        return;
    }

    if !admin_dir.join("package.json").exists() {
        println!("cargo:warning=admin/package.json not found, skipping admin UI build");
        return;
    }

    // npm install if needed
    if !admin_dir.join("node_modules").exists() {
        let status = Command::new("npm")
            .args(["install", "--silent"])
            .current_dir(admin_dir)
            .status();

        match status {
            Ok(s) if s.success() => {}
            Ok(s) => println!("cargo:warning=npm install failed with {s}"),
            Err(e) => {
                println!("cargo:warning=npm not found ({e}), skipping admin UI build");
                return;
            }
        }
    }

    // npm run build
    let status = Command::new("npm")
        .args(["run", "build", "--silent"])
        .current_dir(admin_dir)
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => println!("cargo:warning=npm run build failed with {s}"),
        Err(e) => println!("cargo:warning=npm build failed: {e}"),
    }
}
