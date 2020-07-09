#ifndef ZENOH_NET_FFI_
#define ZENOH_NET_FFI_

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include "zenoh-types.h"


typedef struct ZNSession ZNSession;

typedef struct ZProperties ZProperties;

extern const int BROKER_MODE;

extern const int CLIENT_MODE;

extern const int PEER_MODE;

extern const int ROUTER_MODE;

/**
 * Add a property
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_close(ZNSession *session);

/**
 * Add a property
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
unsigned long zn_declare_resource(ZNSession *session, const char *r_name);

/**
 * Add a property
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
unsigned long zn_declare_resource_ws(ZNSession *session, unsigned long rid, const char *suffix);

/**
 * Declares a zenoh subscriber
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
int zn_declare_subscriber(ZNSession *session,
                          const char *r_name,
                          void (*callback)(const char*, unsigned int, const char*, unsigned int));

/**
 * Add a property
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
ZNSession *zn_open(int mode, const char *locator, const ZProperties *_ps);

/**
 * Add a property
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
ZProperties *zn_properties_add(ZProperties *rps, unsigned long id, const char *value);

/**
 * Add a property
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_properties_free(ZProperties *rps);

ZProperties *zn_properties_make(void);

/**
 * Writes a named resource.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
int zn_write(ZNSession *session, const char *r_name, const char *payload, unsigned int len);

/**
 * Writes a named resource.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
int zn_write_wrid(ZNSession *session, unsigned long r_id, const char *payload, unsigned int len);

#endif /* ZENOH_NET_FFI_ */
