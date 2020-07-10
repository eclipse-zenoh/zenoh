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
use futures::prelude::*;
use futures::select;
use async_std::sync::{Arc, channel, Sender};
use async_std::task;
use zenoh::net::*;
use zenoh::net::Config;

#[no_mangle]
pub static BROKER_MODE : c_int = whatami::BROKER as c_int;
#[no_mangle]
pub static ROUTER_MODE : c_int = whatami::ROUTER as c_int;
#[no_mangle]
pub static PEER_MODE : c_int = whatami::PEER as c_int;
#[no_mangle]
pub static CLIENT_MODE : c_int = whatami::CLIENT as c_int;


pub struct ZNSession(zenoh::net::Session);

pub struct ZProperties(zenoh::net::Properties);

pub struct ZSubscriber(Option<Arc<Sender<bool>>>);


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
pub unsafe extern "C" fn zn_open(mode: c_int, locator: *const c_char, _ps: *const ZProperties) -> *mut ZNSession {  

  let s = task::block_on(async move {
    let c : Config = Default::default();    
    let config = if !locator.is_null() { 
      c
        .mode(mode as u64)
        .add_peer(CStr::from_ptr(locator).to_str().unwrap()) 
      } else { c.mode(mode as u64) };

    open(config, None).await
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

/// Declares a zenoh subscriber
/// 
/// # Safety
/// The main reason for this function to be unsafe is that it does casting of a pointer into a box.
/// 
#[no_mangle]
pub unsafe extern "C" 
fn zn_declare_subscriber(session: *mut ZNSession, r_name: *const c_char, 
                        callback: extern fn(*const c_char, c_uint, *const c_char, c_uint)) -> *mut ZSubscriber { 
    
  if session.is_null() || r_name.is_null()  { 
    return Box::into_raw(Box::new(ZSubscriber(None))) 
  }

  let si: SubInfo = Default::default();
  let s = Box::from_raw(session);  
  let name = CStr::from_ptr(r_name).to_str().unwrap();
  
  let (tx, rx) = channel::<bool>(1);  
  let r = ZSubscriber(Some(Arc::new(tx)));
  
  let mut sub : Subscriber = task::block_on(s.0.declare_subscriber(&ResKey::RName(name.to_string()), &si)).unwrap();
  // match task::block_on(s.0.declare_subscriber(&ResKey::RName(name.to_string()), &si)) {
    // Ok(mut sub) => {
      // Note: This is done to ensure that even if the call-back into C
      // does any blocking call we do not incour the risk of blocking
      // any of the task resolving futures.
      task::spawn_blocking(move || (task::block_on(async move {    
        loop {      
          select!(
            s = sub.next().fuse() => {            
              // This is a bit brutal but avoids an allocation and
              // a copy that would be otherwise required to add the 
              // C string terminator. See the test_sub.c to find out how to deal
              // with non null terminated strings.
              let sample = s.unwrap();
              let data = sample.payload.to_vec();
              callback(
                sample.res_name.as_ptr() as *const c_char, 
                sample.res_name.len() as c_uint,
                data.as_ptr() as *const c_char, 
                data.len() as c_uint);
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


#[no_mangle]
pub unsafe extern "C" fn zn_undeclare_subscriber(sub: *mut ZSubscriber) {
  match *Box::from_raw(sub) {
    ZSubscriber(Some(tx)) => task::block_on(tx.send(true)),
    ZSubscriber(None) => ()
  }  
}

