//
// Copyright (c) 2017, 2020 ADLINK Technology Inc.
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ADLINK zenoh team, <zenoh@adlink-labs.tech>
//
#![recursion_limit = "256"]

use async_std::prelude::FutureExt;
use async_std::sync::{channel, Arc, Sender};
use async_std::task;
use futures::prelude::*;
use futures::select;
use libc::{c_char, c_int, c_uchar, c_uint, c_ulong};
use std::convert::TryFrom;
use std::ffi::{CStr, CString};
use std::slice;
use zenoh::net::Config;
use zenoh::net::*;
use zenoh_protocol::core::ZInt;
use zenoh_util::to_zint;

#[no_mangle]
pub static BROKER: c_uint = whatami::BROKER as c_uint;
#[no_mangle]
pub static ROUTER: c_uint = whatami::ROUTER as c_uint;
#[no_mangle]
pub static PEER: c_uint = whatami::PEER as c_uint;
#[no_mangle]
pub static CLIENT: c_uint = whatami::CLIENT as c_uint;

// Flags used in Queryable declaration and in queries
#[no_mangle]
pub static ALL_KINDS: c_uint = zenoh::net::queryable::ALL_KINDS as c_uint;
#[no_mangle]
pub static STORAGE: c_uint = zenoh::net::queryable::STORAGE as c_uint;
#[no_mangle]
pub static EVAL: c_uint = zenoh::net::queryable::EVAL as c_uint;

// Properties returned by zn_info()
#[no_mangle]
pub static ZN_INFO_PID_KEY: c_uint = 0x00 as c_uint;
#[no_mangle]
pub static ZN_INFO_PEER_PID_KEY: c_uint = 0x01 as c_uint;
#[no_mangle]
pub static ZN_INFO_ROUTER_PID_KEY: c_uint = 0x02 as c_uint;

pub struct ZNSession(zenoh::net::Session);

pub struct ZNProperties(zenoh::net::Properties);

enum ZnSubOps {
    Pull,
    Close,
}

pub struct ZNSubscriber(Option<Arc<Sender<ZnSubOps>>>);

pub struct ZNQueryTarget(zenoh::net::QueryTarget);

pub struct ZNQueryConsolidation(zenoh::net::QueryConsolidation);

pub struct ZNQueryable(Option<Arc<Sender<bool>>>);

pub struct ZNQuery(zenoh::net::Query);

pub struct ZNSubInfo(zenoh::net::SubInfo);

pub struct ZNScout(std::vec::Vec<Hello>);

pub struct ZNLocators(std::vec::Vec<std::ffi::CString>);

#[repr(C)]
pub struct zn_string {
    val: *const c_char,
    len: c_uint,
}

#[repr(C)]
pub struct zn_bytes {
    val: *const c_uchar,
    len: c_uint,
}

#[repr(C)]
pub struct zn_sample {
    key: zn_string,
    value: zn_bytes,
}

#[repr(C)]
pub struct zn_source_info {
    kind: c_uint,
    id: zn_bytes,
}

#[no_mangle]
pub extern "C" fn zn_query_target_default() -> *mut ZNQueryTarget {
    Box::into_raw(Box::new(ZNQueryTarget(QueryTarget::default())))
}

#[no_mangle]
pub extern "C" fn zn_query_consolidation_default() -> *mut ZNQueryConsolidation {
    Box::into_raw(Box::new(
        ZNQueryConsolidation(QueryConsolidation::default()),
    ))
}

#[no_mangle]
pub extern "C" fn zn_query_consolidation_none() -> *mut ZNQueryConsolidation {
    Box::into_raw(Box::new(ZNQueryConsolidation(QueryConsolidation::None)))
}

#[no_mangle]
pub extern "C" fn zn_query_consolidation_incremental() -> *mut ZNQueryConsolidation {
    Box::into_raw(Box::new(ZNQueryConsolidation(
        QueryConsolidation::Incremental,
    )))
}

#[no_mangle]
pub extern "C" fn zn_query_consolidation_last_hop() -> *mut ZNQueryConsolidation {
    Box::into_raw(Box::new(ZNQueryConsolidation(QueryConsolidation::LastHop)))
}

