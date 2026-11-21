//! Exercise the headless entry point with a disposable TeX tree and a local package-manager fixture.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use psg_academic::process::{RunOutput, run as run_process};

fn program(path: &Path, script: &str) {
    std::fs::write(path, script).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn run(root: &Path, bin: &Path, command: &str) -> RunOutput {
    let mut child = Command::new(env!("CARGO_BIN_EXE_psg-academic-pdf"));
    child
        .arg(command)
        .env(psg_academic::managed::DIR_VARIABLE, root)
        .env("PATH", bin)
        .env("XDG_DATA_HOME", root.join("data"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY");
    run_process(child, Duration::from_secs(10), None).unwrap()
}

#[test]
fn headless_repair_reinstalls_biber_in_managed_tex() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("TinyTeX/bin/test-platform");
    std::fs::create_dir_all(&bin).unwrap();
    program(&bin.join("pdflatex"), "#!/bin/sh\necho 'pdfTeX fixture'\n");
    program(&bin.join("latexmk"), "#!/bin/sh\necho 'latexmk fixture'\n");
    program(&bin.join("biber"), "#!/bin/sh\necho 'Biber startup failed' >&2\nexit 1\n");
    program(
        &bin.join("tlmgr"),
        r#"#!/bin/sh
printf '%s\n' "$*" > "${0%/*}/tlmgr.log"
[ "$*" = "install --reinstall biber biblatex" ] || exit 2
printf '#!/bin/sh\necho "Biber fixture restored"\n' > "${0%/*}/biber"
"#,
    );
    let before = run(root.path(), &bin, "check-tex");
    assert_eq!(before.status.and_then(|status| status.code()), Some(1));
    assert!(before.stderr.contains("Biber startup failed"));

    let repair = run(root.path(), &bin, "repair-biber");
    assert!(repair.success(), "{}", repair.stderr);
    assert!(repair.stdout.contains("Reinstalled Biber and biblatex."));
    assert_eq!(std::fs::read_to_string(bin.join("tlmgr.log")).unwrap().trim(), "install --reinstall biber biblatex");

    let after = run(root.path(), &bin, "check-tex");
    assert!(after.success(), "{}", after.stderr);
    assert!(after.stdout.contains("Biber fixture restored"));
}

#[test]
fn headless_repair_requires_a_managed_installation() {
    let root = tempfile::tempdir().unwrap();
    let output = run(root.path(), root.path(), "repair-biber");
    assert_eq!(output.status.and_then(|status| status.code()), Some(1));
    assert!(output.stderr.contains("run install-tex first"));
}
