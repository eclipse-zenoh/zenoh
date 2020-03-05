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
export ZENOD_VERBOSITY=debug

basename=`basename $0`
filename="${basename%.*}"
outdir=${filename}_`date +"%y-%m-%d_%H-%M"`
mkdir $outdir

. init.sh

echo "-------- START test $filename"

run_brokers ../../../common/graph3 $outdir

sleep 5

runproc zenohc_sub $outdir zenohc.exe -p tcp/127.0.0.1:9022
sub=$?

echo "open" > ${proc_in[$sub]}
echo "dres 10 //test/res1" > ${proc_in[$sub]}
echo "dsub 10" > ${proc_in[$sub]}

sleep 2

runproc zenohc_pub $outdir zenohc.exe -p tcp/127.0.0.1:9026
pub=$?

echo "open" > ${proc_in[$pub]}
echo "dres 5 //test/res1" > ${proc_in[$pub]}
echo "dpub 5" > ${proc_in[$pub]}
echo "pub 5 MSG" > ${proc_in[$pub]}

sleep 1

cleanall

if [ `cat ${proc_log[$sub]} | grep MSG | wc -l` -gt 0 ]
then 
  echo "[OK]"
  echo "-------- END test $filename"
  echo ""
  exit 0
else
  echo "[ERROR] sub didn't receive MSG"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
