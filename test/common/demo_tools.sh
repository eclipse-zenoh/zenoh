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

run_brokers()
{
  graph=$1
  graphname=$(basename $1)
  defaultfolder=run_${graphname}_`date +"%y-%m-%d_%H-%M"`
  folder=${2:-$defaultfolder}
  delay=${3:-0}

  mkdir $folder
  
  for i in `getnodes $graph | sort -r -u`
  do
    runproc zenohd-$i $folder `broker_cmd $graph $i`
    sleep $delay
  done
}


run_client()
{
  runproc zenohc-$1 $2 zenohc.exe -p tcp/127.0.0.1:$1
}

sub()
{
  printf "open\ndres $1 $2\ndsub $1\n" 
}

pub()
{
  printf "open\nwriten $1 $2 1 $3\n" 
}


monitortrees()
{
  frame=1

  gentreesgraph $1 $2 live "frame $frame"

  "${3:-code}" -n $2/$(basename $1)-live-trees.png

  while true
  do 
    sleep ${4:-0}
    frame=$(($frame+1))
    gentreesgraph $1 $2 live "frame $frame"
  done
}

monitorflow()
{
  frame=1

  genflowgraph $1 $2 live "frame $frame"

  "${3:-code}" -n $2/$(basename $1)-live-flow.png

  while true
  do 
    sleep ${4:-0}
    frame=$(($frame+1))
    genflowgraph $1 $2 live "frame $frame"
  done
}
