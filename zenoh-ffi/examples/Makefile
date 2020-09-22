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

debug:
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi zn_sub.c -o zn_sub
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi zn_pub.c -o zn_pub
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi zn_write.c -o zn_write
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi zn_query.c -o zn_query
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi zn_eval.c -o zn_eval
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi zn_pull.c -o zn_pull
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi zn_info.c -o zn_info

release:
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi zn_sub_thr.c -o zn_sub_thr
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi zn_pub_thr.c -o zn_pub_thr
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi zn_sub.c -o zn_sub
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi zn_pub.c -o zn_pub
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi zn_write.c -o zn_write
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi zn_query.c -o zn_query
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi zn_eval.c -o zn_eval
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi zn_pull.c -o zn_pull
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi zn_info.c -o zn_info

all:
	make debug
	make release

clean:
	$(RM) zn_sub zn_pub zn_write zn_query zn_eval zn_pull zn_info zn_sub_thr zn_pub_thr