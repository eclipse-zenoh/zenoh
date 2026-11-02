// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// AI Disclosure: Generated with assistance from OpenAI Codex.
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
async fn rejected_open_syn_refuses_the_link() {
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
    // An unpatched initiator enables SHM without waiting for an OpenAck, so the acceptor must
    // refuse the link. Returning Ok here leaves the acceptor plain and the initiator on SHM.
    assert!(
        bob.recv_open_syn(Some(syn)).await.is_err(),
        "acceptor declined an SHM proposal and kept the link"
    );
    assert_disabled(bob);
}

#[tokio::test]
async fn unusable_open_syn_counters_reject_establishment() {
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
    decoded.alice_counters = HandoffCounterIds::PerPrio(
        PriorityContainer::from_fn(|_| Ok::<_, ZError>(u16::MAX)).unwrap(),
    );
    let mut bytes = vec![];
    Zenoh080.write(&mut bytes.writer(), &decoded).unwrap();
    syn.value = bytes.into();
    assert!(bob.recv_open_syn(Some(syn)).await.is_err());
    assert_disabled(bob);
}

#[tokio::test]
async fn absent_open_syn_keeps_acceptor_plain() {
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let bob = ShmFsm::new(&bob_auth);
    bob.recv_open_syn(None).await.unwrap();
    let ack = bob.send_open_ack(Reliability::Reliable).await.unwrap();
    assert!(ack.is_none());
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
    let err = alice.recv_open_ack(Some(ack)).await.unwrap_err();
    assert!(
        err.to_string().contains("Recv OpenAck. Decoding error"),
        "unexpected error: {err}"
    );
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
    let err = alice.recv_open_ack(Some(ack)).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("Recv OpenAck. Handoff channel creation error"),
        "unexpected error: {err}"
    );
}

/// An InitAck that offers SHM through a segment the initiator cannot map.
async fn exchange_init_with_unreachable_segment(alice: &ShmFsm<'_>, bob: &ShmFsm<'_>) {
    let syn = alice.send_init_syn(&StateOpen::new()).await.unwrap();
    bob.recv_init_syn(syn).await.unwrap();
    let mut ack = bob.send_init_ack(()).await.unwrap().unwrap();
    let mut decoded: InitAck = Zenoh080.read(&mut ack.value.reader()).unwrap();
    let removed = AuthUnicast::new(&[]).unwrap();
    decoded.bob_segment = removed.segment_id();
    drop(removed);
    assert!(RXAuthSegment::open(decoded.bob_segment).is_err());
    let mut bytes = vec![];
    Zenoh080.write(&mut bytes.writer(), &decoded).unwrap();
    ack.value = bytes.into();
    alice.recv_init_ack(Some(ack)).await.unwrap();
}

