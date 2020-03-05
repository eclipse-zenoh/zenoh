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

get_nodeid()
{
  echo $1 | cut -d' ' -f 4 | cut -d')' -f1
}

get_parent()
{
  echo $1 | cut -d'(' -f 9 | cut -d')' -f1
}

getnodes()
{
  cat $1 | grep "\-\-" | sed "s% *\([0-9]*\) *-- *\([0-9]*\)[^0-9].*%\1|\2%g" | tr '|' '\n' | sort -u
  #cat $1 | grep "\-\-" | sed "s%--%%g" | sed "s%{%%g" | sed "s%}%%g" | sed "s%\([^;]*\);.*%\1%g" | tr -s ' ' '\n' | sed '/^\s*$/d' | sort -u 
}

get_pid_from_data()
{
  echo $1 | cut -d'[' -f4 | cut -d']' -f1
}


broker_cmd()
{
  graph=$1
  node=$2
  port=$node
  httpport=$(($port - 1000))
  peers=""
  for peer in `cat $graph | grep -e "$node *\-\-" | sed "s% *\([0-9]*\) *-- *\([0-9]*\)[^0-9].*%\2%g"`
  do 
    if [[ $peers == "" ]] 
    then 
      peers="tcp/127.0.0.1:$peer"
    else
      peers="$peers,tcp/127.0.0.1:$peer"
    fi
  done
  #printf "%-100s > %s\n" "zenohd.exe -t $i -p $peers" "\$outdir/zenohd_$i.log 2>&1"
  if [[ $peers == "" ]] 
  then 
    echo "zenohd.exe -P \"$ZENOH_HTTP -h $httpport\" -t $port -s $node"
  else
    echo "zenohd.exe -P \"$ZENOH_HTTP -h $httpport\" -t $port -s $node -p $peers"
  fi
}

get_sessions()
{
  cat $1 | grep "Received tree state on" | cut -d' ' -f6 | sort -u
}

get_pid_from_port()
{
  cat $2/zenohd-$1.log | grep Local | head -n 1 | sed "s%.*node_id \([^)]*\)).*%\1%g"
}

get_pid_from_sid()
{
  cat $1 | grep "Received tree state on $2" | cut -d'(' -f2  | cut -d')' -f1 | sort -u
}

gentreesgraph()
{
  graph=$1
  graphname=$(basename $graph)
  folder=$2
  suffix=$3
  label=$4
  output=$folder/$graphname-$suffix-trees

  colors[0]="red"
  colors[1]="blue"
  colors[2]="green"
  colors[3]="magenta"
  colors[4]="cyan"
  colors[5]="yellow"


  echo "digraph G {" > $output
  echo "  fontname=\"Andale Mono\"" >> $output
  echo "  label=\"$label\"" >> $output
  echo "  nodesep=0.15" >> $output
  echo "  node [penwidth=2 shape=box style=rounded fontname=\"Andale Mono\"]" >> $output

  # copy all nodes
  for node in $(getnodes $graph)
  do 
    echo "  \"$(get_pid_from_port $node $folder)\" [label = \"$node\"]" >> $output
  done

  for i in $(getnodes $graph)
  do
    status=`cat $folder/zenohd-$i.log | grep "Local\|kill" | tail -n 1`
    if [[ $status == *"kill"* ]]
    then 
      echo "  \"$(get_pid_from_port $i $folder)\" [style=dotted]" >> $output
    else 
      for j in 0 1 2 3 4 5
      do 
        status=`cat $folder/zenohd-$i.log | grep "Local" | grep "tree_nb $j" | tail -n 1`
        if [[ ! $status == "" ]]
        then 
          nodeid=$(get_nodeid "$status")
          parent=$(get_parent "$status")
          
          if [[ $parent == "" ]]
          then 
            echo "  \"$nodeid\" [fillcolor=${colors[$j]} penwidth=2 shape=box style=\"rounded,filled\" fontname=\"Andale Mono\"]" >> $output
          fi
        fi
      done
    fi
  done 

  for j in 0 1 2 3 4 5
  do 
    echo "  subgraph Tree$j {" >> $output
    echo "      edge [color=${colors[$j]}  penwidth=2]" >> $output
    for i in $(getnodes $graph)
    do
      cat $folder/zenohd-$i.log | grep "Local\|kill" | grep "tree_nb $j\|kill" | tail -n 1 | while read status
      do 
        if [[ ! $status == *"kill"* ]]
        then 
          nodeid=$(get_nodeid "$status")
          parent=$(get_parent "$status")
          if [[ $parent != "" ]]
          then 
            echo "      \"$nodeid\" -> \"$parent\"" >> $output
          fi
        fi
      done 
    done
    echo "  }" >> $output
      
  done

  echo "  subgraph Base {" >> $output
  echo "    edge [dir=none; style=dashed]" >> $output
  cat $graph | grep "\-\-" | while read pair
  do 
    src=`echo $pair | cut -d'-' -f1 | tr -d '[:space:]'`
    dst=`echo $pair | cut -d'-' -f3 | cut -d';' -f1 | cut -d'[' -f1 | tr -d '[:space:]'`
    existingedges=`cat $output | grep "$(get_pid_from_port $src $folder)" | grep "$(get_pid_from_port $dst $folder)" | wc -l`
    if [ $existingedges -le 0 ]
    then 
      echo "    \"$(get_pid_from_port $src $folder)\" -> \"$(get_pid_from_port $dst $folder)\"" >> $output
    fi
  done
  
  echo "  }" >> $output


  echo "}" >> $output

  neato -Tpng $output -o $folder/$graphname-$suffix-trees.png
  echo "generated $folder/$graphname-$suffix-trees.png"
}

