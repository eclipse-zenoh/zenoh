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
//! Runs the Debian maintainer scripts in the order dpkg runs them during an
//! upgrade, against a temporary directory standing in for `/`.
//! Regression tests for https://github.com/eclipse-zenoh/zenoh/issues/2778.
#![cfg(target_os = "linux")]

use std::{fs, path::PathBuf, process::Command};

const PREINST: &str = include_str!("../.deb/preinst");
const POSTINST: &str = include_str!("../.deb/postinst");
const POSTRM: &str = include_str!("../.deb/postrm");

/// The destructive branch of the postrm shipped up to zenohd 1.10.1.
const LEGACY_POSTRM: &str = r#"
case "$1" in
    purge|remove|upgrade|failed-upgrade|abort-install|abort-upgrade|disappear)
    userdel zenohd > /dev/null 2>&1 || true
    rm -rf /etc/zenohd /var/zenohd
    ;;
esac
"#;

/// A temporary root with an installed zenohd whose configuration and state
/// the user has changed.
struct Root(PathBuf);

impl Root {
    fn installed(name: &str) -> Self {
        let root =
            Root(std::env::temp_dir().join(format!("zenohd-debian-{name}-{}", std::process::id())));
        let _ = fs::remove_dir_all(&root.0);
        fs::create_dir_all(root.0.join("etc/zenohd")).unwrap();
        fs::create_dir_all(root.0.join("var/zenohd/db")).unwrap();
        fs::write(root.0.join("etc/zenohd/zenohd.json5"), "edited").unwrap();
        fs::write(root.0.join("etc/zenohd/custom.json5"), "custom").unwrap();
        fs::write(root.0.join("var/zenohd/db/state"), "state").unwrap();
        fs::write(root.0.join("account"), "").unwrap();
        root
    }

    /// Runs a maintainer script with its paths redirected into the root. The
    /// account and service commands are stubs: the account is the file
    /// `account`, and chown appends its arguments to the file `chown`.
    fn run(&self, script: &str, args: &[&str]) {
        let script = format!(
            r#"
id() {{ [ -e "$ROOT/account" ]; }}
useradd() {{ : > "$ROOT/account"; }}
userdel() {{ rm -f "$ROOT/account"; }}
chown() {{ echo "$*" >> "$ROOT/chown"; }}
systemctl() {{ :; }}
{}
"#,
            script
                .replace("/etc/zenohd", "${ROOT:?}/etc/zenohd")
                .replace("/var/zenohd", "${ROOT:?}/var/zenohd")
        );
        let status = Command::new("/bin/sh")
            .env("ROOT", &self.0)
            .args(["-c", &script, "maintainer-script"])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "maintainer script {args:?} failed");
    }

    /// dpkg unpacks a conffile next to the live one, recreating its directory.
    fn unpack(&self) {
        fs::create_dir_all(self.0.join("etc/zenohd")).unwrap();
        fs::write(self.0.join("etc/zenohd/zenohd.json5.dpkg-new"), "shipped").unwrap();
    }

    /// dpkg --configure --force-confold, then postinst.
    fn configure(&self) {
        let etc = self.0.join("etc/zenohd");
        if etc.join("zenohd.json5.dpkg-new").exists() {
            let target = if etc.join("zenohd.json5").exists() {
                "zenohd.json5.dpkg-dist"
            } else {
                "zenohd.json5"
            };
            fs::rename(etc.join("zenohd.json5.dpkg-new"), etc.join(target)).unwrap();
        }
        self.run(POSTINST, &["configure", "1.10.1"]);
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.0.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn assert_kept(&self) {
        assert_eq!(self.read("etc/zenohd/zenohd.json5"), "edited");
        assert_eq!(self.read("etc/zenohd/custom.json5"), "custom");
        assert_eq!(self.read("var/zenohd/db/state"), "state");
        assert!(self.0.join("account").exists(), "zenohd account missing");
        assert!(!self.0.join("etc/zenohd.dpkg-backup").exists());
        assert!(!self.0.join("var/zenohd.dpkg-backup").exists());
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn debian_upgrade_from_legacy_postrm_keeps_configuration_and_state() {
    let root = Root::installed("legacy");
    root.run(PREINST, &["upgrade", "1.10.1"]);
    root.unpack();
    root.run(LEGACY_POSTRM, &["upgrade", "1.11.0"]);
    root.configure();
    root.assert_kept();
    // The legacy postrm deleted the account, so its files go to the new one.
    assert_eq!(
        root.read("chown"),
        format!("-R zenohd:zenohd {}/var/zenohd\n", root.0.display())
    );
}

#[test]
fn debian_upgrade_from_legacy_postrm_retried_after_unpack_keeps_configuration_and_state() {
    let root = Root::installed("legacy-retried");
    root.run(PREINST, &["upgrade", "1.10.1"]);
    root.unpack();
    root.run(LEGACY_POSTRM, &["upgrade", "1.11.0"]);
    // The upgrade stops before postinst; dpkg unpacks the new package again.
    root.run(PREINST, &["upgrade", "1.11.0"]);
    root.unpack();
    root.run(POSTRM, &["upgrade", "1.11.0"]);
    root.configure();
    root.assert_kept();
}

#[test]
fn debian_upgrade_between_fixed_versions_keeps_configuration_and_state() {
    let root = Root::installed("fixed");
    root.run(PREINST, &["upgrade", "1.11.0"]);
    root.unpack();
    root.run(POSTRM, &["upgrade", "1.11.1"]);
    root.configure();
    root.assert_kept();
    assert_eq!(root.read("etc/zenohd/zenohd.json5.dpkg-dist"), "shipped");
}
