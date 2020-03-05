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

echo "====== START c2b tests"

./test_c2b_1/test_c2b_1.sh && \
./test_c2b_2/test_c2b_2.sh && \
./test_c2b_3/test_c2b_3.sh && \
./test_c2b_4/test_c2b_4.sh && \
./test_c2b_5/test_c2b_5.sh && \
./test_c2b_6/test_c2b_6.sh 

if [ $? -eq 0 ]
then
    echo "[OK]"
    echo "====== END c2b tests"
    echo ""
    exit 0
else
    echo "[ERROR]"
    echo "====== END c2b tests"
    echo ""
    exit -1
fi
