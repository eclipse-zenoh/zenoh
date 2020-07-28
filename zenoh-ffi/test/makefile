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
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi test_session.c -o test_session
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi test_sub.c -o test_sub
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi test_query.c -o test_query
	clang -I ../include -L ../../target/debug/ -lzenoh_ffi test_queryable.c -o test_queryable

release:
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi session_test.c -o session_test
	clang -O3 -I ../include -L ../../target/release/ -lzenoh_ffi thrput_test.c -o thrput_test
	clang -O3 -I ../include -L ../../target/debug/ -lzenoh_ffi test_sub.c -o test_sub
	clang -O3 -I ../include -L ../../target/debug/ -lzenoh_ffi test_query.c -o test_query
	clang -O3 -I ../include -L ../../target/debug/ -lzenoh_ffi test_queryable.c -o test_queryable

all:
	make debug
	make release

clean:
	$(RM) thrput_test test_session test_sub test_query