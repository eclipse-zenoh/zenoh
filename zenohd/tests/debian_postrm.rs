//
// Copyright (c) 2026 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//
#![cfg(unix)]

use std::process::{Command, Output};

fn postrm(args: &[&str]) -> Output {
    // Never run the real rm/userdel, even when testing as root. An unusable PATH
    // also prevents any unmocked external command from reaching the host.
    let script = format!(
        r#"
exec 3>&1
record() {{
    printf '%s' "$1" >&3
    shift
    printf ' <%s>' "$@" >&3
    printf '\n' >&3
}}
rm() {{ record rm "$@"; }}
userdel() {{ record userdel "$@"; }}
{}
"#,
        include_str!("../.deb/postrm")
    );

    Command::new("/bin/sh")
        .env_clear()
        .env("PATH", "/dev/null")
        .args(["-c", &script, "postrm"])
        .args(args)
        .output()
        .expect("failed to run the Debian postrm script with mocked commands")
}

#[test]
fn debian_postrm_upgrade_preserves_configuration_state_and_account() {
    // Regression for https://github.com/eclipse-zenoh/zenoh/issues/2778:
    // dpkg invokes the old package's postrm with upgrade and the new version.
    let output = postrm(&["upgrade", "1.10.1"]);
    assert!(
        output.status.success(),
        "postrm upgrade failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "an upgrade must not remove configuration, state, or the service account; recorded commands:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn debian_postrm_purge_removes_configuration_state_and_account() {
    let output = postrm(&["purge"]);
    assert!(
        output.status.success(),
        "postrm purge failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Also verifies that userdel is recorded despite the script's redirections.
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "userdel <zenohd>\nrm <-rf> </etc/zenohd> </var/zenohd> </etc/zenohd.dpkg-backup> </var/zenohd.dpkg-backup>\n"
    );
}

#[test]
fn debian_postrm_rejects_unknown_action_without_removing_anything() {
    let output = postrm(&["unknown-action"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("postrm called with unknown argument `unknown-action'"));
    assert!(
        output.stdout.is_empty(),
        "an unknown action must not run destructive commands: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
