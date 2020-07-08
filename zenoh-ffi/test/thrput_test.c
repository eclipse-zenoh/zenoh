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
  size_t len;
  if (argc < 2) {
    printf("USAGE:\n\t thrput_test <size>\n");
    return -1;    
  }
  len = atoi(argv[1]);
  

  ZNSession *s = zn_open(PEER_MODE, 0, 0);
  if (s == 0) {
    printf("Error creating session!\n");
    exit(-1);
  } 
  sleep(1);
  char *data = (char*) malloc(len);
  memset(data, 1, len);
  printf("Running throughput test for %zu bytes payload.\n", len);
  const char *key = "/test/thr";
  size_t id = zn_declare_resource(s, key);  
  while (1) {
    zn_write_wrid(s, id, data, len);
  }
}