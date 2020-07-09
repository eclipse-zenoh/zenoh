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

void sub_callback(const char* key, unsigned int klen, const char* data, unsigned int dlen) {    
    printf(">> Received:\n\t (%.*s, %.*s)\n", klen, key, dlen, data);    
}

int main(int argc, char** argv) {
    char *key_expr = "/demo/example/**";
    // char *key_expr = "/test/*";
    if (argc > 1) {        
        key_expr = argv[1];        
    }
    printf("Subscription expression to %s\n", key_expr);

    ZNSession *s = zn_open(PEER_MODE, 0, 0);

    if (s == 0) {
        printf("Error creating session!\n");
        exit(-1);
    } 

    sleep(1);

    zn_declare_subscriber(s, key_expr, sub_callback);

    char ch;
    read(0, &ch, 1);
}