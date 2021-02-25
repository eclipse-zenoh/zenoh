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

#[macro_export]
macro_rules! dispatch_fn {
    () => {};
    (async fn $fn_name:ident(&self $(, $($p_name:ident: $p_type:ty),*)? $(,)? ) $(-> $r_type:ty)?; $($tail:tt)*) => {
        pub(crate) async fn $fn_name(&self $(, $($p_name: $p_type),*)? ) $(-> $r_type)? {
            dispatch!(self {this => this.$fn_name($($($p_name),*)? ).await})
        }
        dispatch_fn!($($tail)*);
    };
    (fn $fn_name:ident(&self $(, $($p_name:ident: $p_type:ty),*)? $(,)? ) $(-> $r_type:ty)?; $($tail:tt)*) => {
        pub(crate) fn $fn_name(&self $(, $($p_name: $p_type),*)? ) $(-> $r_type)? {
            dispatch!(self {this => this.$fn_name($($($p_name),*)? )})
        }
        dispatch_fn!($($tail)*);
    };
}

#[macro_export]
macro_rules! dispatcher {
    ($d:ident (
        $($(#[$meta:meta])*
        $impl_name:ident($impl_type:ty)),* $(,)?
    ) {
        $($body:tt)*
    }) => {
        #[allow(dead_code)]
        #[derive(Clone)]
        pub enum $d {
            $($(#[$meta])*
            $impl_name($impl_type)),*
        }

        macro_rules! dispatch {
            ($i:ident {$t:ident => $e:expr}) => (
                match $i {
                    $($(#[$meta])*
                    $d::$impl_name($t) => $e),*
                }
            )
        }

        #[allow(dead_code)]
        impl $d {
            dispatch_fn!($($body)*);
        }
    };
}
