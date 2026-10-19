#!/bin/sh
#
# Copyright (c) 2026 ZettaScale Technology
#
# This program and the accompanying materials are made available under the
# terms of the Eclipse Public License 2.0 which is available at
# http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
# which is available at https://www.apache.org/licenses/LICENSE-2.0.
#
# SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
#
# Contributors:
#   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
#
set -eu

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

# These tests modify real system paths and accounts, never the host's.
[ -f /.dockerenv ] || [ -f /run/.containerenv ] ||
    fail "run this script in a disposable Docker or Podman container"
[ "$(id -u)" = 0 ] || fail "the container must run as root"

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# There is no systemd running in the container. Only service management is
# stubbed; dpkg, conffile handling, filesystem operations and accounts are real.
mkdir "$work/bin"
cat > "$work/bin/systemctl" <<'EOF'
#!/bin/sh
set -eu
case "$*" in
    daemon-reload|"disable zenohd") ;;
    *) echo "Unexpected systemctl call: $*" >&2; exit 1 ;;
esac
EOF
chmod 755 "$work/bin/systemctl"
export PATH="$work/bin:$PATH"

build_package() {
    version=$1
    package="$work/$version"
    mkdir -p "$package/DEBIAN" "$package/etc/zenohd" "$package/lib/systemd/system"
    cat > "$package/DEBIAN/control" <<EOF
Package: zenohd
Version: $version
Architecture: all
Maintainer: Zenoh developers <zenoh-dev@eclipse.org>
Description: zenohd maintainer-script regression test fixture
EOF
    echo /etc/zenohd/zenohd.json5 > "$package/DEBIAN/conffiles"
    cp "$repo/zenohd/.service/zenohd.json5" "$package/etc/zenohd/"
    # Change the shipped conffile so upgrades exercise dpkg's conflict handling.
    echo "// Packaging test version $version" >> "$package/etc/zenohd/zenohd.json5"
    cp "$repo/zenohd/.service/zenohd.service" "$package/lib/systemd/system/"
    cp "$repo/zenohd/.deb/"* "$package/DEBIAN/"
    if [ "$version" = 1.10.1 ]; then
        rm -f "$package/DEBIAN/preinst"
        # Destructive postrm shipped through 1.10.1 (regression #2778).
        cat > "$package/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
case "$1" in
    purge|remove|upgrade|failed-upgrade|abort-install|abort-upgrade|disappear)
        userdel zenohd > /dev/null 2>&1 || true
        rm -rf /etc/zenohd /var/zenohd
        ;;
    *) exit 1 ;;
esac
EOF
    fi
    chmod 755 "$package/DEBIAN/"*inst "$package/DEBIAN/postrm"
    dpkg-deb --build "$package" "$work/zenohd_$version.deb"
}

install_package() {
    dpkg --force-confold --install "$work/zenohd_$1.deb"
}

seed_data() {
    printf 'edited\n' > /etc/zenohd/zenohd.json5
    printf 'custom\n' > /etc/zenohd/custom.json5
    mkdir -p /var/zenohd/db
    printf 'state\n' > /var/zenohd/db/state
    chown -R zenohd:zenohd /var/zenohd
}

assert_no_snapshots() {
    [ ! -e /etc/zenohd.dpkg-backup ] || fail "configuration snapshot left behind"
    [ ! -e /var/zenohd.dpkg-backup ] || fail "state snapshot left behind"
}

assert_data() {
    [ "$(cat /etc/zenohd/zenohd.json5)" = edited ] || fail "edited configuration lost"
    [ "$(cat /etc/zenohd/custom.json5)" = custom ] || fail "custom configuration lost"
    [ "$(cat /var/zenohd/db/state)" = state ] || fail "persistent state lost"
    id zenohd > /dev/null || fail "service account missing"
    [ "$(stat -c %U:%G /var/zenohd/db/state)" = zenohd:zenohd ] ||
        fail "state ownership not restored"
}

assert_purged() {
    [ ! -e /etc/zenohd ] || fail "configuration survived purge"
    [ ! -e /var/zenohd ] || fail "state survived purge"
    if id zenohd > /dev/null 2>&1; then
        fail "service account survived purge"
    fi
    assert_no_snapshots
}

for version in 1.10.1 1.11.0 1.11.1; do
    build_package "$version"
done

echo "=== Fresh installation and upgrade from legacy postrm ==="
install_package 1.10.1
seed_data
old_uid=$(id -u zenohd)
# Unpack separately to verify that the legacy postrm really deletes the data.
dpkg --unpack "$work/zenohd_1.11.0.deb"
[ ! -e /etc/zenohd ] && [ ! -e /var/zenohd ] ||
    fail "legacy postrm did not reproduce the destructive upgrade"
# Occupy the old UID to exercise restoration with a different service UID.
useradd --system --uid "$old_uid" --no-create-home --shell /bin/false zenohd-ci-reserved
dpkg --force-confold --configure zenohd
[ "$(id -u zenohd)" != "$old_uid" ] || fail "service UID did not change"
assert_data
assert_no_snapshots
userdel zenohd-ci-reserved

echo "=== Upgrade between fixed versions ==="
fixed_uid=$(id -u zenohd)
install_package 1.11.1
assert_data
assert_no_snapshots
[ "$(id -u zenohd)" = "$fixed_uid" ] || fail "fixed upgrade changed the service UID"
cmp /etc/zenohd/zenohd.json5.dpkg-dist "$work/1.11.1/etc/zenohd/zenohd.json5"

echo "=== Unknown maintainer-script action ==="
if sh "$repo/zenohd/.deb/postrm" unknown-action > "$work/unknown.log" 2>&1; then
    fail "postrm accepted an unknown action"
fi
grep -F 'postrm called with unknown argument' "$work/unknown.log"
assert_data

echo "=== Remove preserves data; reinstall restores the package ==="
dpkg --remove zenohd
assert_data
assert_no_snapshots
install_package 1.11.1
assert_data
assert_no_snapshots

echo "=== Purge deletes configuration, state, and account ==="
dpkg --purge zenohd
assert_purged

echo "=== Retry an interrupted legacy upgrade ==="
install_package 1.10.1
seed_data
dpkg --unpack "$work/zenohd_1.11.0.deb"
dpkg --unpack "$work/zenohd_1.11.0.deb"
dpkg --force-confold --configure zenohd
assert_data
assert_no_snapshots
dpkg --purge zenohd
assert_purged

echo "=== Purge an interrupted legacy upgrade, then reinstall ==="
install_package 1.10.1
seed_data
dpkg --unpack "$work/zenohd_1.11.0.deb"
dpkg --purge zenohd
assert_purged
install_package 1.11.0
cmp /etc/zenohd/zenohd.json5 "$work/1.11.0/etc/zenohd/zenohd.json5"
[ ! -e /etc/zenohd/custom.json5 ] || fail "reinstall resurrected custom configuration"
[ ! -e /var/zenohd/db ] || fail "reinstall resurrected purged state"
assert_no_snapshots
dpkg --purge zenohd
assert_purged

echo "All Debian packaging lifecycle tests passed."
