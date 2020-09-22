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
#include "zenoh-ffi.h"

void data_handler(const zn_sample *sample) {
    printf(">> [Subscription listener] Received (%.*s, %.*s)\n",
        sample->key.len, sample->key.val,
        sample->value.len, sample->value.val);
}

int main(int argc, char **argv) {
    char *uri = "/demo/example/**";
    if (argc > 1) {
        uri = argv[1];
    }
    char *locator = 0;
    if (argc > 2) {
        locator = argv[2];
    }

    printf("Openning session...\n");
    ZNSession *s = zn_open(PEER, locator, 0);
    if (s == 0) {
        printf("Unable to open session!\n");
        exit(-1);
    }

    printf("Declaring Subscriber on '%s'...\n", uri);
    ZNSubscriber *sub = zn_declare_subscriber(s, uri, zn_subinfo_pull(), data_handler);
    if (sub == 0) {
        printf("Unable to declare subscriber.\n");
        exit(-1);
    }

    printf("Press <enter> to pull data...\n");
    char c = 0;
    while (c != 'q') {
        c = fgetc(stdin);
        zn_pull(sub);
    }

    zn_undeclare_subscriber(sub);
    zn_close(s);
    return 0;
}