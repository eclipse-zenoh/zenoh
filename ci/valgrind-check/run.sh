#!/usr/bin/env bash
# Copyright (c) 2024 ZettaScale Technology
#
# This program and the accompanying materials are made available under the
# terms of the Eclipse Public License 2.0 which is available at
# http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
# which is available at https://www.apache.org/licenses/LICENSE-2.0.
#
# SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
#
# Contributors:
#   ZettaScale Zenoh Team, <zenoh@zettascale.tech>

set -e
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

function check_leaks {
  echo "Checking $1 for memory leaks"
  valgrind --leak-check=full --num-callers=50 --log-file="$SCRIPT_DIR/$1_leaks.log" $SCRIPT_DIR/target/debug/$1
  num_leaks=$(grep 'ERROR SUMMARY: [0-9]+' -Eo "$SCRIPT_DIR/$1_leaks.log" | grep '[0-9]+' -Eo)
  echo "Detected $num_leaks memory leaks"
  if (( num_leaks == 0 ))
  then
    return 0
  else
    cat $SCRIPT_DIR/$1_leaks.log
    return -1
  fi
}

cargo build --manifest-path=$SCRIPT_DIR/Cargo.toml
check_leaks "queryable_get"
check_leaks "pub_sub"