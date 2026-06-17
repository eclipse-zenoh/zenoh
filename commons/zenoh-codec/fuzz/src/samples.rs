//
// Copyright (c) 2026 ZettaScale Technology
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

use std::time::Duration;

use zenoh_buffers::ZSlice;
use zenoh_protocol::{
    common::ZExtBody,
    core::{Locator, Reliability, Resolution, WhatAmI},
    network::{
        declare::{common::DeclareFinal, subscriber::DeclareSubscriber},
        interest::{InterestMode, InterestOptions},
        request::ext::QueryTarget,
        Declare, Interest, NetworkBody, NetworkMessage, Oam as NetworkOam, Push as NetworkPush,
        Request, Response, ResponseFinal,
    },
    scouting::{HelloProto, Scout, ScoutingBody, ScoutingMessage},
    transport::{
        batch_size, close, fragment, frame, init, join, oam, Close, Fragment, Frame, InitAck,
        InitSyn, Join, Oam, OpenAck, OpenSyn, PrioritySn, TransportMessage,
    },
    zenoh::{PushBody, Put, Query, Reply, RequestBody, ResponseBody},
};

use crate::util::fixed_zid;

/// Returns one deterministic transport sample per transport-body family we fuzz.
pub(crate) fn sample_transport_message_seed_messages() -> Vec<(&'static str, TransportMessage)> {
    let network_message = sample_push_network_message();

    vec![
        (
            "close",
            Close {
                reason: close::reason::INVALID,
                session: true,
            }
            .into(),
        ),
        ("fragment", sample_fragment().into()),
        ("frame", sample_frame(network_message).into()),
        ("init_ack", sample_init_ack().into()),
        ("init_syn", sample_init_syn().into()),
        ("join", sample_join().into()),
        ("keep_alive", zenoh_protocol::transport::KeepAlive.into()),
        (
            "oam",
            zenoh_protocol::transport::TransportBody::OAM(sample_oam()).into(),
        ),
        ("open_ack", sample_open_ack().into()),
        ("open_syn", sample_open_syn().into()),
    ]
}

/// Builds a simple `Push(Put)` network message used by several transport seeds.
pub(crate) fn sample_push_network_message() -> NetworkMessage {
    let put = Put {
        payload: vec![0x11, 0x22, 0x33].into(),
        ..Put::default()
    };
    let push = NetworkPush::from(PushBody::from(put));
    NetworkBody::Push(push).into()
}

/// Builds a `Push(Del)` network message for delete-path coverage.
pub(crate) fn sample_push_del_network_message() -> NetworkMessage {
    let push = NetworkPush::from(PushBody::from(zenoh_protocol::zenoh::Del::default()));
    NetworkBody::Push(push).into()
}

/// Builds a deterministic request message with a default query payload.
pub(crate) fn sample_request_network_message() -> NetworkMessage {
    let request = Request {
        id: 0x0102_0304,
        wire_expr: "demo/request".into(),
        ext_qos: zenoh_protocol::network::request::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: zenoh_protocol::network::request::ext::NodeIdType::DEFAULT,
        ext_target: QueryTarget::DEFAULT,
        ext_budget: None,
        ext_timeout: None,
        payload: RequestBody::from(Query::default()),
    };
    NetworkBody::Request(request).into()
}

/// Builds a deterministic response message with a small put reply payload.
pub(crate) fn sample_response_network_message() -> NetworkMessage {
    let reply = Reply {
        consolidation: zenoh_protocol::zenoh::ConsolidationMode::DEFAULT,
        ext_unknown: Vec::new(),
        payload: PushBody::from(Put {
            payload: vec![0x44, 0x55].into(),
            ..Put::default()
        }),
    };
    let response = Response {
        rid: 0x1112_1314,
        wire_expr: "demo/response".into(),
        payload: ResponseBody::from(reply),
        ext_qos: zenoh_protocol::network::response::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_respid: None,
    };
    NetworkBody::Response(response).into()
}

/// Builds a deterministic terminal response message.
pub(crate) fn sample_response_final_network_message() -> NetworkMessage {
    let response_final = ResponseFinal {
        rid: 0x2122_2324,
        ext_qos: zenoh_protocol::network::response::ext::QoSType::DEFAULT,
        ext_tstamp: None,
    };
    NetworkBody::ResponseFinal(response_final).into()
}

/// Builds a deterministic interest message with default optional extensions.
pub(crate) fn sample_interest_network_message() -> NetworkMessage {
    let interest = Interest {
        id: 0x3132_3334,
        mode: InterestMode::Current,
        options: InterestOptions::KEYEXPRS,
        wire_expr: None,
        ext_qos: zenoh_protocol::network::interest::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: zenoh_protocol::network::interest::ext::NodeIdType::DEFAULT,
    };
    NetworkBody::Interest(interest).into()
}

/// Builds a deterministic subscriber declaration message.
pub(crate) fn sample_declare_network_message() -> NetworkMessage {
    let declare = Declare {
        interest_id: None,
        ext_qos: zenoh_protocol::network::declare::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: zenoh_protocol::network::declare::ext::NodeIdType::DEFAULT,
        body: zenoh_protocol::network::DeclareBody::DeclareSubscriber(DeclareSubscriber {
            id: 0x4142_4344,
            wire_expr: "demo/subscriber".into(),
        }),
    };
    NetworkBody::Declare(declare).into()
}

