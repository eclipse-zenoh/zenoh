#!/bin/bash

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

SOURCE="${BASH_SOURCE[0]}"
while [ -h "$SOURCE" ]; do # resolve $SOURCE until the file is no longer a symlink
  TARGET="$(readlink "$SOURCE")"
  if [[ $TARGET == /* ]]; then
    SOURCE="$TARGET"
  else
    SOURCE="$( dirname "$SOURCE" )/$TARGET" # if $SOURCE was a relative symlink, we need to resolve it relative to the path where the symlink file was located
  fi
done
RUNDIR="$( dirname "$SOURCE" )"
ZENOHDIR="$( cd -P "$( dirname "$SOURCE" )" && cd ../.. &&  pwd )"

if [ -d "$ZENOHDIR/_build/default" ]; then BUILDDIR="$ZENOHDIR/_build/default"; fi
if [ -d "$ZENOHDIR/../../_build/default/lib/zenoh" ]; then BUILDDIR="$ZENOHDIR/../../_build/default/lib/zenoh"; fi

export PATH=${BUILDDIR}/src/zenoh-router-daemon/:$PATH
export PATH=${BUILDDIR}/src/zenoh-cat/:$PATH
export PATH=${BUILDDIR}/example/throughput/:$PATH
export PATH=${BUILDDIR}/example/roundtrip/:$PATH
export PATH=${ZENOHDIR}/test/common/:$PATH

export ZENOH_HTTP=${BUILDDIR}/src/zenoh-http/zenoh-http-plugin.cmxs

source proc_mgr.sh
source graph_tools.sh
source demo_tools.sh
