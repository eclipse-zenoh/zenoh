use std::{
    mem::MaybeUninit,
    sync::{Arc, Mutex},
};

use crate::net::{
    primitives::EPrimitives,
    routing::dispatcher::{
        face::{Face, FaceState},
        tables::TablesLock,
    },
};

/// Add gateway between south and eastwest bounds.
///
/// This method should be called at construction of a [`crate::net::routing::router::Router`].
pub(crate) fn init(tables_lock: &Arc<TablesLock>) {
    let mut tables = tables_lock.tables.write().unwrap();

    let zid = tables.zid;
    let south_fid = tables.face_counter;
    let eastwest_fid = tables.face_counter + 1;
    tables.face_counter += 2;

    let eastwest = Arc::new(EastwestGateway {
        eastwest: Mutex::new(MaybeUninit::uninit()),
    });

    let south = Arc::new(SouthGateway {
        south: Mutex::new(MaybeUninit::uninit()),
    });

    let south_face = Face {
        tables: tables_lock.clone(),
        state: tables
            .faces
            .entry(south_fid)
            .or_insert_with(|| {
                FaceState::new(
                    south_fid,
                    zid,
                    todo!(),
                    true,
                    #[cfg(feature = "stats")]
                    None,
                    eastwest.clone(),
                    None,
                    None,
                    tables_lock.hat_code.south.new_face(),
                    true,
                )
            })
            .clone(),
    };

    let eastwest_face = Face {
        tables: tables_lock.clone(),
        state: tables
            .faces
            .entry(eastwest_fid)
            .or_insert_with(|| {
                FaceState::new(
                    eastwest_fid,
                    zid,
                    todo!(),
                    false,
                    #[cfg(feature = "stats")]
                    None,
                    south.clone(),
                    None,
                    None,
                    tables_lock.hat_code.eastwest.new_face(),
                    true,
                )
            })
            .clone(),
    };

    eastwest.eastwest.lock().unwrap().write(eastwest_face);
    south.south.lock().unwrap().write(south_face);
}

/// Gateway for messages arriving from the eastwest hat to the south hat.
pub(crate) struct SouthGateway {
    south: Mutex<MaybeUninit<Face>>,
}

impl EPrimitives for SouthGateway {
    fn send_interest(&self, ctx: super::RoutingContext<&mut zenoh_protocol::network::Interest>) {}

    fn send_declare(&self, ctx: super::RoutingContext<&mut zenoh_protocol::network::Declare>) {}

    fn send_push(
        &self,
        msg: &mut zenoh_protocol::network::Push,
        reliability: zenoh_protocol::core::Reliability,
    ) {
    }

    fn send_request(&self, msg: &mut zenoh_protocol::network::Request) {}

    fn send_response(&self, msg: &mut zenoh_protocol::network::Response) {}

    fn send_response_final(&self, msg: &mut zenoh_protocol::network::ResponseFinal) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Gateway for messages arriving from the south hat to the eastwest hat.
pub(crate) struct EastwestGateway {
    eastwest: Mutex<MaybeUninit<Face>>,
}

impl EPrimitives for EastwestGateway {
    fn send_interest(&self, ctx: super::RoutingContext<&mut zenoh_protocol::network::Interest>) {
        todo!()
    }

    fn send_declare(&self, ctx: super::RoutingContext<&mut zenoh_protocol::network::Declare>) {
        todo!()
    }

    fn send_push(
        &self,
        msg: &mut zenoh_protocol::network::Push,
        reliability: zenoh_protocol::core::Reliability,
    ) {
    }

    fn send_request(&self, msg: &mut zenoh_protocol::network::Request) {}

    fn send_response(&self, msg: &mut zenoh_protocol::network::Response) {}

    fn send_response_final(&self, msg: &mut zenoh_protocol::network::ResponseFinal) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
