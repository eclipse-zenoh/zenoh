// SPDX-License-Identifier: (EPL-2.0 OR Apache-2.0) AND CC0-1.0
//
// AI Disclosure: Generated with assistance from OpenAI Codex.
// AI-generated portions are made available under CC0-1.0.
// Assisted-by: OpenAI Codex

use super::*;
use crate::common::shm::handoff::PriorityContainer;

async fn exchange_init(alice: &ShmFsm<'_>, bob: &ShmFsm<'_>) {
    let syn = alice.send_init_syn(&StateOpen::new()).await.unwrap();
    bob.recv_init_syn(syn).await.unwrap();
    let ack = bob.send_init_ack(()).await.unwrap();
    alice.recv_init_ack(ack).await.unwrap();
}

fn assert_disabled(fsm: ShmFsm<'_>) {
    let (transport, link) = fsm.shm_init_result();
    assert!(transport.is_none(), "incomplete negotiation enabled SHM");
    assert!(matches!(link.rx, RxHandoffChannel::Disabled));
    assert!(matches!(link.tx.ids(), HandoffCounterIds::Disabled));
}

fn ack_with_counters(bob_counters: HandoffCounterIds) -> open::ext::Shm {
    let mut bytes = vec![];
    Zenoh080
        .write(&mut bytes.writer(), &OpenAck { bob_counters })
        .unwrap();
    open::ext::Shm::new(bytes.into())
}

#[tokio::test]
async fn completed_handshake_enables_shm_for_both_reliabilities() {
    for reliability in [Reliability::Reliable, Reliability::BestEffort] {
        let alice_auth = AuthUnicast::new(&[]).unwrap();
        let bob_auth = AuthUnicast::new(&[]).unwrap();
        let alice = ShmFsm::new(&alice_auth);
        let bob = ShmFsm::new(&bob_auth);
        exchange_init(&alice, &bob).await;
        let syn = alice.send_open_syn(reliability).await.unwrap();
        assert!(syn.is_some());
        bob.recv_open_syn(syn).await.unwrap();
        let ack = bob.send_open_ack(reliability).await.unwrap();
        assert!(ack.is_some());
        alice.recv_open_ack(ack).await.unwrap();

        for fsm in [alice, bob] {
            let (transport, link) = fsm.shm_init_result();
            assert!(transport.is_some());
            // Best-effort links use SHM without reliable handoff counters.
            let disabled = reliability == Reliability::BestEffort;
            assert_eq!(matches!(link.rx, RxHandoffChannel::Disabled), disabled);
            assert_eq!(
                matches!(link.tx.ids(), HandoffCounterIds::Disabled),
                disabled
            );
        }
    }
}

#[tokio::test]
async fn one_sided_auth_segment_open_failure_disables_both_peers() {
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    let syn = alice.send_init_syn(&StateOpen::new()).await.unwrap();
    bob.recv_init_syn(syn).await.unwrap();
    let mut ack = bob.send_init_ack(()).await.unwrap().unwrap();
    let mut decoded: InitAck = Zenoh080.read(&mut ack.value.reader()).unwrap();

    // Advertise a segment whose last owner has already gone away. This forces
    // the same open failure as #2863 without renaming another test's live SHM.
    let removed = AuthUnicast::new(&[]).unwrap();
    decoded.bob_segment = removed.segment_id();
    drop(removed);
    assert!(RXAuthSegment::open(decoded.bob_segment).is_err());
    let mut bytes = vec![];
    Zenoh080.write(&mut bytes.writer(), &decoded).unwrap();
    ack.value = bytes.into();
    alice.recv_init_ack(Some(ack)).await.unwrap();

    let syn = alice.send_open_syn(Reliability::Reliable).await.unwrap();
    assert!(syn.is_none());
    bob.recv_open_syn(syn).await.unwrap();
    let ack = bob.send_open_ack(Reliability::Reliable).await.unwrap();
    assert!(
        ack.is_none(),
        "acceptor acknowledged an absent SHM proposal"
    );
    alice.recv_open_ack(ack).await.unwrap();
    assert_disabled(alice);
    assert_disabled(bob);
}

#[tokio::test]
async fn absent_open_ack_disables_initiator_shm() {
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init(&alice, &bob).await;
    assert!(alice
        .send_open_syn(Reliability::Reliable)
        .await
        .unwrap()
        .is_some());
    alice.recv_open_ack(None).await.unwrap();
    assert_disabled(alice);
}

#[tokio::test]
async fn rejected_open_syn_does_not_advertise_shm() {
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init(&alice, &bob).await;
    let mut syn = alice
        .send_open_syn(Reliability::Reliable)
        .await
        .unwrap()
        .unwrap();
    let mut decoded: OpenSyn = Zenoh080.read(&mut syn.value.reader()).unwrap();
    decoded.bob_challenge ^= 1;
    let mut bytes = vec![];
    Zenoh080.write(&mut bytes.writer(), &decoded).unwrap();
    syn.value = bytes.into();
    bob.recv_open_syn(Some(syn)).await.unwrap();
    let ack = bob.send_open_ack(Reliability::Reliable).await.unwrap();
    assert!(
        ack.is_none(),
        "acceptor acknowledged a rejected SHM proposal"
    );
    alice.recv_open_ack(ack).await.unwrap();
    assert_disabled(alice);
    assert_disabled(bob);
}

#[tokio::test]
async fn malformed_open_ack_rejects_establishment() {
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init(&alice, &bob).await;
    let syn = alice.send_open_syn(Reliability::Reliable).await.unwrap();
    bob.recv_open_syn(syn).await.unwrap();
    let mut ack = bob
        .send_open_ack(Reliability::Reliable)
        .await
        .unwrap()
        .unwrap();
    ack.value = vec![].into();
    assert!(alice.recv_open_ack(Some(ack)).await.is_err());
}

#[tokio::test]
async fn invalid_open_ack_counters_reject_establishment() {
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init(&alice, &bob).await;
    let syn = alice.send_open_syn(Reliability::Reliable).await.unwrap();
    bob.recv_open_syn(syn).await.unwrap();
    assert!(bob
        .send_open_ack(Reliability::Reliable)
        .await
        .unwrap()
        .is_some());
    let counters = PriorityContainer::from_fn(|_| Ok::<_, ZError>(u16::MAX)).unwrap();
    let ack = ack_with_counters(HandoffCounterIds::PerPrio(counters));
    assert!(alice.recv_open_ack(Some(ack)).await.is_err());
}

#[tokio::test]
async fn unsolicited_open_ack_rejects_establishment() {
    let auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&auth);
    let ack = ack_with_counters(HandoffCounterIds::Disabled);
    assert!(alice.recv_open_ack(Some(ack)).await.is_err());
}

#[tokio::test]
async fn open_ack_without_local_proposal_rejects_establishment() {
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init(&alice, &bob).await;
    let ack = ack_with_counters(HandoffCounterIds::Disabled);
    assert!(alice.recv_open_ack(Some(ack)).await.is_err());
}
