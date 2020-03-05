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

run_brokers ../../../common/graph2 $outdir

sleep 1

runproc zenohc_sub1 $outdir zenohc.exe -p tcp/127.0.0.1:9005
sub1=$?

echo "open" > ${proc_in[$sub1]}
echo "dres 10 //test/res1" > ${proc_in[$sub1]}
echo "dsub 10" > ${proc_in[$sub1]}

sleep 1 

runproc zenohc_sub2 $outdir zenohc.exe -p tcp/127.0.0.1:9011
sub2=$?

echo "open" > ${proc_in[$sub2]}
echo "dres 10 //test/res1" > ${proc_in[$sub2]}
echo "dsub 10" > ${proc_in[$sub2]}

sleep 1 

runproc zenohc_sub3 $outdir zenohc.exe -p tcp/127.0.0.1:9010
sub3=$?

echo "open" > ${proc_in[$sub3]}
echo "dres 10 //test/res1" > ${proc_in[$sub3]}
echo "dsub 10" > ${proc_in[$sub3]}

sleep 1 

runproc zenohc_sub4 $outdir zenohc.exe -p tcp/127.0.0.1:9010
sub4=$?

echo "open" > ${proc_in[$sub4]}
echo "dres 10 //test/res1" > ${proc_in[$sub4]}
echo "dsub 10" > ${proc_in[$sub4]}

sleep 1 

runproc zenohc_pub1 $outdir zenohc.exe -p tcp/127.0.0.1:9014
pub1=$?

echo "open" > ${proc_in[$pub1]}
echo "dres 5 //test/res1" > ${proc_in[$pub1]}
echo "dpub 5" > ${proc_in[$pub1]}

sleep 1

runproc zenohc_pub2 $outdir zenohc.exe -p tcp/127.0.0.1:9008
pub2=$?

echo "open" > ${proc_in[$pub2]}
echo "dres 5 //test/res1" > ${proc_in[$pub2]}
echo "dpub 5" > ${proc_in[$pub2]}

sleep 1

runproc zenohc_pub3 $outdir zenohc.exe -p tcp/127.0.0.1:9009
pub3=$?

echo "open" > ${proc_in[$pub3]}
echo "dres 5 //test/res1" > ${proc_in[$pub3]}
echo "dpub 5" > ${proc_in[$pub3]}

sleep 1

echo "pub 5 MSG1" > ${proc_in[$pub1]}

echo "pub 5 MSG2" > ${proc_in[$pub2]}

echo "pub 5 MSG3" > ${proc_in[$pub3]}

sleep 1

cleanall

if [ `cat ${proc_log[$sub1]} | grep MSG1 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub1 didn't receive MSG1"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
if [ `cat ${proc_log[$sub1]} | grep MSG2 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub1 didn't receive MSG2"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
if [ `cat ${proc_log[$sub1]} | grep MSG3 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub1 didn't receive MSG3"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi

if [ `cat ${proc_log[$sub2]} | grep MSG1 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub2 didn't receive MSG1"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
if [ `cat ${proc_log[$sub2]} | grep MSG2 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub2 didn't receive MSG2"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
if [ `cat ${proc_log[$sub2]} | grep MSG3 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub2 didn't receive MSG3"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi

if [ `cat ${proc_log[$sub3]} | grep MSG1 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub3 didn't receive MSG1"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
if [ `cat ${proc_log[$sub3]} | grep MSG2 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub3 didn't receive MSG2"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
if [ `cat ${proc_log[$sub3]} | grep MSG3 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub3 didn't receive MSG3"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi

if [ `cat ${proc_log[$sub4]} | grep MSG1 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub4 didn't receive MSG1"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
if [ `cat ${proc_log[$sub4]} | grep MSG2 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub4 didn't receive MSG2"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
if [ `cat ${proc_log[$sub4]} | grep MSG3 | wc -l` -lt 1 ]
then 
  echo "[ERROR] sub4 didn't receive MSG3"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi

echo "[OK]"
echo "-------- END test $filename"
echo ""
  exit 0