#[no_mangle]
pub extern "C" fn zn_properties_make() -> *mut ZNProperties {
    Box::into_raw(Box::new(ZNProperties(zenoh::net::Properties::new())))
}

/// Get the properties length
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_properties_len(ps: *mut ZNProperties) -> c_uint {
    (*ps).0.len() as c_uint
}

/// Get the properties n-th property ID
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_property_id(ps: *mut ZNProperties, n: c_uint) -> c_uint {
    (*ps).0[n as usize].0 as c_uint
}

/// Get the properties n-th property value
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_property_value(ps: *mut ZNProperties, n: c_uint) -> *const zn_bytes {
    let ptr = (*ps).0[n as usize].1.as_ptr();
    let value = Box::new(zn_bytes {
        val: ptr as *const c_uchar,
        len: (*ps).0[n as usize].1.len() as c_uint,
    });
    Box::into_raw(value)
}

/// Add a property
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_properties_add(
    ps: *mut ZNProperties,
    id: c_ulong,
    value: *const c_char,
) -> *mut ZNProperties {
    let bs = CStr::from_ptr(value).to_bytes();
    (*ps).0.push((to_zint!(id), Vec::from(bs)));
    ps
}

/// Add a property
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_properties_free(ps: *mut ZNProperties) {
    let bps = Box::from_raw(ps);
    drop(bps);
}

/// Return the resource name for this query
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_query_res_name(query: *mut ZNQuery) -> *const zn_string {
    let rn = zn_string {
        val: (*query).0.res_name.as_ptr() as *const c_char,
        len: (*query).0.res_name.len() as c_uint,
    };
    Box::into_raw(Box::new(rn))
}

/// Return the predicate for this query
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_query_predicate(query: *mut ZNQuery) -> *const zn_string {
    let pred = zn_string {
        val: (*query).0.predicate.as_ptr() as *const c_char,
        len: (*query).0.predicate.len() as c_uint,
    };
    Box::into_raw(Box::new(pred))
}

/// Create the default subscriber info.
///
/// This describes a reliable push subscriber without any negotiated
/// schedule. Starting from this default variants can be created.
#[no_mangle]
pub extern "C" fn zn_subinfo_default() -> *mut ZNSubInfo {
    Box::into_raw(Box::new(ZNSubInfo(SubInfo::default())))
}

/// Create a subscriber info for a pull subscriber
///
/// This describes a reliable pull subscriber without any negotiated
/// schedule.
#[no_mangle]
pub extern "C" fn zn_subinfo_pull() -> *mut ZNSubInfo {
    let si = SubInfo {
        reliability: Reliability::Reliable,
        mode: SubMode::Push,
        period: None,
    };
    Box::into_raw(Box::new(ZNSubInfo(si)))
}

/// Get the number of entities scouted  and available as part of
/// the ZNScout
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_scout_len(si: *mut ZNScout) -> c_uint {
    (*si).0.len() as c_uint
}

/// Get the whatami for the scouted entity at the given index
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_scout_whatami(si: *mut ZNScout, idx: c_uint) -> c_uint {
    match (*si).0[idx as usize].whatami {
        Some(w) => w as c_uint,
        None => ROUTER as c_uint,
    }
}

/// Get the peer-id for the scouted entity at the given index
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_scout_peerid(si: *mut ZNScout, idx: c_uint) -> *const c_uchar {
    match &(*si).0[idx as usize].pid {
        Some(v) => v.as_slice().as_ptr() as *const c_uchar,
        None => std::ptr::null(),
    }
}

/// Get the locators for the scouted.
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_scout_locators(si: *mut ZNScout, idx: c_uint) -> *mut ZNLocators {
    let mut vs = vec![];
    match &(*si).0[idx as usize].locators {
        Some(ls) => {
            for l in ls {
                vs.push(CString::new(format!("{}", l)).unwrap())
            }
        }
        None => (),
    }
    Box::into_raw(Box::new(ZNLocators(vs)))
}

/// Get the number of locators for the scouted entity.
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_scout_locators_len(ls: *mut ZNLocators) -> c_uint {
    (*ls).0.len() as c_uint
}

