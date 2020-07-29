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

typedef struct ZNScout ZNScout;

typedef struct ZNSession ZNSession;

typedef struct ZNSubInfo ZNSubInfo;

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

extern const unsigned int BROKER;

extern const unsigned int CLIENT;

extern const unsigned int EVAL;

extern const unsigned int PEER;

extern const unsigned int ROUTER;

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
                                    ZNSubInfo *sub_info,
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
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
ZNProperties *zn_properties_add(ZNProperties *ps, unsigned long id, const char *value);

/**
 * Add a property
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_properties_free(ZNProperties *ps);

/**
 * Get the properties length
 *
 * # Safety
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
unsigned int zn_properties_len(ZNProperties *ps);

ZNProperties *zn_properties_make(void);

/**
 * Get the properties n-th property ID
 *
 * # Safety
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
unsigned int zn_property_id(ZNProperties *ps, unsigned int n);

/**
 * Get the properties n-th property value
 *
 * # Safety
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
const zn_bytes *zn_property_value(ZNProperties *ps, unsigned int n);

/**
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_pull(ZNSubscriber *sub);

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
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
const zn_string *zn_query_predicate(ZNQuery *query);

/**
 * Return the resource name for this query
 *
 * # Safety
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
const zn_string *zn_query_res_name(ZNQuery *query);

ZNQueryTarget *zn_query_target_default(void);

/**
 * The scout mask allows to specify what to scout for.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
ZNScout *zn_scout(unsigned int what, const char *iface, unsigned long scout_period);

/**
 * Get the number of entities scouted  and available as part of
 * the ZNScout
 *
 * # Safety
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
unsigned int zn_scout_len(ZNScout *si);

/**
 * Get the peer-id for the scouted entity at the given index
 *
 * # Safety
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
const unsigned char *zn_scout_peerid(ZNScout *si, unsigned int idx);

/**
 * Get the whatami for the scouted entity at the given index
 *
 * # Safety
 * The main reason for this function to be unsafe is that it dereferences a pointer.
 *
 */
unsigned int zn_scout_whatami(ZNScout *si, unsigned int idx);

/**
 * Sends a reply to a query.
 *
 * # Safety
 * The main reason for this function to be unsafe is that it does casting of a pointer into a box.
 *
 */
void zn_send_reply(ZNQuery *query, const char *key, const unsigned char *payload, unsigned int len);

/**
 * Create the default subscriber info.
 *
 * This describes a reliable push subscriber without any negotiated
 * schedule. Starting from this default variants can be created.
 */
ZNSubInfo *zn_subinfo_default(void);

/**
 * Create a subscriber info for a pull subscriber
 *
 * This describes a reliable pull subscriber without any negotiated
 * schedule.
 */
ZNSubInfo *zn_subinfo_pull(void);

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
