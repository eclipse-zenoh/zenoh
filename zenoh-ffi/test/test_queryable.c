/*
 * Copyright (c) 2017, 2020 ADLINK Technology Inc.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Eclipse Public License 2.0 which is available at
 * http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
 * which is available at https://www.apache.org/licenses/LICENSE-2.0.
 *
 * SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
 *
 * Contributors:
 *   ADLINK zenoh team, <zenoh@adlink-labs.tech>
 */
#include "zenoh-ffi.h"
#include <stdlib.h>
#include <stdio.h>
#include <unistd.h>
#include <string.h>

const char *key_expr = "/demo/example/zenoh-rs-eval";
const char *value = "Hello from C";

void replier(ZNQuery *query) {
    const zn_string *res = zn_query_res_name(query);
    const zn_string *pred = zn_query_predicate(query);
    printf("Received query: %.*s:%.*s\n", res->len, res->val, pred->len, pred->val);
    zn_send_reply(query, key_expr, (const unsigned char *)value, strlen(value));
}

int main(int argc, char** argv) {
    ZNQueryable *q = 0;

    if (argc > 1) {
        key_expr = argv[1];
    }
    printf("Subscription expression to %s\n", key_expr);

    ZNSession *s = zn_open(PEER, 0, 0);

    if (s == 0) {
        printf("Error creating session!\n");
        exit(-1);
    }

    q = zn_declare_queryable(s, key_expr, EVAL, replier);
    if (q == 0) {
        printf("Unable to register queryable\n");
        return -1;
    }
    char ch;
    printf("Press a key to terminate...\n");
    scanf("%c", &ch);

    zn_undeclare_queryable(q);
}
