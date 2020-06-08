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
  ZNSession *s = zn_open("", 0);
  if (s == 0) {
    printf("Error creating session!\n");
    exit(-1);
  } 
  sleep(1);
  const char *data = "Hello from C";
  const char *key = "/zenoh/demo/quote";
  zn_write(s, key, data, strlen(data));
  zn_close(s);
}