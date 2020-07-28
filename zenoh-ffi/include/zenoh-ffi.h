#ifndef ZENOH_NET_FFI_
#define ZENOH_NET_FFI_

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include "zenoh-types.h"


typedef struct ZNProperties ZNProperties;

typedef struct ZNQuery ZNQuery;

typedef struct ZNQueryConsolidation ZNQueryConsolidation;

typedef struct ZNQueryTarget ZNQueryTarget;

typedef struct ZNQueryable ZNQueryable;

typedef struct ZNSession ZNSession;

typedef struct ZNSubscriber ZNSubscriber;

typedef struct zn_string {
  const char *val;
  unsigned int len;
} zn_string;

typedef struct zn_bytes {
  const unsigned char *val;
  unsigned int len;
} zn_bytes;

typedef struct zn_sample {
  zn_string key;
  zn_bytes value;
} zn_sample;

typedef struct zn_source_info {
  unsigned int kind;
  zn_bytes id;
} zn_source_info;

extern const unsigned int ALL_KINDS;

extern const int BROKER_MODE;

extern const int CLIENT_MODE;

extern const unsigned int EVAL;

extern const int PEER_MODE;

extern const int ROUTER_MODE;

extern const unsigned int STORAGE;

extern const unsigned int ZN_INFO_PEER_PID_KEY;

extern const unsigned int ZN_INFO_PID_KEY;

extern const unsigned int ZN_INFO_ROUTER_PID_KEY;

/**
 * Close a zenoh session
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_close(ZNSession *session);

/**
 * Notifies the zenoh runtime that there won't be any more replies sent for this
 * query.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_close_query(ZNQuery *query);

/**
 * Declares a zenoh queryable entity
 *
 * Returns the queryable entity or null if the creation was unsuccessful.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
ZNQueryable *zn_declare_queryable(ZNSession *session,
                                  const char *r_name,
                                  unsigned int kind,
                                  void (*callback)(ZNQuery*));

/**
 * Declare a zenoh resource
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
unsigned long zn_declare_resource(ZNSession *session, const char *r_name);

/**
 * Declare a zenoh resource with a suffix
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
unsigned long zn_declare_resource_ws(ZNSession *session, unsigned long rid, const char *suffix);

/**
 * Declares a zenoh subscriber
 *
 * Returns the created subscriber or null if the declaration failed.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
ZNSubscriber *zn_declare_subscriber(ZNSession *session,
                                    const char *r_name,
                                    void (*callback)(const zn_sample*));

/**
 * Return information on currently open session along with the the kind of entity for which the
 * session has been established.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
ZNProperties *zn_info(ZNSession *session);

/**
 * Open a zenoh session
 *
 * Returns the created session or null if the creation did not succeed
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
ZNSession *zn_open(int mode, const char *locator, const ZNProperties *_ps);

/**
 * Add a property
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
ZNProperties *zn_properties_add(ZNProperties *rps, unsigned long id, const char *value);

/**
 * Add a property
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_properties_free(ZNProperties *rps);

/**
 * Get the properties length
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
unsigned int zn_properties_len(ZNProperties *ps);

ZNProperties *zn_properties_make(void);

/**
 * Get the properties n-th property ID
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
unsigned int zn_property_id(ZNProperties *ps, unsigned int n);

/**
 * Get the properties n-th property value
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
const zn_bytes *zn_property_value(ZNProperties *ps, unsigned int n);

/**
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_query(ZNSession *session,
              const char *key_expr,
              const char *predicate,
              ZNQueryTarget *target,
              ZNQueryConsolidation *consolidation,
              void (*callback)(const zn_source_info*, const zn_sample*));

ZNQueryConsolidation *zn_query_consolidation_default(void);

ZNQueryConsolidation *zn_query_consolidation_incremental(void);

ZNQueryConsolidation *zn_query_consolidation_last_hop(void);

ZNQueryConsolidation *zn_query_consolidation_none(void);

/**
 * Return the predicate for this query
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
const zn_string *zn_query_predicate(ZNQuery *query);

/**
 * Return the resource name for this query
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
const zn_string *zn_query_res_name(ZNQuery *query);

ZNQueryTarget *zn_query_target_default(void);

/**
 * Sends a reply to a query.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_send_reply(ZNQuery *query, const char *key, const unsigned char *payload, unsigned int len);

/**
 * Un-declares a zenoh queryable
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_undeclare_queryable(ZNQueryable *sub);

/**
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_undeclare_subscriber(ZNSubscriber *sub);

/**
 * Writes a named resource.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
int zn_write(ZNSession *session, const char *r_name, const char *payload, unsigned int len);

/**
 * Writes a named resource using a resource id. This is the most wire efficient way of writing in zenoh.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
int zn_write_wrid(ZNSession *session,
                  unsigned long r_id,
                  const char *payload,
                  unsigned int len);

#endif /* ZENOH_NET_FFI_ */
