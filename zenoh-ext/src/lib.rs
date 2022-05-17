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
pub mod group;
mod publication_cache;
mod querying_subscriber;
mod session_ext;
pub use publication_cache::{PublicationCache, PublicationCacheBuilder};
pub use querying_subscriber::{
    CallbackQueryingSubscriber, CallbackQueryingSubscriberBuilder, QueryingSubscriberBuilder,
};
pub use session_ext::SessionExt;
pub mod compress {
    // pub struct Compress<T>(pub T);
    // impl<T: Into<zenoh::prelude::Value>> Into<zenoh::prelude::Value> for Compress<T> {
    //     fn into(self) -> zenoh::prelude::Value {
    //         let mut value = self.0.into();
    // 		value.;
    //         value
    //     }
    // }
}
