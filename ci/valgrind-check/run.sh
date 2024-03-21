#!/usr/bin/env bash
set -e
function check_leaks {
  echo "Checking $1 for memory leaks"
  valgrind --leak-check=full --num-callers=50 --log-file="$1_leaks.log" ./target/debug/$1
  num_leaks=$(grep 'ERROR SUMMARY: [0-9]+' -Eo "$1_leaks.log" | grep '[0-9]+' -Eo)
  echo "Detected $num_leaks memory leaks"
  if (( num_leaks == 0 ))
  then
    return 0
  else
    return -1
  fi
}

cargo build
check_leaks "queryable_get"
check_leaks "pub_sub"