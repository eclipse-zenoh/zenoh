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
#![recursion_limit="256"]

use libc::{c_char, c_uchar, c_ulong, c_uint, c_int};
use std::ffi::CStr;
use std::convert::TryFrom;
use std::slice;
use futures::prelude::*;
use futures::select;
use async_std::sync::{Arc, channel, Sender};
use async_std::task;
use zenoh::net::*;
use zenoh::net::Config;
use zenoh_protocol::core::ZInt;
use zenoh_util::to_zint;

#[no_mangle]
pub static BROKER_MODE : c_int = whatami::BROKER as c_int;
#[no_mangle]
pub static ROUTER_MODE : c_int = whatami::ROUTER as c_int;
#[no_mangle]
pub static PEER_MODE : c_int = whatami::PEER as c_int;
#[no_mangle]
pub static CLIENT_MODE : c_int = whatami::CLIENT as c_int;

// Properties returned by zn_info()
#[no_mangle]
pub static ZN_INFO_PID_KEY: c_uint        = 0x00 as c_uint;
#[no_mangle]
pub static ZN_INFO_PEER_PID_KEY: c_uint   = 0x01 as c_uint;
#[no_mangle]
pub static ZN_INFO_ROUTER_PID_KEY: c_uint = 0x02 as c_uint;

pub struct ZNSession(zenoh::net::Session);

pub struct ZNProperties(zenoh::net::Properties);

pub struct ZNSubscriber(Option<Arc<Sender<bool>>>);

pub struct ZNQueryTarget(zenoh::net::QueryTarget);

pub struct ZNQueryConsolidation(zenoh::net::QueryConsolidation);



#[repr(C)]
pub struct zn_string {
    val: *const c_char,
    len: c_uint
}

#[repr(C)]
pub struct zn_bytes {
    val: *const c_uchar,
    len: c_uint
}

#[repr(C)]
pub struct zn_sample {
    key: zn_string,
    value: zn_bytes
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
    Box::into_raw(Box::new(ZNQueryConsolidation(QueryConsolidation::default())))
}

#[no_mangle]
pub extern "C" fn zn_query_consolidation_none() -> *mut ZNQueryConsolidation {
    Box::into_raw(Box::new(ZNQueryConsolidation(QueryConsolidation::None)))
}

