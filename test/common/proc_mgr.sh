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
proc_cmd=""
proc_pid=""
proc_in=""
proc_log=""
lastproc=10

runproc()
{
  name=$1
  shift
  outdir=$1
  shift
  lastproc=`expr $lastproc + 1`
  proc_cmd[$lastproc]="$*"
  proc_in[$lastproc]=$outdir/$name.in 
  proc_log[$lastproc]=$outdir/$name.log

  mkfifo ${proc_in[$lastproc]}
  eval "exec $lastproc<>${proc_in[$lastproc]}"
  eval "${proc_cmd[$lastproc]} < ${proc_in[$lastproc]} > ${proc_log[$lastproc]} 2>&1 &"
  proc_pid[$lastproc]=$!
  printf "run %3i %-20s > %s (%s)\n" "$lastproc" "$name" "${proc_log[$lastproc]}" "${proc_pid[$lastproc]}"
  usname=`echo "$name" | sed "s%-%_%g" | sed "s% %_%g"`
  eval "$usname=$lastproc"
  return $lastproc
}

restartproc()
{
  if [ "${proc_pid[$1]}" != "" ] 
  then
    echo "proc $1 already running (pid : ${proc_pid[$1]})"
  else
    mkfifo ${proc_in[$1]}
    eval "exec $1<>${proc_in[$1]}"
    eval "${proc_cmd[$1]} < ${proc_in[$1]} > ${proc_log[$1]} 2>&1 &"
    proc_pid[$1]=$!
  fi
}

killproc()
{
  eval "exec $1>&-"
  rm -f ${proc_in[$1]}
  kill -9 ${proc_pid[$1]}
  proc_pid[$1]=""
  echo "**********************************************" >> ${proc_log[$1]}
  echo "kill -9" >> ${proc_log[$1]}
  echo "**********************************************" >> ${proc_log[$1]}
}

cleanall()
{
  for i in $(seq 11 $lastproc)
  do
    killproc $i
  done
}
