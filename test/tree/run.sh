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
source ../common/init.sh

export ZENOD_VERBOSITY=debug

graphname=$(basename $1)

folder=run_${graphname}_`date +"%y-%m-%d_%H-%M"`

mkdir $folder

cp $1 $folder/$graphname

neato -Tpng $1 -o $folder/$graphname.png

run_brokers $1 $folder

sleep 2

gentreesgraph $1 $folder

cleanall

convert $folder/$graphname.png $folder/$graphname--trees.png -append $folder/$graphname-all.png

