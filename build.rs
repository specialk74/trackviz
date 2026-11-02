//! Fetches the Inter font at build time so the .ttf doesn't live in git.
//!
//! The font ends up in OUT_DIR and is embedded with include_bytes!. Sources, in order:
//!   1. fonts/Inter.ttf, if present (for offline builds: put the file there by hand);
//!   2. a copy already downloaded into OUT_DIR by a previous build;
//!   3. a download with `curl` (macOS, Linux, Windows 10+) from a pinned URL.
//! The SHA-256 is always checked, so a different or corrupted file fails the build.
//!
//! DejaVu Sans Mono doesn't need this: it comes from the `dejavu` crate.

use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

const FILE: &str = "Inter.ttf";
/// Inter variable font (opsz, wght) from the Google Fonts repo, pinned to a commit.
const URL: &str = "https://raw.githubusercontent.com/google/fonts/0b58fb370093f9a9f4ff785d94405710b79de67c/ofl/inter/Inter%5Bopsz,wght%5D.ttf";
const SHA256: &str = "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031";

fn sha256_ok(path: &Path) -> bool {
    match std::fs::read(path) {
        Ok(data) => {
            let hex: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
            hex == SHA256
        }
        Err(_) => false,
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=fonts");

    let out = Path::new(&std::env::var("OUT_DIR").unwrap()).join(FILE);
    let local = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("fonts").join(FILE);

    if local.exists() {
        if !sha256_ok(&local) {
            panic!("{} has an unexpected SHA-256 (expected {SHA256}); delete it to download the right one", local.display());
        }
        std::fs::copy(&local, &out).expect("cannot copy fonts/Inter.ttf to OUT_DIR");
        return;
    }
    if sha256_ok(&out) {
        return;
    }

    let tmp = out.with_extension("part");
    let status = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location", "--retry", "3", "-o"])
        .arg(&tmp)
        .arg(URL)
        .status();
    match status {
        Ok(s) if s.success() => {}
        Ok(s) => panic!("downloading {FILE} failed (curl exit {s}).\n{}", offline_hint(&local)),
        Err(e) => panic!("cannot run curl to download {FILE}: {e}\n{}", offline_hint(&local)),
    }
    if !sha256_ok(&tmp) {
        let _ = std::fs::remove_file(&tmp);
        panic!("downloaded {FILE} has an unexpected SHA-256 (expected {SHA256})");
    }
    std::fs::rename(&tmp, &out).expect("cannot move the downloaded font into OUT_DIR");
}

fn offline_hint(local: &Path) -> String {
    format!("Without network access, download it by hand and save it as {}:\n  {URL}", local.display())
}