/// Get the locator at the given index.
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_scout_locator_get(ls: *mut ZNLocators, idx: c_uint) -> *const c_char {
    (*ls).0[idx as usize].as_ptr()
}

/// Frees the locators
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_scout_locators_free(ls: *mut ZNLocators) {
    drop(Box::from_raw(ls))
}

/// The scout mask allows to specify what to scout for.
///
/// # Safety
/// The main reason for this function to be unsafe is that it dereferences a pointer.
///
#[no_mangle]
pub unsafe extern "C" fn zn_scout(
    what: c_uint,
    iface: *const c_char,
    scout_period: c_ulong,
) -> *mut ZNScout {
    let w = what as ZInt;
    let i = CStr::from_ptr(iface).to_str().unwrap();

    let hellos = task::block_on(async move {
        let mut hs = std::vec::Vec::<Hello>::new();
        let mut stream = zenoh::net::scout(w, i).await;
        let scout = async {
            while let Some(hello) = stream.next().await {
                hs.push(hello)
            }
        };
        let timeout = async_std::task::sleep(std::time::Duration::from_millis(scout_period as u64));
        FutureExt::race(scout, timeout).await;
        hs
    });
    Box::into_raw(Box::new(ZNScout(hellos)))
}

/// Frees the ZNScout by releasing its associated memory.
///
/// # Safety
/// The main reason for this function to be unsafe is that it does of a pointer into a box.
#[no_mangle]
pub unsafe extern "C" fn zn_scout_free(s: *mut ZNScout) {
    drop(Box::from_raw(s))
}

/// Initialise the zenoh runtime logger
///
#[no_mangle]
pub extern "C" fn zn_init_logger() {
    env_logger::init();
}

