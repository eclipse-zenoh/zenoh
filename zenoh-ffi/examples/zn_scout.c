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

void fprintpid(FILE *stream, const unsigned char *peerid, int len) {
    if (peerid == NULL) {
        fprintf(stream, "None");
    } else {
        fprintf(stream, "Some(");
        for (int i = 0; i < len; i++) {
          fprintf(stream, "%02X", (int)peerid[i]);
        }
        fprintf(stream, ")");
    }
}

void fprintwhatami(FILE *stream, unsigned int whatami) {
    if (whatami == ROUTER) { fprintf(stream, "\"Router\""); }
    else if (whatami == PEER) { fprintf(stream, "\"Peer\""); }
    else  { fprintf(stream, "\"Other\""); }
}

void fprintlocators(FILE *stream, ZNLocators *locs) {
    fprintf(stream, "[");
    for (int i = 0; i < zn_scout_locators_len(locs); i++) {
        fprintf(stream, "\"");
        fprintf(stream, "%s", zn_scout_locator_get(locs, i));
        fprintf(stream, "\"");
        if (i < zn_scout_locators_len(locs) - 1) {
            fprintf(stream, ", ");
        }
    }
    fprintf(stream, "]");
}

void fprinthello(FILE *stream, ZNScout *scout, unsigned int idx) {
    fprintf(stream, "Hello { pid: ");
    fprintpid(stream, zn_scout_peerid(scout, idx), zn_scout_peerid_len(scout, idx));
    fprintf(stream, ", whatami: ");
    fprintwhatami(stream, zn_scout_whatami(scout, idx));
    fprintf(stream, ", locators: ");
    ZNLocators *locs = zn_scout_locators(scout, idx);
    fprintlocators(stream, locs);
    zn_scout_locators_free(locs);
    fprintf(stream, " }");
}

int main(int argc, char** argv) {
  printf("Scouting...\n");
  ZNScout *scout = zn_scout(ROUTER | PEER, "auto", 1000);  
  if (zn_scout_len(scout) > 0) {
    for (unsigned int i = 0; i < zn_scout_len(scout); ++i) {
        fprinthello(stdout, scout, i);
        fprintf(stdout, "\n");
    }
  } else {
    printf("Did not find any zenoh process.\n");
  }
  zn_scout_free(scout);
  return 0;
}