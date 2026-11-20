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

use super::*;

/// Explicit pending bounds override history depth; zero is rejected.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_max_pending_samples_builder() {
    let session = ztimeout!(zenoh::open(Config::default())).unwrap();
    let sub = ztimeout!(session
        .declare_subscriber("test/ext/mps/a")
        .advanced()
        .max_pending_samples(7))
    .unwrap();
    assert_eq!(zlock!(sub.statesref).max_pending_samples, 7);
    let sub2 = ztimeout!(session
        .declare_subscriber("test/ext/mps/b")
        .advanced()
        .history(HistoryConfig::default().max_samples(5)))
    .unwrap();
    assert_eq!(zlock!(sub2.statesref).max_pending_samples, 5);
    let sub3 = ztimeout!(session
        .declare_subscriber("test/ext/mps/c")
        .advanced()
        .history(HistoryConfig::default().max_samples(5))
        .max_pending_samples(9))
    .unwrap();
    assert_eq!(zlock!(sub3.statesref).max_pending_samples, 9);
    let sub4 = ztimeout!(session.declare_subscriber("test/ext/mps/d").advanced()).unwrap();
    assert_eq!(
        zlock!(sub4.statesref).max_pending_samples,
        DEFAULT_MAX_PENDING_SAMPLES
    );
    assert!(ztimeout!(session
        .declare_subscriber("test/ext/mps/z")
        .advanced()
        .max_pending_samples(0))
    .is_err());
    drop((sub, sub2, sub3, sub4));
    ztimeout!(session.close()).unwrap();
}

/// Only fragment zero carries the publisher timestamp and attachment; assembly
/// and cached replies preserve those metadata rules.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_fragment_timestamp_on_first_fragment() {
    let mut config = peer_config();
    config
        .timestamping
        .set_enabled(Some(ModeDependentValue::Unique(true)))
        .unwrap();
    let session = ztimeout!(zenoh::open(config)).unwrap();
    assert!(session.hlc().is_some());
    let sub = ztimeout!(session.declare_subscriber("test/ext/frag/ts").advanced()).unwrap();
    let publisher = ztimeout!(session
        .declare_publisher("test/ext/frag/ts")
        .advanced()
        .fragmentation(4)
        .cache(crate::CacheConfig::default().max_samples(10))
        .sample_miss_detection(crate::MissDetectionConfig::default()))
    .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    ztimeout!(publisher.put(PAYLOAD).attachment("attach")).unwrap();
    let delivered = ztimeout!(sub.recv_async()).unwrap();
    let timestamp = delivered.timestamp().copied();
    assert_eq!(
        delivered.attachment().unwrap().to_bytes().as_ref(),
        b"attach",
        "assembled sample must carry attachment"
    );
    let fragments = Arc::new(Mutex::new(Vec::<(u32, Option<Timestamp>, bool)>::new()));
    let _ = ztimeout!(session
        .get(Selector::from((
            KeyExpr::try_from("test/ext/frag/ts/@adv/**").unwrap(),
            "_sn=0..0".to_string()
        )))
        .callback({
            let fragments = fragments.clone();
            move |reply: Reply| {
                if let Ok(sample) = reply.into_result() {
                    if let Some(info) = sample.frag_info() {
                        fragments.lock().unwrap().push((
                            info.frag_num(),
                            sample.timestamp().copied(),
                            sample.attachment().is_some(),
                        ));
                    }
                }
            }
        })
        .consolidation(ConsolidationMode::None)
        .accept_replies(ReplyKeyExpr::Any)
        .target(QueryTarget::All)
        .timeout(Duration::from_secs(10)));
    {
        let mut fragments = fragments.lock().unwrap();
        fragments.sort_by_key(|(num, _, _)| *num);
        assert_eq!(fragments.len(), 3, "expected all 3 fragments from cache");
        let (_, first_timestamp, first_attachment) = fragments[0];
        assert_eq!(
            timestamp, first_timestamp,
            "assembled sample must carry fragment 0's timestamp"
        );
        assert!(first_timestamp.is_some(), "fragment 0 must be timestamped");
        assert!(first_attachment, "fragment 0 must carry attachment");
        assert!(
            fragments[1..]
                .iter()
                .all(|(_, ts, att)| ts.is_none() && !att),
            "other fragments must carry neither attachment nor publisher timestamp"
        );
    }
    drop((sub, publisher));
    ztimeout!(session.close()).unwrap();
}

/// Without SourceInfo a fragment is meaningless: do not create state or deliver it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_orphan_fragment_dropped() {
    let session = ztimeout!(zenoh::open(Config::default())).unwrap();
    let key = KeyExpr::try_from("test/ext/frag/orphan").unwrap();
    let received = Arc::new(Mutex::new(Vec::new()));
    let statesref = frag_recovery_state(&session, &key, received.clone());
    zlock!(statesref).retransmission = false;
    let orphan = SampleBuilder::put(key.clone(), "4567")
        .frag_info(FragInfo::new(3, 1))
        .into();
    {
        let mut states = zlock!(statesref);
        let inserted = handle_sample(&mut states, orphan);
        assert!(!inserted.new_fragment_slot);
        assert!(
            states.sequenced_states.is_empty() && states.timestamped_states.is_empty(),
            "orphan created source state"
        );
    }
    assert!(
        received.lock().unwrap().is_empty(),
        "orphan fragment delivered"
    );
    let publisher = session
        .declare_publisher("test/ext/frag/orphan/pub")
        .wait()
        .unwrap();
    let id = publisher.id();
    {
        let mut states = zlock!(statesref);
        let inserted = handle_sample(&mut states, fragment(&key, id, 0, 1, 3, "4567"));
        assert!(inserted.new_fragment_slot);
        assert_eq!(
            states
                .sequenced_states
                .get_mut(&id)
                .unwrap()
                .pending_samples
                .len(),
            1
        );
    }
    drop(statesref);
    ztimeout!(session.close()).unwrap();
}
