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

echo "-------- START test $filename"

printf "run %-20s > %s\n" "zenohd 7447" "$outdir/zenohd_7447.log"
zenohd.exe > $outdir/zenohd_7447.log 2>&1 &
zenohd7447pid=$!

sleep 1

printf "run %-20s > %s\n" "zenohd 7448" "$outdir/zenohd_7448.log"
zenohd.exe -t 7448 -p tcp/127.0.0.1:7447 > $outdir/zenohd_7448.log 2>&1 &
zenohd7448pid=$!

sleep 1

printf "run %-20s > %s\n" "zenohc pub" "$outdir/zenohc_pub.log"
mkfifo $outdir/zenohc_pub.in
exec 4<>$outdir/zenohc_pub.in 
zenohc.exe < $outdir/zenohc_pub.in > $outdir/zenohc_pub.log 2>&1 &
zenohcpubpid=$!

echo "open" > $outdir/zenohc_pub.in
echo "dres 5 //test/res1" > $outdir/zenohc_pub.in
echo "dpub 5" > $outdir/zenohc_pub.in

sleep 1

printf "run %-20s > %s\n" "zenohc sub" "$outdir/zenohc_sub.log"
mkfifo $outdir/zenohc_sub.in
exec 3<>$outdir/zenohc_sub.in 
zenohc.exe -p tcp/127.0.0.1:7448 < $outdir/zenohc_sub.in > $outdir/zenohc_sub.log 2>&1 &
zenohcsubpid=$!

echo "open" > $outdir/zenohc_sub.in
echo "dres 10 //test/res1"> $outdir/zenohc_sub.in
echo "dsub 10"> $outdir/zenohc_sub.in

sleep 1

echo "pub 5 MSG" > $outdir/zenohc_pub.in

sleep 1

exec 3>&-
exec 4>&-

kill -9 $zenohd7447pid
kill -9 $zenohd7448pid
kill -9 $zenohcsubpid
kill -9 $zenohcpubpid

rm -f $outdir/zenohc_sub.in 
rm -f $outdir/zenohc_pub.in

if [ `cat $outdir/zenohc_sub.log | grep MSG | wc -l` -gt 0 ]
then 
  echo "[OK]"
  echo "-------- END test $filename"
  echo ""
  exit 0
else
  echo "[ERROR] zenohc_sub didn't receive MSG"
  echo "-------- END test $filename"
  echo ""
  exit -1
fi