genflowgraph()
{
  graph=$1
  graphname=$(basename $graph)
  folder=$2
  suffix=$3
  label=$4
  output=$folder/$graphname-$suffix-flow

  colors[0]="red"
  colors[1]="blue"
  colors[2]="green"
  colors[3]="magenta"
  colors[4]="cyan"
  colors[5]="yellow"


  echo "digraph G {" > $output

  echo "  fontname=\"Andale Mono\"" >> $output
  echo "  label=\"$label\"" >> $output
  echo "  nodesep=0.15" >> $output
  echo "  node [penwidth=2 shape=box style=rounded fontname=\"Andale Mono\"]" >> $output
  

  # copy all nodes
  for node in $(getnodes $graph)
  do 
    #status=`cat $folder/zenohd-$node.log | grep "Local\|kill" | tail -n 1`
    status=`tail -n 50 $folder/zenohd-$node.log | grep "Local\|kill" | tail -n 1`
    if [[ $status == *"kill"* ]]
    then 
      echo "  \"$(get_pid_from_port $node $folder)\" [label = \"$node\"; style=dotted]" >> $output
    else
      echo "  \"$(get_pid_from_port $node $folder)\" [label = \"$node\"]" >> $output
    fi
  done

  now=`date +'%s'`
  limit=`expr $now - 1`

  for j in red green blue
  do 
    
    echo "  subgraph $j {" >> $output
    echo "      edge [color=$j penwidth=2]" >> $output
    for node in $(getnodes $graph)
    do
      dst=$(get_pid_from_port $node $folder)
      #cat $folder/zenohd-$node.log | grep "Handling" | grep "$j" | while read h 
      tail -n 50 $folder/zenohd-$node.log | grep "Handling" | grep "$j" | while read h 
      do 
        ts=`echo "$h" | cut -d'[' -f2 | cut -d'.' -f1`
        if [ $ts -ge $limit ]
        then
          echo "$h"
        fi
      done | cut -d'[' -f4 | cut -d']' -f1 | sort -u | while read src 
      do 
        if [ "$src" != "UNKNOWN" ]
        then
          echo "      \"$src\" -> \"$dst\"" >> $output
        fi
      done
    done
    echo "  }" >> $output
      
  done

  echo "  subgraph Base {" >> $output
  echo "    edge [dir=none; style=dashed]" >> $output
  cat $graph | grep "\-\-" | while read pair
  do 
    src=`echo $pair | cut -d'-' -f1 | tr -d '[:space:]'`
    dst=`echo $pair | cut -d'-' -f3 | cut -d';' -f1 | cut -d'[' -f1 | tr -d '[:space:]'`
    existingedges=`cat $output | grep "$(get_pid_from_port $src $folder)" | grep "$(get_pid_from_port $dst $folder)" | wc -l`
    if [ $existingedges -le 0 ]
    then 
      echo "    \"$(get_pid_from_port $src $folder)\" -> \"$(get_pid_from_port $dst $folder)\"" >> $output
    fi
  done
  echo "  }" >> $output

  echo "}" >> $output

  neato -Tpng $output -o $folder/$graphname-$suffix-flow.png
  echo "generated $folder/$graphname-$suffix-flow.png"
}
