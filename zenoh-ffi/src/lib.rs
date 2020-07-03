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
use libc::{c_char, c_ulong, c_uint, c_int};
use std::ffi::CStr;
use std::slice;
use async_std::task;
use zenoh::net;
use zenoh::net::Config;
use zenoh_protocol::core::{ResKey, ResourceId}; // { rname, PeerId, ResourceId, , ZError, ZErrorKind };

pub struct ZNSession(zenoh::net::Session, );

pub struct ZProperties(zenoh::net::Properties);

#[no_mangle]
pub extern "C" fn zn_properties_make() -> *mut ZProperties {
  Box::into_raw(Box::new(ZProperties(zenoh::net::Properties::new())))
}

/// Add a property
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_properties_add(rps: *mut ZProperties, id: c_ulong, value: *const c_char) -> *mut ZProperties {
  let mut ps = Box::from_raw(rps);  
  let bs = CStr::from_ptr(value).to_bytes();
  ps.0.insert(id as zenoh::net::ZInt, Vec::from(bs));
  rps
}

/// Add a property
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_properties_free(rps: *mut ZProperties ) {
  let ps = Box::from_raw(rps);  
  drop(ps);
}

/// Add a property
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_open(locator: *const c_char, _ps: *const ZProperties) -> *mut ZNSession {
  let s = task::block_on(async move {
    let config = Config::client();
    let config = if !locator.is_null() { config.add_peer(CStr::from_ptr(locator).to_str().unwrap()) } else { config };
    net::open(config, None).await
  }).unwrap();
  Box::into_raw(Box::new(ZNSession(s)))
}

/// Add a property
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

/// Add a property
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_declare_resource(session: *mut ZNSession, r_name: *const c_char) -> c_ulong {
  if r_name.is_null()  { return 0 };
  let s = Box::from_raw(session);
  let name = CStr::from_ptr(r_name).to_str().unwrap();
  let r = task::block_on(s.0.declare_resource(&ResKey::RName(name.to_string()))).unwrap() as c_ulong;
  Box::into_raw(s);
  r
}

/// Add a property
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_declare_resource_ws(session: *mut ZNSession, rid: c_ulong, suffix: *const c_char) -> c_ulong {
  if suffix.is_null()  { return 0 };
  let s = Box::from_raw(session);
  let sfx = CStr::from_ptr(suffix).to_str().unwrap();
  let r = task::block_on(s.0.declare_resource(&ResKey::RIdWithSuffix(rid as ResourceId, sfx.to_string()))).unwrap() as c_ulong;
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
  if r_name.is_null()  { return -1 };
  let s = Box::from_raw(session);
  let name = CStr::from_ptr(r_name).to_str().unwrap();
  let r = ResKey::RName(name.to_string());
  // let bs = ArcSlice::new(Arc::new(Vec::from(slice::from_raw_parts(payload as *const u8, len as usize))), 0, len as usize);    
  let r = match task::block_on(s.0.write(&r, slice::from_raw_parts(payload as *const u8, len as usize).into())) {
    Ok(()) => 0,
    _ => 1
  };
  let _ = Box::into_raw(s);
  r
  
}

/// Writes a named resource. 
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" fn zn_write_wrid(session: *mut ZNSession, r_id: c_ulong, payload: *const c_char, len: c_uint) -> c_int {
  let s = Box::from_raw(session);  
  let r = {
    #[cfg(unix)] { ResKey::RId(r_id) }
    #[cfg(windows)] { ResKey::RId(r_id.into()) }
  }; 
  let r = match task::block_on(s.0.write(&r, slice::from_raw_parts(payload as *const u8, len as usize).into())) {
    Ok(()) => 0,
    _ => 1
  };
  let _ = Box::into_raw(s);
  r

}

