//
// Copyright (c) 2025 ZettaScale Technology
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

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, RwLock},
};

use tracing::trace;
use zenoh_keyexpr::{keyexpr, OwnedNonWildKeyExpr};
use zenoh_protocol::{
    core::{WireExpr, EMPTY_EXPR_ID},
    network::{
        interest::InterestMode, DeclareBody, Mapping, Push, Request, Response, ResponseFinal,
    },
};

use super::dispatcher::face::Face;
use crate::net::primitives::{EPrimitives, Primitives};

pub(crate) struct Namespace {
    namespace: OwnedNonWildKeyExpr,
    primitives: Arc<Face>,
}

impl Namespace {
    pub(crate) fn new(namespace: OwnedNonWildKeyExpr, primitives: Arc<Face>) -> Self {
        Namespace {
            namespace,
            primitives,
        }
    }

    fn handle_namespace_egress(&self, key_expr: &mut WireExpr, new_keyexpr_declare: bool) {
        if key_expr.scope == EMPTY_EXPR_ID || new_keyexpr_declare {
            // non - optimized ke
            let key = key_expr.suffix.as_ref();
            key_expr.suffix = std::borrow::Cow::Owned(match key.is_empty() {
                true => self.namespace.as_str().to_owned(), // a case where only a namespace was declared
                false => self.namespace.as_str().to_owned() + "/" + key,
            });
        }
        // already optimized ke, given that all of the ke declarations pass through this functions
        // it should already account for namespace prefix, and thus no extra processing is needed
    }

    fn handle_declare_egress(&self, msg: &mut zenoh_protocol::network::Declare) {
        match &mut msg.body {
            DeclareBody::DeclareKeyExpr(m) => {
                self.handle_namespace_egress(&mut m.wire_expr, true);
            }
            DeclareBody::UndeclareKeyExpr(_) => {}
            DeclareBody::DeclareSubscriber(m) => {
                self.handle_namespace_egress(&mut m.wire_expr, false);
            }
            DeclareBody::UndeclareSubscriber(_) => {}
            DeclareBody::DeclareQueryable(m) => {
                self.handle_namespace_egress(&mut m.wire_expr, false);
            }
            DeclareBody::UndeclareQueryable(m) => {
                self.handle_namespace_egress(&mut m.ext_wire_expr.wire_expr, false);
            }
            DeclareBody::DeclareToken(m) => {
                self.handle_namespace_egress(&mut m.wire_expr, false);
            }
            DeclareBody::UndeclareToken(_) => {}
            DeclareBody::DeclareFinal(_) => {}
        }
    }
}

impl Primitives for Namespace {
    fn send_interest(&self, msg: &mut zenoh_protocol::network::Interest) {
        if let Some(w) = &mut msg.wire_expr {
            self.handle_namespace_egress(w, false);
        }
        self.primitives.send_interest(msg);
    }

    fn send_declare(&self, msg: &mut zenoh_protocol::network::Declare) {
        self.handle_declare_egress(msg);
        self.primitives.send_declare(msg);
    }

    fn send_push(&self, msg: &mut Push, reliability: zenoh_protocol::core::Reliability) {
        self.handle_namespace_egress(&mut msg.wire_expr, false);
        self.primitives.send_push(msg, reliability);
    }

    fn send_request(&self, msg: &mut Request) {
        self.handle_namespace_egress(&mut msg.wire_expr, false);
        self.primitives.send_request(msg);
    }

    fn send_response(&self, msg: &mut Response) {
        self.handle_namespace_egress(&mut msg.wire_expr, false);
        self.primitives.send_response(msg);
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        self.primitives.send_response_final(msg);
    }

