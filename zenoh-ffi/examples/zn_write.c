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
#include <stdio.h>
#include <unistd.h>
#include <string.h>
#include "zenoh-ffi.h"

int main(int argc, char **argv) {
    char *uri = "/demo/example/zenoh-c-write";
    if (argc > 1) {
        uri = argv[1];
    }
    char *value = "Write from C!";
    if (argc > 2) {
        value = argv[2];
    }
    char *locator = 0;
    if (argc > 3) {
        locator = argv[3];
    }

    printf("Openning session...\n");
    ZNSession *s = zn_open(PEER, locator, 0);
    if (s == 0) {
        printf("Unable to open session!\n");
        exit(-1);
    }

    zn_write(s, uri, value, strlen(value));

    zn_close(s);
    return 0;
}