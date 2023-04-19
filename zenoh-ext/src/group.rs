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

//! To manage groups and group memeberships

use async_std::sync::Mutex;
use async_std::task::JoinHandle;
use flume::{Receiver, Sender};
use futures::prelude::*;
use futures::select;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryInto;
use std::ops::Add;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zenoh::prelude::r#async::*;
use zenoh::publication::Publisher;
use zenoh::query::ConsolidationMode;
use zenoh::Error as ZError;
use zenoh::Result as ZResult;
use zenoh::Session;
use zenoh_result::bail;
use zenoh_sync::Condition;

const GROUP_PREFIX: &str = "zenoh/ext/net/group";
const EVENT_POSTFIX: &str = "evt";
const VIEW_REFRESH_LEASE_RATIO: f32 = 0.75f32;
const DEFAULT_LEASE: Duration = Duration::from_secs(18);
const DEFAULT_PRIORITY: Priority = Priority::DataHigh;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JoinEvent {
    pub member: Member,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LeaseExpiredEvent {
    pub mid: OwnedKeyExpr,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LeaveEvent {
    pub mid: OwnedKeyExpr,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewLeaderEvent {
    pub mid: OwnedKeyExpr,
}

#[derive(Serialize, Deserialize, Debug)]
struct KeepAliveEvent {
    pub mid: OwnedKeyExpr,
}

#[derive(Serialize, Deserialize, Debug)]
enum GroupNetEvent {
    Join(JoinEvent),
    Leave(LeaveEvent),
    KeepAlive(KeepAliveEvent),
}

/// Events exposed to the user to be informed for relevant
/// changes in the group.
#[derive(Serialize, Deserialize, Debug)]
pub enum GroupEvent {
    Join(JoinEvent),
    Leave(LeaveEvent),
    LeaseExpired(LeaseExpiredEvent),
    NewLeader(NewLeaderEvent),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MemberLiveliness {
    Auto,
    Manual,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Member {
    mid: OwnedKeyExpr,
    info: Option<String>,
    liveliness: MemberLiveliness,
    lease: Duration,
    refresh_ratio: f32,
    #[serde(skip)]
    priority: Priority,
}

impl Member {
    pub fn new<T>(mid: T) -> ZResult<Member>
    where
        T: TryInto<OwnedKeyExpr> + Send,
        <T as TryInto<OwnedKeyExpr>>::Error: Into<ZError>,
    {
        let mid: OwnedKeyExpr = mid.try_into().map_err(|e| e.into())?;
        if mid.is_wild() {
            bail!("Member ID is not allowed to contain wildcards: {}", mid);
        }
        Ok(Member {
            mid,
            info: None,
            liveliness: MemberLiveliness::Auto,
            lease: DEFAULT_LEASE,
            refresh_ratio: VIEW_REFRESH_LEASE_RATIO,
            priority: DEFAULT_PRIORITY,
        })
    }

    pub fn id(&self) -> &keyexpr {
        &self.mid
    }

    pub fn info<T>(mut self, i: T) -> Self
    where
        T: Into<String>,
    {
        self.info = Some(i.into());
        self
    }

    pub fn lease(mut self, d: Duration) -> Self {
        self.lease = d;
        self
    }

    pub fn liveliness(mut self, l: MemberLiveliness) -> Self {
        self.liveliness = l;
        self
    }

    pub fn refresh_ratio(mut self, r: f32) -> Self {
        self.refresh_ratio = r;
        self
    }

    pub fn priority(mut self, p: Priority) -> Self {
        self.priority = p;
        self
    }
}

struct GroupState {
    gid: String,
    local_member: Member,
    members: Mutex<HashMap<OwnedKeyExpr, (Member, Instant)>>,
    group_publisher: Publisher<'static>,
    user_events_tx: Mutex<Option<Sender<GroupEvent>>>,
    cond: Condition,
}

pub struct Group {
    state: Arc<GroupState>,
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Group {
    fn drop(&mut self) {
        // cancel background tasks
        async_std::task::block_on(async {
            while let Some(handle) = self.tasks.pop() {
                let _ = handle.cancel().await;
            }
        });
    }
}

async fn keep_alive_task(state: Arc<GroupState>) {
    let mid = state.local_member.mid.clone();
    let evt = GroupNetEvent::KeepAlive(KeepAliveEvent { mid });
    let buf = bincode::serialize(&evt).unwrap();
    let period = state
        .local_member
        .lease
        .mul_f32(state.local_member.refresh_ratio);
    loop {
        async_std::task::sleep(period).await;
        log::trace!("Sending Keep Alive for: {}", &state.local_member.mid);
        let _ = state.group_publisher.put(buf.clone()).res().await;
    }
}

fn spawn_watchdog(s: Arc<GroupState>, period: Duration) -> JoinHandle<()> {
    let watch_dog = async move {
        loop {
            async_std::task::sleep(period).await;
            let now = Instant::now();
            let mut ms = s.members.lock().await;
            let expired_members: Vec<OwnedKeyExpr> = ms
                .iter()
                .filter(|e| e.1 .1 < now)
                .map(|e| e.0.clone())
                .collect();

            for e in &expired_members {
                log::debug!("Member with lease expired: {}", e);
                ms.remove(e);
            }
            if !expired_members.is_empty() {
                log::debug!("Other members list: {:?}", ms.keys());
                drop(ms);
                let u_evt = &*s.user_events_tx.lock().await;
                for e in expired_members {
                    if let Some(tx) = u_evt {
                        tx.send(GroupEvent::LeaseExpired(LeaseExpiredEvent { mid: e }))
                            .unwrap()
                    }
                }
            }
        }
    };
    async_std::task::spawn(watch_dog)
}

async fn query_handler(z: Arc<Session>, state: Arc<GroupState>) {
    let qres: KeyExpr = format!(
        "{}/{}/{}",
        GROUP_PREFIX, &state.gid, &state.local_member.mid
    )
    .try_into()
    .unwrap();
    log::debug!("Started query handler for: {}", &qres);
    let buf = bincode::serialize(&state.local_member).unwrap();
    let queryable = z.declare_queryable(&qres).res().await.unwrap();

    while let Ok(query) = queryable.recv_async().await {
        log::trace!("Serving query for: {}", &qres);
        query
            .reply(Ok(Sample::new(qres.clone(), buf.clone())))
            .res()
            .await
            .unwrap();
    }
}

async fn net_event_handler(z: Arc<Session>, state: Arc<GroupState>) {
    let sub = z
        .declare_subscriber(state.group_publisher.key_expr())
        .res()
        .await
        .unwrap();
    while let Ok(s) = sub.recv_async().await {
        match bincode::deserialize::<GroupNetEvent>(&(s.value.payload.contiguous())) {
            Ok(evt) => match evt {
                GroupNetEvent::Join(je) => {
                    log::debug!("Member join: {:?}", &je.member);
                    let alive_till = Instant::now().add(je.member.lease);
                    let mut ms = state.members.lock().await;
                    ms.insert(je.member.mid.clone(), (je.member.clone(), alive_till));
                    log::debug!("Other members list: {:?}", ms.keys());
                    state.cond.notify_all();
                    drop(ms);
                    let u_evt = &*state.user_events_tx.lock().await;
                    if let Some(tx) = u_evt {
                        tx.send(GroupEvent::Join(je)).unwrap()
                    }
                }
                GroupNetEvent::Leave(le) => {
                    log::debug!("Member leave: {:?}", &le.mid);
                    let mut ms = state.members.lock().await;
                    ms.remove(&le.mid);
                    log::debug!("Other members list: {:?}", ms.keys());
                    drop(ms);
                    let u_evt = &*state.user_events_tx.lock().await;
                    if let Some(tx) = u_evt {
                        tx.send(GroupEvent::Leave(le)).unwrap()
                    }
                }
                GroupNetEvent::KeepAlive(kae) => {
                    log::debug!(
                        "KeepAlive from {} ({})",
                        &kae.mid,
                        if kae.mid.ne(&state.local_member.mid) {
                            "other"
                        } else {
                            "myself"
                        }
                    );
                    if kae.mid.ne(&state.local_member.mid) {
                        let mut mm = state.members.lock().await;
                        let v = mm.remove(&kae.mid);
                        match v {
                            Some((m, _)) => {
                                log::trace!("Updating leasefor: {:?}", &kae.mid);
                                let alive_till = Instant::now().add(m.lease);
                                mm.insert(m.mid.clone(), (m, alive_till));
                            }
                            None => {
                                log::debug!(
                                    "Received Keep Alive from unknown member: {}",
                                    &kae.mid
                                );
                                let qres = format!("{}/{}/{}", GROUP_PREFIX, &state.gid, kae.mid);
                                // @TODO: we could also send this member info
                                let qc = ConsolidationMode::None;
                                log::trace!("Issuing Query for {}", &qres);
                                let receiver = z.get(&qres).consolidation(qc).res().await.unwrap();

                                while let Ok(reply) = receiver.recv_async().await {
                                    match reply.sample {
                                        Ok(sample) => {
                                            match bincode::deserialize::<Member>(
                                                &sample.payload.contiguous(),
                                            ) {
                                                Ok(m) => {
                                                    let mut expiry = Instant::now();
                                                    expiry = expiry.add(m.lease);
                                                    log::debug!(
                                                        "Received member information: {:?}",
                                                        &m
                                                    );
                                                    mm.insert(kae.mid.clone(), (m.clone(), expiry));
                                                    log::debug!(
                                                        "Other members list: {:?}",
                                                        mm.keys()
                                                    );
                                                    // Advertise a JoinEvent
                                                    let u_evt = &*state.user_events_tx.lock().await;
                                                    if let Some(tx) = u_evt {
                                                        let je = JoinEvent { member: m };
                                                        tx.send_async(GroupEvent::Join(je))
                                                            .await
                                                            .unwrap()
                                                    }
                                                }
                                                Err(e) => {
                                                    log::warn!(
                                                        "Unable to deserialize the Member info received: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            log::warn!("Error received: {}", e);
                                        }
                                    }
                                }
                                state.cond.notify_all();
                            }
                        }
                    } else {
                        log::trace!("KeepAlive from Local Participant -- Ignoring");
                    }
                }
            },
            Err(e) => {
                log::warn!("Failed decoding net-event due to: {:?}", e);
            }
        }
    }
}

impl Group {
    pub async fn join<T>(z: Arc<Session>, group: T, with: Member) -> ZResult<Group>
    where
        T: TryInto<OwnedKeyExpr>,
        <T as TryInto<OwnedKeyExpr>>::Error: Into<ZError>,
    {
        let group: OwnedKeyExpr = group.try_into().map_err(|e| e.into())?;
        if group.is_wild() {
            bail!("Group ID is not allowed to contain wildcards: {}", group);
        }

        let event_expr = format!("{GROUP_PREFIX}/{group}/{EVENT_POSTFIX}");
        let publisher = z
            .declare_publisher(event_expr)
            .priority(with.priority)
            .res()
            .await
            .unwrap();
        let state = Arc::new(GroupState {
            gid: String::from(group),
            local_member: with.clone(),
            members: Mutex::new(Default::default()),
            group_publisher: publisher,
            user_events_tx: Mutex::new(Default::default()),
            cond: Condition::new(),
        });
        let is_auto_liveliness = matches!(with.liveliness, MemberLiveliness::Auto);

        // announce the member:
        log::debug!("Sending Join Message for local member: {:?}", &with);
        let join_evt = GroupNetEvent::Join(JoinEvent { member: with });
        let buf = bincode::serialize(&join_evt).unwrap();
        let _ = state.group_publisher.put(buf).res().await;

        // If the liveliness is manual it is the user who has to assert it.
        if is_auto_liveliness {
            async_std::task::spawn(keep_alive_task(state.clone()));
        }
        let events_task = async_std::task::spawn(net_event_handler(z.clone(), state.clone()));
        let queries_task = async_std::task::spawn(query_handler(z.clone(), state.clone()));
        let watchdog_task = spawn_watchdog(state.clone(), Duration::from_secs(1));
        Ok(Group {
            state,
            tasks: Vec::from([events_task, queries_task, watchdog_task]),
        })
    }

    /// Returns a receivers that will allow to receive notifications for group events.
    /// Notice that there can be a single subscription at the time, each call to subscribe
    /// will cancel the previous subscription.
    pub async fn subscribe(&self) -> Receiver<GroupEvent> {
        let (tx, rx) = flume::unbounded();
        *self.state.user_events_tx.lock().await = Some(tx);
        rx
    }

    /// Returns the group identifier.
    pub fn group_id(&self) -> &str {
        &self.state.gid
    }

    /// Returns this member identifier.
    pub fn local_member_id(&self) -> &str {
        &self.state.local_member.mid
    }

    /// Returns the current group view, in other terms the list
    /// of group members.
    pub async fn view(&self) -> Vec<Member> {
        let mut ms: Vec<Member> = self
            .state
            .members
            .lock()
            .await
            .iter()
            .map(|e| e.1 .0.clone())
            .collect();
        ms.push(self.state.local_member.clone());
        ms
    }

    /// Wait for a view size to be established or times out. The resulting selector parameters
    /// indicates whether the desired view size has been established.
    pub async fn wait_for_view_size(&self, size: usize, timeout: Duration) -> bool {
        if self.state.members.lock().await.len() + 1 >= size {
            true
        } else {
            // let s = self.state.clone();
            let f = async {
                loop {
                    let ms = self.state.members.lock().await;
                    if ms.len() + 1 >= size {
                        return true;
                    } else {
                        self.state.cond.wait(ms).await;
                    }
                }
            };
            let r: bool = select! {
                p = f.fuse() => p,
                _ = async_std::task::sleep(timeout).fuse() => false,
            };
            r
        }
    }

    /// Returns the current group size.
    pub async fn size(&self) -> usize {
        let ms = self.state.members.lock().await;
        ms.len() + 1 // with +1 being the local member
    }

    /// Returns the evental leader for this group. Notice that a view change may cause
    /// a change on leader. Thus it is wise to always get the leader after a view change.
    pub async fn leader(&self) -> Member {
        use std::cmp::Ordering;
        let group = self.view().await;
        let mut leader = self.state.local_member.clone();
        for m in group {
            if leader.id().as_str().cmp(m.id().as_str()) == Ordering::Less {
                leader = m
            }
        }
        leader
    }
}
