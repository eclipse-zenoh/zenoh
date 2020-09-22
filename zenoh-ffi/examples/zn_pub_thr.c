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

int main(int argc, char** argv) {
  char *locator = 0;
  if (argc < 2) {
    printf("USAGE:\n\tzn_pub_thr <payload-size> [<zenoh-locator>]\n\n");
    exit(-1);
  }
  size_t len = atoi(argv[1]);  
  printf("Running throughput test for payload of %zu bytes.\n", len);
  if (argc > 2) {
    locator = argv[2];
  }  

  ZNSession *s = zn_open(PEER, locator, 0);
  if (s == 0) {
    printf("Unable to open session!\n");
    exit(-1);
  }

  char *data = (char*) malloc(len);
  memset(data, 1, len);

  size_t rid = zn_declare_resource(s, "/test/thr");
  while (1) {
    zn_write_wrid(s, rid, data, len);
  }
}