/// Open a zenoh session
///
/// Returns the created session or null if the creation did not succeed
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_open(
    mode: c_int,
    locator: *const c_char,
    _ps: *const ZNProperties,
) -> *mut ZNSession {
    let s = task::block_on(async move {
        let c: Config = Default::default();
        let config = if !locator.is_null() {
            c.mode(to_zint!(mode))
                .add_peer(CStr::from_ptr(locator).to_str().unwrap())
        } else {
            c.mode(to_zint!(mode))
        };

        open(config, None).await
    });
    match s {
        Ok(v) => Box::into_raw(Box::new(ZNSession(v))),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Return information on currently open session along with the the kind of entity for which the
/// session has been established.
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_info(session: *mut ZNSession) -> *mut ZNProperties {
    let ps = task::block_on((*session).0.info());
    let bps = Box::new(ZNProperties(ps));
    Box::into_raw(bps)
}
/// Close a zenoh session
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_close(session: *mut ZNSession) {
    task::block_on((*Box::from_raw(session)).0.close()).unwrap();
}

/// Declare a zenoh resource
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_declare_resource(
    session: *mut ZNSession,
    r_name: *const c_char,
) -> c_ulong {
    if r_name.is_null() {
        return 0;
    }
    let name = CStr::from_ptr(r_name).to_str().unwrap();
    task::block_on(
        (*session)
            .0
            .declare_resource(&ResKey::RName(name.to_string())),
    )
    .unwrap() as c_ulong
}

/// Declare a zenoh resource with a suffix
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_declare_resource_ws(
    session: *mut ZNSession,
    rid: c_ulong,
    suffix: *const c_char,
) -> c_ulong {
    if suffix.is_null() {
        return 0;
    }
    let sfx = CStr::from_ptr(suffix).to_str().unwrap();
    task::block_on(
        (*session)
            .0
            .declare_resource(&ResKey::RIdWithSuffix(to_zint!(rid), sfx.to_string())),
    )
    .unwrap() as c_ulong
}

/// Writes a named resource.
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_write(
    session: *mut ZNSession,
    r_name: *const c_char,
    payload: *const c_char,
    len: c_uint,
) -> c_int {
    if r_name.is_null() {
        return -1;
    }

    let name = CStr::from_ptr(r_name).to_str().unwrap();
    let r = ResKey::RName(name.to_string());
    match task::block_on((*session).0.write(
        &r,
        slice::from_raw_parts(payload as *const u8, len as usize).into(),
    )) {
        Ok(()) => 0,
        _ => 1,
    }
}

/// Writes a named resource using a resource id. This is the most wire efficient way of writing in zenoh.
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_write_wrid(
    session: *mut ZNSession,
    r_id: c_ulong,
    payload: *const c_char,
    len: c_uint,
) -> c_int {
    let r = ResKey::RId(to_zint!(r_id));
    match smol::block_on((*session).0.write(
        &r,
        slice::from_raw_parts(payload as *const u8, len as usize).into(),
    )) {
        Ok(()) => 0,
        _ => 1,
    }
}

/// Declares a zenoh subscriber
///
/// Returns the created subscriber or null if the declaration failed.
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_declare_subscriber(
    session: *mut ZNSession,
    r_name: *const c_char,
    sub_info: *mut ZNSubInfo,
    callback: extern "C" fn(*const zn_sample),
) -> *mut ZNSubscriber {
    if session.is_null() || r_name.is_null() {
        return std::ptr::null_mut();
    }

    let si = Box::from_raw(sub_info);
    let name = CStr::from_ptr(r_name).to_str().unwrap();

    let (tx, rx) = channel::<ZnSubOps>(8);
    let rsub = ZNSubscriber(Some(Arc::new(tx)));
    let s = Box::from_raw(session);
    let mut sub: Subscriber = task::block_on(
        (*session)
            .0
            .declare_subscriber(&ResKey::RName(name.to_string()), &si.0),
    )
    .unwrap();
    // Note: This is done to ensure that even if the call-back into C
    // does any blocking call we do not incour the risk of blocking
    // any of the task resolving futures.
    task::spawn_blocking(move || {
        task::block_on(async move {
            let key = zn_string {
                val: std::ptr::null(),
                len: 0,
            };
            let value = zn_bytes {
                val: std::ptr::null(),
                len: 0,
            };
            let mut sample = zn_sample { key, value };

            loop {
                select!(
                    s = sub.stream().next().fuse() => {
                        // This is a bit brutal but avoids an allocation and
                        // a copy that would be otherwise required to add the
                        // C string terminator. See the test_sub.c to find out how to deal
                        // with non null terminated strings.
                        let us = s.unwrap();
                        let data = us.payload.to_vec();
                        sample.key.val = us.res_name.as_ptr() as *const c_char;
                        sample.key.len = us.res_name.len() as c_uint;
                        sample.value.val = data.as_ptr() as *const c_uchar;
                        sample.value.len = data.len() as c_uint;
                        callback(&sample)
                    },
                    op = rx.recv().fuse() => {
                        match op {
                            Ok(ZnSubOps::Pull) => {
                                let _ = sub.pull().await;
                                ()
                            },

                            Ok(ZnSubOps::Close) => {
                                let _ = (s).0.undeclare_subscriber(sub).await;
                                Box::into_raw(s);
                                return ()
                            },
                            _ => return ()
                        }
                    }
                )
            }
        })
    });
    Box::into_raw(Box::new(rsub))
}

// Un-declares a zenoh subscriber
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_pull(sub: *mut ZNSubscriber) {
    match *Box::from_raw(sub) {
        ZNSubscriber(Some(tx)) => smol::block_on(tx.send(ZnSubOps::Pull)),
        ZNSubscriber(None) => (),
    }
}

// Un-declares a zenoh subscriber
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_undeclare_subscriber(sub: *mut ZNSubscriber) {
    match *Box::from_raw(sub) {
        ZNSubscriber(Some(tx)) => smol::block_on(tx.send(ZnSubOps::Close)),
        ZNSubscriber(None) => (),
    }
}

// Issues a zenoh query
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_query(
    session: *mut ZNSession,
    key_expr: *const c_char,
    predicate: *const c_char,
    target: *mut ZNQueryTarget,
    consolidation: *mut ZNQueryConsolidation,
    callback: extern "C" fn(*const zn_source_info, *const zn_sample),
) {
    let s = Box::from_raw(session);
    let ke = CStr::from_ptr(key_expr).to_str().unwrap();
    let p = CStr::from_ptr(predicate).to_str().unwrap();
    let qt = Box::from_raw(target);
    let qc = Box::from_raw(consolidation);

    task::spawn_blocking(move || {
        task::block_on(async move {
            let mut q = s.0.query(&ke.into(), p, qt.0, qc.0).await.unwrap();
            let key = zn_string {
                val: std::ptr::null(),
                len: 0,
            };
            let value = zn_bytes {
                val: std::ptr::null(),
                len: 0,
            };
            let mut sample = zn_sample { key, value };
            let id = zn_bytes {
                val: std::ptr::null(),
                len: 0,
            };
            let mut source_info = zn_source_info { kind: 0, id };

            while let Some(reply) = q.next().await {
                source_info.kind = reply.source_kind as c_uint;
                source_info.id.val = reply.replier_id.as_slice().as_ptr() as *const c_uchar;
                source_info.id.len = reply.replier_id.as_slice().len() as c_uint;
                sample.key.val = reply.data.res_name.as_ptr() as *const c_char;
                sample.key.len = reply.data.res_name.len() as c_uint;
                let data = reply.data.payload.to_vec();
                sample.value.val = data.as_ptr() as *const c_uchar;
                sample.value.len = data.len() as c_uint;

                callback(&source_info, &sample)
            }
            let _ = Box::into_raw(s);
        })
    });
}

/// Declares a zenoh queryable entity
///
/// Returns the queryable entity or null if the creation was unsuccessful.
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_declare_queryable(
    session: *mut ZNSession,
    r_name: *const c_char,
    kind: c_uint,
    callback: extern "C" fn(*mut ZNQuery),
) -> *mut ZNQueryable {
    if session.is_null() || r_name.is_null() {
        return std::ptr::null_mut();
    }

    let s = Box::from_raw(session);
    let name = CStr::from_ptr(r_name).to_str().unwrap();

    let (tx, rx) = channel::<bool>(1);
    let r = ZNQueryable(Some(Arc::new(tx)));

    let mut queryable: zenoh::net::Queryable =
        task::block_on(s.0.declare_queryable(&ResKey::RName(name.to_string()), kind as ZInt))
            .unwrap();
    // Note: This is done to ensure that even if the call-back into C
    // does any blocking call we do not incour the risk of blocking
    // any of the task resolving futures.
    task::spawn_blocking(move || {
        task::block_on(async move {
            loop {
                select!(
                query = queryable.stream().next().fuse() => {
                  // This is a bit brutal but avoids an allocation and
                  // a copy that would be otherwise required to add the
                  // C string terminator. See the test_sub.c to find out how to deal
                  // with non null terminated strings.
                  let bquery = Box::new(ZNQuery(query.unwrap()));
                  let rbquery = Box::into_raw(bquery);
                  callback(rbquery);
                  Box::from_raw(rbquery);
                },
                _ = rx.recv().fuse() => {
                    let _ = s.0.undeclare_queryable(queryable).await;
                    Box::into_raw(s);
                    return ()
                })
            }
        })
    });
    Box::into_raw(Box::new(r))
}

/// Un-declares a zenoh queryable
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_undeclare_queryable(sub: *mut ZNQueryable) {
    match *Box::from_raw(sub) {
        ZNQueryable(Some(tx)) => smol::block_on(tx.send(true)),
        ZNQueryable(None) => (),
    }
}

/// Sends a reply to a query.
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_send_reply(
    query: *mut ZNQuery,
    key: *const c_char,
    payload: *const c_uchar,
    len: c_uint,
) {
    let name = CStr::from_ptr(key).to_str().unwrap();
    let s = Sample {
        res_name: name.to_string(),
        payload: slice::from_raw_parts(payload as *const u8, len as usize).into(),
        data_info: None,
    };
    task::block_on((*query).0.replies_sender.send(s));
}

/// Notifies the zenoh runtime that there won't be any more replies sent for this
/// query.
///
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
///
#[no_mangle]
pub unsafe extern "C" fn zn_close_query(query: *mut ZNQuery) {
    let bq = Box::from_raw(query);
    std::mem::drop(bq);
}