    fn send_close(&self) {
        self.primitives.send_close();
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(crate) struct ENamespace {
    namespace: OwnedNonWildKeyExpr,
    primitives: Arc<dyn EPrimitives + Send + Send>,
    incomplete_ingress_keyexpr_declarations: RwLock<HashMap<u16, String>>,
    blocked_subscribers: RwLock<HashSet<u32>>,
    blocked_queryables: RwLock<HashSet<u32>>,
    blocked_tokens: RwLock<HashSet<u32>>,
    blocked_interests: RwLock<HashSet<u32>>,
}

impl ENamespace {
    pub(crate) fn new(
        namespace: OwnedNonWildKeyExpr,
        primitives: Arc<dyn EPrimitives + Send + Sync>,
    ) -> Self {
        ENamespace {
            namespace,
            primitives,
            incomplete_ingress_keyexpr_declarations: HashMap::new().into(),
            blocked_subscribers: HashSet::new().into(),
            blocked_queryables: HashSet::new().into(),
            blocked_tokens: HashSet::new().into(),
            blocked_interests: HashSet::new().into(),
        }
    }

    fn handle_namespace_ingress(&self, key_expr: &mut WireExpr, message_id: Option<u16>) -> bool {
        if key_expr.scope != EMPTY_EXPR_ID && key_expr.mapping == Mapping::Receiver {
            return true;
        }
        if key_expr.scope != EMPTY_EXPR_ID {
            // mapping sender
            // optimized ke
            match zread!(self.incomplete_ingress_keyexpr_declarations).get(&key_expr.scope) {
                Some(head) => {
                    // if it references an incomplete keyexpr, we concatenate them and verify again as fully non-optimized ke
                    if key_expr.suffix.is_empty() {
                        return false;
                    }
                    key_expr.scope = EMPTY_EXPR_ID;
                    key_expr.suffix = (head.clone() + key_expr.suffix.as_ref()).into();
                    return self.handle_namespace_ingress(key_expr, None);
                }
                None => return true,
            }
        }
        let key = key_expr.suffix.as_ref();
        let ke = unsafe { keyexpr::from_str_unchecked(key) };
        if let Some(tail) = ke.strip_nonwild_prefix(&self.namespace) {
            key_expr.suffix = tail.as_str().to_owned().into();

            true
        } else if let Some(id) = message_id {
            if key_expr.mapping == Mapping::Sender {
                // ke does not match namespace - but this can be a partial declaration
                // we store this ke for checking future keyexprs, referencing it
                zwrite!(self.incomplete_ingress_keyexpr_declarations)
                    .insert(id, key_expr.suffix.as_ref().to_string());
            }
            false
        } else {
            trace!("Rejecting message containing wire expression `{}`, since it does not match namespace `{}`", &key_expr, self.namespace.as_ref());
            false
        }
    }

    fn handle_declare_ingress(&self, msg: &mut zenoh_protocol::network::Declare) -> bool {
        match &mut msg.body {
            DeclareBody::DeclareKeyExpr(m) => {
                self.handle_namespace_ingress(&mut m.wire_expr, Some(m.id))
            }
            DeclareBody::UndeclareKeyExpr(m) => {
                zwrite!(self.incomplete_ingress_keyexpr_declarations)
                    .remove(&m.id)
                    .is_none()
            }
            DeclareBody::DeclareSubscriber(m) => {
                if !self.handle_namespace_ingress(&mut m.wire_expr, None) {
                    zwrite!(self.blocked_subscribers).insert(m.id);
                    return false;
                }
                true
            }
            DeclareBody::UndeclareSubscriber(m) => !zwrite!(self.blocked_subscribers).remove(&m.id),
            DeclareBody::DeclareQueryable(m) => {
                if !self.handle_namespace_ingress(&mut m.wire_expr, None) {
                    zwrite!(self.blocked_queryables).insert(m.id);
                    return false;
                }
                true
            }
            DeclareBody::UndeclareQueryable(m) => !zwrite!(self.blocked_queryables).remove(&m.id),
            DeclareBody::DeclareToken(m) => {
                if !self.handle_namespace_ingress(&mut m.wire_expr, None) {
                    zwrite!(self.blocked_tokens).insert(m.id);
                    return false;
                }
                true
            }
            DeclareBody::UndeclareToken(m) => !zwrite!(self.blocked_tokens).remove(&m.id),
            DeclareBody::DeclareFinal(_) => true,
        }
    }

    fn handle_interest_ingress(&self, msg: &mut zenoh_protocol::network::Interest) -> bool {
        match msg.mode {
            InterestMode::Final => !zwrite!(self.blocked_interests).remove(&msg.id),
            _ => match &mut msg.wire_expr {
                Some(wire_expr) => {
                    if !self.handle_namespace_ingress(wire_expr, None) {
                        zwrite!(self.blocked_interests).insert(msg.id);
                        return false;
                    }
                    true
                }
                None => true,
            },
        }
    }
}

impl EPrimitives for ENamespace {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn send_interest(&self, ctx: super::RoutingContext<&mut zenoh_protocol::network::Interest>) {
        if self.handle_interest_ingress(ctx.msg) {
            self.primitives.send_interest(ctx);
        }
    }

    fn send_declare(&self, ctx: super::RoutingContext<&mut zenoh_protocol::network::Declare>) {
        if self.handle_declare_ingress(ctx.msg) {
            self.primitives.send_declare(ctx);
        }
    }

    fn send_push(&self, msg: &mut Push, reliability: zenoh_protocol::core::Reliability) {
        if self.handle_namespace_ingress(&mut msg.wire_expr, None) {
            self.primitives.send_push(msg, reliability);
        }
    }

    fn send_request(&self, msg: &mut Request) {
        if self.handle_namespace_ingress(&mut msg.wire_expr, None) {
            self.primitives.send_request(msg);
        }
    }

    fn send_response(&self, msg: &mut Response) {
        if self.handle_namespace_ingress(&mut msg.wire_expr, None) {
            self.primitives.send_response(msg);
        }
    }

    fn send_response_final(&self, msg: &mut ResponseFinal) {
        self.primitives.send_response_final(msg);
    }
}
