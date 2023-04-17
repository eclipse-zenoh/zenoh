//
// Copyright (c) 2023 ZettaScale Technology
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
macro_rules! stats_struct {
    (
     $(#[$meta:meta])*
     $vis:vis struct $struct_name:ident {
        $(
        $(#[$field_meta:meta])*
        $field_vis:vis $field_name:ident,
        )*
     }
    ) => {
        paste::paste! {
            $(#[$meta])*
            $vis struct $struct_name {
                $(
                $(#[$field_meta:meta])*
                $field_vis $field_name: usize,
                )*
            }

            struct [<$struct_name Atomic>] {
                $(
                $(#[$field_meta:meta])*
                $field_name: AtomicUsize,
                )*
            }

            impl [<$struct_name Atomic>] {
                fn snapshot(&self) -> $struct_name {
                    $struct_name {
                        $($field_name: self.[<get_ $field_name>](),)*
                    }
                }

                $(
                fn [<get_ $field_name>](&self) -> usize {
                    self.$field_name.load(Ordering::Relaxed)
                }

                fn [<inc_ $field_name>](&self, nb: usize) {
                    self.$field_name.fetch_add(nb, Ordering::Relaxed);
                }
                )*
            }

            impl Default for [<$struct_name Atomic>] {
                fn default() -> [<$struct_name Atomic>] {
                    [<$struct_name Atomic>] {
                        $($field_name: AtomicUsize::new(0),)*
                    }
                }
            }
        }
    }
}
pub(crate) use stats_struct;