#[tokio::test]
async fn unsolicited_open_ack_rejects_establishment() {
    // Positive control: the same flow with a reachable segment completes.
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init(&alice, &bob).await;
    let syn = alice.send_open_syn(Reliability::Reliable).await.unwrap();
    bob.recv_open_syn(syn).await.unwrap();
    let ack = bob.send_open_ack(Reliability::Reliable).await.unwrap();
    alice.recv_open_ack(ack).await.unwrap();

    // The InitAck offered SHM but the initiator mapped no segment, then an OpenAck
    // extension arrives. The acceptor runs with SHM, so the handshake must fail.
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init_with_unreachable_segment(&alice, &bob).await;
    assert!(alice.rx_segment.get().is_none());
    let ack = ack_with_counters(HandoffCounterIds::Disabled);
    let err = alice.recv_open_ack(Some(ack)).await.unwrap_err();
    assert!(
        err.to_string().contains("Unexpected SHM acknowledgement"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn open_ack_without_offer_is_ignored() {
    // Every OpenAck below carries an extension that no offer asked for: a valid one,
    // one with invalid counters and an undecodable one.
    let counters = PriorityContainer::from_fn(|_| Ok::<_, ZError>(u16::MAX)).unwrap();
    let acks = [
        ack_with_counters(HandoffCounterIds::Disabled),
        ack_with_counters(HandoffCounterIds::PerPrio(counters)),
        open::ext::Shm::new(vec![].into()),
    ];
    for ack in acks {
        // The InitAck carried no SHM extension, so the initiator ignores the OpenAck one
        // and both the transport and the link run without SHM.
        let alice_auth = AuthUnicast::new(&[]).unwrap();
        let alice = ShmFsm::new(&alice_auth);
        alice.recv_init_ack(None).await.unwrap();
        alice.recv_open_ack(Some(ack.clone())).await.unwrap();
        assert!(alice.rx_lease.get().is_none());
        assert_disabled(alice);

        // Negative control: after an InitAck that offered SHM the same extension fails.
        let alice_auth = AuthUnicast::new(&[]).unwrap();
        let bob_auth = AuthUnicast::new(&[]).unwrap();
        let alice = ShmFsm::new(&alice_auth);
        let bob = ShmFsm::new(&bob_auth);
        exchange_init(&alice, &bob).await;
        assert!(alice.recv_open_ack(Some(ack)).await.is_err());
    }
}

#[tokio::test]
async fn open_ack_without_local_proposal_rejects_establishment() {
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init(&alice, &bob).await;
    let ack = ack_with_counters(HandoffCounterIds::Disabled);
    let err = alice.recv_open_ack(Some(ack)).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("SHM acknowledgement without a local proposal"),
        "unexpected error: {err}"
    );
}

/// Counts the TX counters that the segment of `auth` can still lease. The leases taken for the
/// count return to the pool when the function ends.
fn free_tx_counters(auth: &AuthUnicast) -> usize {
    let mut held = vec![];
    while let Ok(lease) = auth.lease_tx_counter() {
        held.push(lease);
    }
    held.len()
}

#[tokio::test]
async fn declined_open_syn_leaves_acceptor_tx_counters_free() {
    let full = free_tx_counters(&AuthUnicast::new(&[]).unwrap());

    // Positive control: an accepted proposal makes the acceptor lease counters, and dropping the
    // acceptor returns them. Without this the count below could not show a leak.
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    {
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
        assert!(free_tx_counters(&bob_auth) < full, "no counter was leased");
    }
    assert_eq!(free_tx_counters(&bob_auth), full);

    let declines: [fn(&mut OpenSyn); 2] = [
        |syn| syn.bob_challenge ^= 1,
        |syn| {
            syn.alice_counters = HandoffCounterIds::PerPrio(
                PriorityContainer::from_fn(|_| Ok::<_, ZError>(u16::MAX)).unwrap(),
            )
        },
    ];
    for decline in declines {
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
        decline(&mut decoded);
        let mut bytes = vec![];
        Zenoh080.write(&mut bytes.writer(), &decoded).unwrap();
        syn.value = bytes.into();
        assert!(bob.recv_open_syn(Some(syn)).await.is_err());
        // The acceptor is still alive here, so a counter that the declined path took and did not
        // return shows up as a missing counter.
        assert_eq!(
            free_tx_counters(&bob_auth),
            full,
            "declined proposal kept an acceptor TX counter"
        );
        assert!(bob
            .send_open_ack(Reliability::Reliable)
            .await
            .unwrap()
            .is_none());
        assert_eq!(free_tx_counters(&bob_auth), full);
    }
}

/// A 1.9.0 initiator puts its segment in the InitSyn, so the acceptor maps it. Its OpenSyn
/// extension has the 1.9.0 format and arrives at the acceptor as no extension. This models what
/// the acceptor sees, not the 1.9.0 code of the initiator.
#[tokio::test]
async fn initiator_without_open_syn_extension_keeps_acceptor_plain() {
    let full = free_tx_counters(&AuthUnicast::new(&[]).unwrap());

    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init(&alice, &bob).await;
    // The acceptor holds the initiator segment. Enabling SHM from this alone is the old defect.
    assert!(bob.rx_segment.get().is_some());
    bob.recv_open_syn(None).await.unwrap();
    assert!(bob
        .send_open_ack(Reliability::Reliable)
        .await
        .unwrap()
        .is_none());
    assert_eq!(free_tx_counters(&bob_auth), full);
    assert_disabled(bob);

    // Positive control: the same InitSyn followed by a current OpenSyn enables SHM.
    let alice_auth = AuthUnicast::new(&[]).unwrap();
    let bob_auth = AuthUnicast::new(&[]).unwrap();
    let alice = ShmFsm::new(&alice_auth);
    let bob = ShmFsm::new(&bob_auth);
    exchange_init(&alice, &bob).await;
    let syn = alice.send_open_syn(Reliability::Reliable).await.unwrap();
    bob.recv_open_syn(syn).await.unwrap();
    let ack = bob.send_open_ack(Reliability::Reliable).await.unwrap();
    alice.recv_open_ack(ack).await.unwrap();
    assert!(bob.shm_init_result().0.is_some());
}
