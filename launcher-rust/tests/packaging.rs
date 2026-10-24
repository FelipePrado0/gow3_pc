#![allow(clippy::unwrap_used, clippy::expect_used)]

use spartanport_launcher::core::legacy::{LEGACY_EXE, LEGACY_PLAY_EXE};

fn read(relative: &str) -> String {
    let path = format!("{}/../{relative}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

#[test]
fn the_windows_package_ships_the_rust_launcher() {
    let package = read("packaging/windows/package.sh");
    for needed in [
        "launcher-rust/target/release/spartanport-launcher.exe",
        "legacy::LEGACY_EXE",
        "legacy::LEGACY_PLAY_EXE",
        LEGACY_EXE,
        LEGACY_PLAY_EXE,
        "scripts/controller_diagrams",
        "-iname '*.py'",
        "-iname _internal",
    ] {
        assert!(package.contains(needed), "package.sh lacks {needed}");
    }
    assert!(
        package.contains("cd launcher-rust && cargo build --release --locked"),
        "package.sh must tell to build from inside launcher-rust"
    );
    for banned in [
        "PyInstaller",
        "pyenv",
        "run.py",
        "cp -r scripts patches",
        "--manifest-path launcher-rust/Cargo.toml",
    ] {
        assert!(!package.contains(banned), "package.sh still has {banned}");
    }

    let smoke = read("packaging/windows/smoke-test.ps1");
    for needed in ["--version", "--voice-swap"] {
        assert!(smoke.contains(needed), "smoke-test.ps1 lacks {needed}");
    }
    assert!(
        !smoke.contains("--script"),
        "smoke-test.ps1 still has --script"
    );

    let workflow = read(".github/workflows/windows-zip.yml");
    for needed in [
        "dtolnay/rust-toolchain@stable",
        "cargo build --release --locked",
        "working-directory: launcher-rust",
    ] {
        assert!(workflow.contains(needed), "windows-zip.yml lacks {needed}");
    }
}

#[test]
fn the_appimage_ships_the_rust_launcher() {
    let build = read("packaging/linux/build.sh");
    for needed in [
        "launcher-rust/target/release/spartanport-launcher",
        "scripts/controller_diagrams",
        "(cd launcher-rust && \"${CARGO:-cargo}\" build --release --locked)",
        "-name '*.py'",
        "_internal",
    ] {
        assert!(build.contains(needed), "build.sh lacks {needed}");
    }
    for banned in [
        "PyInstaller",
        "run.py",
        "cp -r scripts patches",
        "--manifest-path launcher-rust/Cargo.toml",
    ] {
        assert!(!build.contains(banned), "build.sh still has {banned}");
    }

    let smoke = read("packaging/linux/smoke-test.sh");
    for needed in ["--version", "panicked"] {
        assert!(smoke.contains(needed), "smoke-test.sh lacks {needed}");
    }
    for banned in ["Traceback", "ImportError"] {
        assert!(!smoke.contains(banned), "smoke-test.sh still has {banned}");
    }

    let deps = read("packaging/linux/ci-deps.sh");
    for banned in ["PyInstaller", "pyinstaller", "venv", "python3-tk"] {
        assert!(!deps.contains(banned), "ci-deps.sh still has {banned}");
    }

    let workflow = read(".github/workflows/linux-appimage.yml");
    assert!(
        workflow.contains("dtolnay/rust-toolchain@stable"),
        "linux-appimage.yml lacks the Rust toolchain"
    );
    assert!(
        !workflow.contains("venv"),
        "linux-appimage.yml still has the venv"
    );
}
