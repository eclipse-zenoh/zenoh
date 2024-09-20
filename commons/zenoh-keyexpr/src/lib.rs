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

//! ⚠️ WARNING ⚠️
//!
//! This crate is intended for Zenoh's internal use.
//!
//! [Click here for Zenoh's documentation](https://docs.rs/zenoh/latest/zenoh)
//!
//! [Key expression](https://github.com/eclipse-zenoh/roadmap/blob/main/rfcs/ALL/Key%20Expressions.md) are Zenoh's address space.
//!
//! In Zenoh, operations are performed on keys. To allow addressing multiple keys with a single operation, we use Key Expressions (KE).
//! KEs are a small language that express sets of keys through a glob-like language.
//!
//! These semantics can be a bit difficult to implement, so this module provides the following facilities:
//!
//! # Storing Key Expressions
//! This module provides 2 flavours to store strings that have been validated to respect the KE syntax, and a third is provided by [`zenoh`](https://docs.rs/zenoh):
//! - [`keyexpr`] is the equivalent of a [`str`],
//! - [`OwnedKeyExpr`] works like an [`Arc<str>`](std::sync::Arc),
//! - [`KeyExpr`](https://docs.rs/zenoh/latest/zenoh/key_expr/struct.KeyExpr.html) works like a [`Cow<str>`](std::borrow::Cow), but also stores some additional context internal to Zenoh to optimize
//!   routing and network usage.
//!
//! All of these types [`Deref`](core::ops::Deref) to [`keyexpr`], which notably has methods to check whether a given [`keyexpr::intersects`] with another,
//! or even if a [`keyexpr::includes`] another.
//!
//! # Tying values to Key Expressions
//! When storing values tied to Key Expressions, you might want something more specialized than a [`HashMap`](std::collections::HashMap) if you want to respect
//! the Key Expression semantics with high performance.
//!
//! Enter [KeTrees](keyexpr_tree). These are data-structures specially built to store KE-value pairs in a manner that supports the set-semantics of KEs.
//!
//! # Building and parsing Key Expressions
//! A common issue in REST API is the association of meaning to sections of the URL, and respecting that API in a convenient manner.
//! The same issue arises naturally when designing a KE space, and [`KeFormat`](format::KeFormat) was designed to help you with this,
//! both in constructing and in parsing KEs that fit the formats you've defined.
//!
//! [`kedefine`](https://docs.rs/zenoh/latest/zenoh/key_expr/format/macro.kedefine.html) also allows you to define formats at compile time, allowing a more performant, but more importantly safer and more convenient use of said formats,
//! as the [`keformat`](https://docs.rs/zenoh/latest/zenoh/key_expr/format/macro.keformat.html) and [`kewrite`](https://docs.rs/zenoh/latest/zenoh/key_expr/format/macro.kewrite.html) macros will be able to tell you if you're attempting to set fields of the format that do not exist.

#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod key_expr;

pub use key_expr::*;
pub mod keyexpr_tree;