/// Builds a deterministic final declaration message.
pub(crate) fn sample_declare_final_network_message() -> NetworkMessage {
    let declare = Declare {
        interest_id: None,
        ext_qos: zenoh_protocol::network::declare::ext::QoSType::DEFAULT,
        ext_tstamp: None,
        ext_nodeid: zenoh_protocol::network::declare::ext::NodeIdType::DEFAULT,
        body: zenoh_protocol::network::DeclareBody::DeclareFinal(DeclareFinal),
    };
    NetworkBody::Declare(declare).into()
}

/// Builds a deterministic network-level OAM message.
pub(crate) fn sample_oam_network_message() -> NetworkMessage {
    let oam = NetworkOam {
        id: 9,
        body: ZExtBody::ZBuf(vec![0x99, 0x01].into()),
        ext_qos: zenoh_protocol::network::oam::ext::QoSType::DEFAULT,
        ext_tstamp: None,
    };
    NetworkBody::OAM(oam).into()
}

/// Builds a deterministic scout message for the scouting seed corpus.
pub(crate) fn sample_scout_scouting_message() -> ScoutingMessage {
    ScoutingBody::Scout(Scout {
        version: 1,
        what: WhatAmI::Peer.into(),
        zid: Some(fixed_zid()),
    })
    .into()
}

/// Builds a deterministic hello message with one locator.
pub(crate) fn sample_hello_scouting_message() -> ScoutingMessage {
    let hello = HelloProto {
        version: 1,
        whatami: WhatAmI::Peer,
        zid: fixed_zid(),
        locators: vec![Locator::new("tcp", "127.0.0.1:7447", "").expect("locator must be valid")],
    };
    ScoutingBody::Hello(hello).into()
}

/// Wraps one network message into a deterministic transport frame.
pub(crate) fn sample_frame(network_message: NetworkMessage) -> Frame {
    Frame {
        reliability: Reliability::Reliable,
        sn: 0x0102_0304,
        ext_qos: frame::ext::QoSType::DEFAULT,
        payload: vec![network_message],
    }
}

/// Builds a deterministic fragment payload for the transport seed corpus.
pub(crate) fn sample_fragment() -> Fragment {
    Fragment {
        reliability: Reliability::Reliable,
        more: false,
        sn: 0x0506_0708,
        payload: ZSlice::from(vec![0xaa, 0xbb, 0xcc]),
        ext_qos: fragment::ext::QoSType::DEFAULT,
        ext_first: None,
        ext_drop: None,
    }
}

/// Builds a minimal `InitSyn` without optional extensions.
pub(crate) fn sample_init_syn() -> InitSyn {
    InitSyn {
        version: 1,
        whatami: WhatAmI::Peer,
        zid: fixed_zid(),
        resolution: Resolution::default(),
        batch_size: batch_size::UNICAST,
        ext_qos: None,
        ext_qos_link: None,
        ext_auth: None,
        ext_mlink: None,
        ext_lowlatency: None,
        ext_compression: None,
        ext_patch: init::ext::PatchType::NONE,
        ext_region_name: None,
    }
}

/// Builds a minimal `InitAck` with a fixed cookie.
pub(crate) fn sample_init_ack() -> InitAck {
    InitAck {
        version: 1,
        whatami: WhatAmI::Peer,
        zid: fixed_zid(),
        resolution: Resolution::default(),
        batch_size: batch_size::UNICAST,
        cookie: vec![0xde, 0xad, 0xbe, 0xef].into(),
        ext_qos: None,
        ext_qos_link: None,
        ext_auth: None,
        ext_mlink: None,
        ext_lowlatency: None,
        ext_compression: None,
        ext_patch: init::ext::PatchType::NONE,
        ext_region_name: None,
    }
}

/// Builds a minimal `OpenSyn` with a fixed cookie.
pub(crate) fn sample_open_syn() -> OpenSyn {
    OpenSyn {
        lease: Duration::from_millis(1_500),
        initial_sn: 0x1112_1314,
        cookie: vec![0xca, 0xfe, 0xba, 0xbe].into(),
        ext_qos: None,
        ext_auth: None,
        ext_mlink: None,
        ext_lowlatency: None,
        ext_compression: None,
        ext_remote_bound: None,
    }
}

/// Builds a minimal `OpenAck` without optional extensions.
pub(crate) fn sample_open_ack() -> OpenAck {
    OpenAck {
        lease: Duration::from_secs(2),
        initial_sn: 0x2122_2324,
        ext_qos: None,
        ext_auth: None,
        ext_mlink: None,
        ext_lowlatency: None,
        ext_compression: None,
        ext_remote_bound: None,
    }
}

/// Builds a deterministic multicast `Join` sample.
pub(crate) fn sample_join() -> Join {
    Join {
        version: 1,
        whatami: WhatAmI::Peer,
        zid: fixed_zid(),
        resolution: Resolution::default(),
        batch_size: batch_size::MULTICAST,
        lease: Duration::from_secs(3),
        next_sn: PrioritySn::DEFAULT,
        ext_qos: None,
        ext_shm: None,
        ext_patch: join::ext::PatchType::NONE,
    }
}

/// Builds a deterministic transport-level OAM sample.
pub(crate) fn sample_oam() -> Oam {
    Oam {
        id: 7,
        body: ZExtBody::Z64(42),
        ext_qos: oam::ext::QoSType::DEFAULT,
    }
}
