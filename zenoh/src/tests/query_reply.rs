use std::{
    any::Any,
    sync::{Arc, Mutex},
    time::Duration,
};

use zenoh_core::ztimeout;
use zenoh_protocol::{
    core::{Reliability, WireExpr, ZenohIdProto},
    network::{Declare, Interest, Mapping, Push, Request, Response, ResponseFinal},
};

use crate::{
    api::queryable::{Query, QueryInner, ReplyPrimitives},
    net::primitives::Primitives,
};

const TIMEOUT: Duration = Duration::from_secs(60);

struct ReplyTestPrimitives {
    wire_expr: Arc<Mutex<Option<WireExpr<'static>>>>,
}

impl ReplyTestPrimitives {
    fn new() -> Self {
        ReplyTestPrimitives {
            wire_expr: Arc::new(Mutex::new(None)),
        }
    }

    fn wire_expr(&self) -> Option<WireExpr<'_>> {
        self.wire_expr.lock().unwrap().clone()
    }
}

impl Primitives for ReplyTestPrimitives {
    fn send_interest(&self, _msg: &mut Interest) {}

    fn send_declare(&self, _msg: &mut Declare) {}

    fn send_push(&self, _msg: &mut Push, _reliability: Reliability) {}

    fn send_push_consume(&self, _msg: &mut Push, _reliability: Reliability, _consume: bool) {}

    fn send_request(&self, _msg: &mut Request) {}

    fn send_response(&self, msg: &mut Response) {
        let _ = self.wire_expr.lock().unwrap().insert(msg.wire_expr.clone());
    }

    fn send_response_final(&self, _msg: &mut ResponseFinal) {}

    fn send_close(&self) {}

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_reply_preserves_optimized_ke() {
    use crate::Config;

    let session = ztimeout!(crate::open(Config::default())).unwrap();

    let primitives = Arc::new(ReplyTestPrimitives::new());

    let query_inner = QueryInner {
        key_expr: "test/**".try_into().unwrap(),
        parameters: "".into(),
        qid: 1,
        zid: ZenohIdProto::default(),
        #[cfg(feature = "unstable")]
        source_info: None,
        primitives: ReplyPrimitives::new_remote(Some(session.downgrade()), primitives.clone()),
    };
    let query = Query {
        inner: Arc::new(query_inner),
        eid: 1,
        value: None,
        attachment: None,
    };

    let ke = "test/reply_declared_ke";
    let declared_ke = ztimeout!(session.declare_keyexpr(ke)).unwrap();
    ztimeout!(query.reply(declared_ke, "payload")).unwrap();

    let mut we = primitives.wire_expr().unwrap();
    assert!(we.suffix.is_empty());
    assert!(we.scope != 0);
    assert!(we.mapping == Mapping::Sender);

    ztimeout!(query.reply(ke, "payload")).unwrap();
    we = primitives.wire_expr().unwrap();
    assert_eq!(&we.suffix, &ke);
    assert!(we.scope == 0);
}
