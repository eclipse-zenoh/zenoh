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

echo "====== START bro tests"

./test_bro_2b_1/test_bro_2b_1.sh && \
./test_bro_2b_2/test_bro_2b_2.sh && \
./test_bro_2b_3/test_bro_2b_3.sh && \
./test_bro_2b_4/test_bro_2b_4.sh && \
./test_bro_2b_5/test_bro_2b_5.sh && \
./test_bro_2b_6/test_bro_2b_6.sh && \
./test_bro_2b_7/test_bro_2b_7.sh && \
./test_bro_14b_1/test_bro_14b_1.sh && \
./test_bro_14b_2/test_bro_14b_2.sh && \
./test_bro_14b_3/test_bro_14b_3.sh && \
./test_bro_14b_4/test_bro_14b_4.sh && \
./test_bro_50b_1/test_bro_50b_1.sh && \
./test_bro_50b_2/test_bro_50b_2.sh

if [ $? -eq 0 ]
then
    echo "[OK]"
    echo "====== END bro tests"
    echo ""
    exit 0
else
    echo "[ERROR]"
    echo "====== END bro tests"
    echo ""
    exit -1
fi
