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
basename=`basename $0`
filename="${basename%.*}"
outdir=${filename}_`date +"%y-%m-%d_%H-%M"`
mkdir $outdir
export ZENOD_VERBOSITY=debug

. proc_mgr.sh

echo "-------- START test $filename"

runproc zenohd $outdir zenohd.exe
zenohd=$?

sleep 1

runproc zenohc_sub1 $outdir zenohc.exe
sub1=$?

echo "open" > ${proc_in[$sub1]}
echo "dres 10 //test/res*"> ${proc_in[$sub1]}
echo "dsub 10"> ${proc_in[$sub1]}

sleep 1

runproc zenohc_sub2 $outdir zenohc.exe
sub2=$?
 
echo "open" > ${proc_in[$sub2]}
echo "dsub 10"> ${proc_in[$sub2]}

sleep 1

runproc zenohc_pub1 $outdir zenohc.exe
pub1=$?

echo "open" > ${proc_in[$pub1]}
echo "dres 10 //test/res1" > ${proc_in[$pub1]}
echo "dpub 10" > ${proc_in[$pub1]}
echo "pub 10 MSG_RES1" > ${proc_in[$pub1]}

sleep 1

runproc zenohc_pub2 $outdir zenohc.exe
pub2=$?

echo "open" > ${proc_in[$pub2]}
echo "dres 5 //test/res2" > ${proc_in[$pub2]}
echo "dpub 5" > ${proc_in[$pub2]}
echo "pub 5 MSG_RES2" > ${proc_in[$pub2]}

echo "dpub 10" > ${proc_in[$pub2]}
echo "pub 10 MSG_SUB2" > ${proc_in[$pub2]}

sleep 1

cleanall

if [ `cat ${proc_log[$sub1]} | grep MSG_RES1 | wc -l` -eq 0 ]
then
  echo "[ERROR] zenohc_sub1 didn't receive MSG_RES1"
  echo "-------- END test $filename"
  echo ""
  exit -1
elif [ `cat ${proc_log[$sub1]} | grep MSG_RES2 | wc -l` -eq 0  ]
  then
  echo "[ERROR] zenohc_sub1 didn't receive MSG_RES2"
  echo "-------- END test $filename"
  echo ""
  exit -1
elif [ `cat ${proc_log[$sub1]} | grep MSG_SUB2 | wc -l` -gt 0  ]
then
  echo "[ERROR] zenohc_sub1 received MSG_SUB2"
  echo "-------- END test $filename"
  echo ""
  exit -1
elif [ `cat ${proc_log[$sub2]} | grep MSG_SUB2 | wc -l` -eq 0  ]
then
  echo "[ERROR] zenohc_sub2 didn't receive MSG_SUB2"
  echo "-------- END test $filename"
  echo ""
  exit -1
elif [ `cat ${proc_log[$sub2]} | grep MSG_RES1 | wc -l` -gt 0  ]
then
  echo "[ERROR] zenohc_sub2 received MSG_RES1"
  echo "-------- END test $filename"
  echo ""
  exit -1
elif [ `cat ${proc_log[$sub2]} | grep MSG_RES2 | wc -l` -gt 0  ]
then
  echo "[ERROR] zenohc_sub2 received MSG_RES2"
  echo "-------- END test $filename"
  echo ""
  exit -1
else
  echo "[OK]"
  echo "-------- END test $filename"
  echo ""
  exit 0
fi