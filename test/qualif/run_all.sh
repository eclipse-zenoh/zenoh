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
cd "$(dirname $0)"
source ../common/init.sh

./test_c2b/run_all.sh && \
./test_bro/run_all.sh

if [ $? -eq 0 ]
then
    echo "[OK]"
    exit 0
else
    echo "[ERROR]"
    exit -1
fi