#[no_mangle]
pub extern "C" fn zn_query_consolidation_incremental() -> *mut ZNQueryConsolidation {
    Box::into_raw(Box::new(ZNQueryConsolidation(QueryConsolidation::Incremental)))
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
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_properties_len(ps: *mut ZNProperties) -> c_uint {
  Box::from_raw(ps).0.len() as c_uint
}

/// Get the properties n-th property ID
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_property_id(ps: *mut ZNProperties, n: c_uint) -> c_uint {
  let bps = Box::from_raw(ps);    
  let r = bps.0[n as usize].0 as c_uint;
  Box::into_raw(bps);
  r

}

/// Get the properties n-th property value
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_property_value(ps: *mut ZNProperties, n: c_uint) ->  *const zn_bytes {
  let bps = Box::from_raw(ps);  
  let ptr = bps.0[n as usize].1.as_ptr();
  let value = Box::new(zn_bytes { val: ptr as *const c_uchar, len: bps.0[n as usize].1.len() as c_uint});
  Box::into_raw(bps);
  Box::into_raw(value)
}


/// Add a property
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_properties_add(rps: *mut ZNProperties, id: c_ulong, value: *const c_char) -> *mut ZNProperties {
    let mut ps = Box::from_raw(rps);  
    let bs = CStr::from_ptr(value).to_bytes();
    ps.0.push((to_zint!(id), Vec::from(bs)));
    Box::into_raw(ps);
    rps
}

/// Add a property
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_properties_free(rps: *mut ZNProperties ) {
    let ps = Box::from_raw(rps);  
    drop(ps);
}

/// Open a zenoh session
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_open(mode: c_int, locator: *const c_char, _ps: *const ZNProperties) -> *mut ZNSession {  
    let s = task::block_on(async move {
        let c : Config = Default::default();    
        let config = if !locator.is_null() { 
            c.mode(to_zint!(mode))
                .add_peer(CStr::from_ptr(locator).to_str().unwrap()) 
        } else { 
            c.mode(to_zint!(mode))
        };

        open(config, None).await
    }).unwrap();
    Box::into_raw(Box::new(ZNSession(s)))
}

/// Return information on currently open session along with the the kind of entity for which the 
/// session has been established.
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_info(session: *mut ZNSession)-> *mut ZNProperties {  
    let s = Box::from_raw(session);
    let ps = task::block_on(s.0.info());
    let bps = Box::new(ZNProperties(ps));
    Box::into_raw(s);
    Box::into_raw(bps)
}
/// Close a zenoh session
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_close(session: *mut ZNSession) {
    let s = Box::from_raw(session);
    task::block_on(s.0.close()).unwrap();
    let _ = Box::into_raw(s);
}

/// Declare a zenoh resource
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_declare_resource(session: *mut ZNSession, r_name: *const c_char) -> c_ulong {
    if r_name.is_null() { return 0 };
    let s = Box::from_raw(session);
    let name = CStr::from_ptr(r_name).to_str().unwrap();
    let r = task::block_on(s.0.declare_resource(&ResKey::RName(name.to_string()))).unwrap() as c_ulong;
    Box::into_raw(s);
    r
}

/// Declare a zenoh resource with a suffix
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_declare_resource_ws(session: *mut ZNSession, rid: c_ulong, suffix: *const c_char) -> c_ulong {
    if suffix.is_null()  { return 0 };
    let s = Box::from_raw(session);
    let sfx = CStr::from_ptr(suffix).to_str().unwrap();
    let r = task::block_on(s.0.declare_resource(&ResKey::RIdWithSuffix(to_zint!(rid), sfx.to_string()))).unwrap() as c_ulong;
    let _ = Box::into_raw(s);  
    r
}


/// Writes a named resource. 
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_write(session: *mut ZNSession, r_name: *const c_char, payload: *const c_char, len: c_uint) -> c_int {
    if r_name.is_null() { return -1 };
    let s = Box::from_raw(session);
    let name = CStr::from_ptr(r_name).to_str().unwrap();
    let r = ResKey::RName(name.to_string());
    let r = match task::block_on(s.0.write(&r, slice::from_raw_parts(payload as *const u8, len as usize).into())) {
        Ok(()) => 0,
        _ => 1
    };
    let _ = Box::into_raw(s);
    r
    
}

/// Writes a named resource using a resource id. This is the most wire efficient way of writing in zenoh. 
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_write_wrid(session: *mut ZNSession, r_id: c_ulong, payload: *const c_char, len: c_uint) -> c_int {
    let s = Box::from_raw(session);
    let r = {
        #[cfg(unix)] { ResKey::RId(to_zint!(r_id)) }
        #[cfg(windows)] { ResKey::RId(to_zint!(r_id)) }
    }; 
    let r = match smol::block_on(s.0.write(&r, slice::from_raw_parts(payload as *const u8, len as usize).into())) {
        Ok(()) => 0,
        _ => 1
    };
    let _ = Box::into_raw(s);
    r

}

/// Declares a zenoh subscriber
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" 
fn zn_declare_subscriber(session: *mut ZNSession, r_name: *const c_char, 
    callback: extern fn(*const zn_sample)) -> *mut ZNSubscriber { 

    if session.is_null() || r_name.is_null() { 
        return Box::into_raw(Box::new(ZNSubscriber(None))) 
    }

    let si: SubInfo = Default::default();
    let s = Box::from_raw(session);
    let name = CStr::from_ptr(r_name).to_str().unwrap();
    
    let (tx, rx) = channel::<bool>(1);
    let r = ZNSubscriber(Some(Arc::new(tx)));
    
    let mut sub : Subscriber = task::block_on(s.0.declare_subscriber(&ResKey::RName(name.to_string()), &si)).unwrap();
    // Note: This is done to ensure that even if the call-back into C
    // does any blocking call we do not incour the risk of blocking
    // any of the task resolving futures.
    task::spawn_blocking(move || (task::block_on(async move {
        let key = zn_string { val: std::ptr::null(), len: 0};
        let value = zn_bytes { val: std::ptr::null(), len: 0};
        let mut sample = zn_sample {key, value};

        loop {
            select!(
                s = sub.next().fuse() => {
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
                _ = rx.recv().fuse() => {
                        print!("Notification thread undeclaring sub!\n");
                        let _ = s.0.undeclare_subscriber(sub).await;
                        return ()
                }
            )
    } } )));
    Box::into_raw(Box::new(r)) 
    }


// Un-declares a zenoh subscriber
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_undeclare_subscriber(sub: *mut ZNSubscriber) {
    match *Box::from_raw(sub) {
        ZNSubscriber(Some(tx)) => smol::block_on(tx.send(true)),
        ZNSubscriber(None) => ()
    }
}

// Issues a zenoh query
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_query(session: *mut ZNSession, key_expr: *const c_char, predicate: *const c_char, 
    target: *mut ZNQueryTarget, consolidation: *mut ZNQueryConsolidation,
    callback: extern fn(*const zn_source_info, *const zn_sample)) {
    
    let s = Box::from_raw(session);
    let ke = CStr::from_ptr(key_expr).to_str().unwrap();
    let p = CStr::from_ptr(predicate).to_str().unwrap();
    let qt = Box::from_raw(target);
    let qc = Box::from_raw(consolidation);
    
    task::spawn_blocking(move || task::block_on(async move {
        let mut q = s.0.query(&ke.into(), p, qt.0, qc.0).await.unwrap();
        let key = zn_string { val: std::ptr::null(), len: 0};
        let value = zn_bytes { val: std::ptr::null(), len: 0};
        let mut sample = zn_sample {key, value};    
        let id = zn_bytes { val: std::ptr::null(), len: 0};
        let mut source_info = zn_source_info {kind: 0, id};
        
        while let Some(reply) = q.next().await {
            source_info.kind = reply.source_kind as c_uint;
            source_info.id.val = reply.replier_id.id.as_ptr() as *const c_uchar;
            source_info.id.len = reply.replier_id.id.len() as c_uint;
            sample.key.val = reply.data.res_name.as_ptr() as *const c_char;
            sample.key.len = reply.data.res_name.len() as c_uint;
            let data = reply.data.payload.to_vec();
            sample.value.val = data.as_ptr() as *const c_uchar;
            sample.value.len = data.len() as c_uint;
         
            callback(&source_info, &sample)
        }
    }));
}