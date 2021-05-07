
#
# Copyright (c) 2017, 2020 ADLINK Technology Inc.
#
# This program and the accompanying materials are made available under the
# terms of the Eclipse Public License 2.0 which is available at
# http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
# which is available at https://www.apache.org/licenses/LICENSE-2.0.
#
# SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
#
# Contributors:
#   ADLINK zenoh team, <zenoh@adlink-labs.tech>
#

# Script generating the "zenoh" top-level Debian package

if [ -z "$1" -o -z "$2" ]; then
    echo "Usage: $0 TARGET ARCH"
    echo "  example: $0 x86_64-unknown-linux-gnu amd64"
    exit 1
fi

TARGET=$1
ARCH=$2

VERSION=`git describe --abbrev=0 | sed s/-/~/g`
PACKAGE_NAME="zenoh_${VERSION}_${ARCH}"
TARGET_DIR="target/${TARGET}/debian/${PACKAGE_NAME}"
CONTROL_FILE="${TARGET_DIR}/DEBIAN/control"

echo "Generate zenoh top-level package: target/${TARGET}/debian/${PACKAGE_NAME}.deb ..."
# create control file for zenoh deb package
mkdir -p ${TARGET_DIR}/DEBIAN
echo "Package: zenoh " > ${CONTROL_FILE}
echo "Version: ${VERSION} " >> ${CONTROL_FILE}
echo "Architecture: ${ARCH}" >> ${CONTROL_FILE}
echo "Vcs-Browser: https://github.com/eclipse-zenoh/zenoh" >> ${CONTROL_FILE}
echo "Vcs-Git: https://github.com/eclipse-zenoh/zenoh" >> ${CONTROL_FILE}
echo "Homepage: http://zenoh.io" >> ${CONTROL_FILE}
echo "Section: net " >> ${CONTROL_FILE}
echo "Priority: optional" >> ${CONTROL_FILE}
echo "Essential: no" >> ${CONTROL_FILE}
echo "Installed-Size: 1024 " >> ${CONTROL_FILE}
echo "Depends: zenohd, zenoh-plugin-rest, zenoh-plugin-storages " >> ${CONTROL_FILE}
echo "Maintainer: zenoh-dev@eclipse.org " >> ${CONTROL_FILE}
echo "Description: The zenoh top-level package" >> ${CONTROL_FILE}
echo "" >> ${CONTROL_FILE}

dpkg-deb --build ${TARGET_DIR}
