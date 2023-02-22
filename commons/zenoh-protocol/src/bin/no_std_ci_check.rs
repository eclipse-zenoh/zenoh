//
// Copyright (c) 2022 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
//

#![no_std]

#[cfg(not(feature = "std"))]
#[panic_handler]
fn dummy_panic_handler(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

fn dummy_get_rand(_: &mut [u8]) -> Result<(), getrandom::Error> {
    Ok(())
}

fn main() {
    getrandom::register_custom_getrandom!(dummy_get_rand);
}
