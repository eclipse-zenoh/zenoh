use async_std::sync::{Arc, Mutex};
use async_std::task::JoinHandle;
use flume::{Receiver, Sender};
use futures::prelude::*;
use futures::select;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Add;
use std::time::{Duration, Instant};
use zenoh::net::queryable::EVAL;
use zenoh::net::{
    CongestionControl, ConsolidationMode, QueryConsolidation, QueryTarget, Reliability, ResKey,
    Sample, Session, SubInfo, SubMode, ZFuture,
};
use zenoh_util::sync::Condition;

const GROUP_PREFIX: &str = "/zenoh/ext/net/group";
const EVENT_POSTFIX: &str = "evt";
const VIEW_REFRESH_LEASE_RATIO: f32 = 0.75f32;
const DEFAULT_LEASE: Duration = Duration::from_secs(18);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct JoinEvent {
    pub member: Member,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LeaseExpiredEvent {
    pub mid: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LeaveEvent {
    pub mid: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewGroupViewEvent {
    pub source: String,
    pub members: Vec<Member>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct NewLeaderEvent {
    pub mid: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct KeepAliveEvent {
    pub mid: String,
}

#[derive(Serialize, Deserialize, Debug)]
enum GroupNetEvent {
    Join(JoinEvent),
    Leave(LeaveEvent),
    KeepAlive(KeepAliveEvent),
    NewGroupView(NewGroupViewEvent),
}

/// Events exposed to the user to be informed for relevant
/// changes in the group.
#[derive(Serialize, Deserialize, Debug)]
pub enum GroupEvent {
    Join(JoinEvent),
    Leave(LeaveEvent),
    LeaseExpired(LeaseExpiredEvent),
    NewLeader(NewLeaderEvent),
    NewGroupView(NewGroupViewEvent),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum MemberLiveliness {
    Auto,
    Manual,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Member {
    mid: String,
    info: Option<String>,
    liveliness: MemberLiveliness,
    lease: Duration,
}

impl Member {
    pub fn new(mid: &str) -> Member {
        Member {
            mid: String::from(mid),
            info: None,
            liveliness: MemberLiveliness::Auto,
            lease: DEFAULT_LEASE,
        }
    }

    pub fn id(&self) -> &str {
        &self.mid
    }

    pub fn info(mut self, i: &str) -> Self {
        self.info = Some(String::from(i));
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
}

struct GroupState {
    gid: String,
    local_member: Member,
    members: Mutex<HashMap<String, (Member, Instant)>>,
    _group_resource: String,
    _group_resource_id: u64,
    event_resource: ResKey,
    user_events_tx: Mutex<Option<Sender<GroupEvent>>>,
    cond: Condition,
}

pub struct Group {
    state: Arc<GroupState>,
}

async fn keep_alive_task(z: Arc<Session>, state: Arc<GroupState>) {
    let mid = state.local_member.mid.clone();
    let evt = GroupNetEvent::KeepAlive(KeepAliveEvent { mid });
    let buf = bincode::serialize(&evt).unwrap();
    let period = state.local_member.lease.mul_f32(VIEW_REFRESH_LEASE_RATIO);
    loop {
        async_std::task::sleep(period).await;
        log::debug!("Sending Keep Alive for: {}", &state.local_member.mid);
        let _ = z
            .write_ext(
                &state.event_resource,
                (buf.clone()).into(),
                0,
                0,
                CongestionControl::Drop,
            )
            .wait();
    }
}

fn spawn_watchdog(s: Arc<GroupState>, period: Duration) -> JoinHandle<()> {
    let watch_dog = async move {
        loop {
            async_std::task::sleep(period).await;
            let now = Instant::now();
            let mut ms = s.members.lock().await;
            let expired_members: Vec<String> = ms
                .iter()
                .filter(|e| e.1 .1 < now)
                .map(|e| String::from(e.0))
                .collect();

            for e in &expired_members {
                ms.remove(e);
            }
            drop(ms);
            let u_evt = &*s.user_events_tx.lock().await;
            for e in expired_members {
                if let Some(tx) = u_evt {
                    tx.send(GroupEvent::LeaseExpired(LeaseExpiredEvent { mid: e }))
                        .unwrap()
                }
            }
        }
    };
    async_std::task::spawn(watch_dog)
}

async fn query_handler(z: Arc<Session>, state: Arc<GroupState>) {
    let qres = format!(
        "{}/{}/{}",
        GROUP_PREFIX, &state.gid, &state.local_member.mid
    );
    log::debug!("Started query handler for: {}", &qres);
    let buf = bincode::serialize(&state.local_member).unwrap();
    let mut queryable = z
        .declare_queryable(&qres.clone().into(), EVAL)
        .await
        .unwrap();

    while let Some(query) = queryable.receiver().next().await {
        log::debug!("Serving query for: {}", &qres);
        query.reply(Sample {
            res_name: qres.clone(),
            payload: buf.clone().into(),
            data_info: None,
        })
    }
}

async fn advertise_view(z: &Arc<Session>, state: &Arc<GroupState>) {
    log::debug!("Maybe Advertising NewGroupView....");
    let mut min: String = state.local_member.mid.clone();
    let sid = &state.local_member.mid;
    let mut members: Vec<Member> = state
        .members
        .lock()
        .await
        .iter()
        .map(|e| {
            if e.0 < sid {
                min = e.0.clone()
            };
            e.1 .0.clone()
        })
        .collect();
    members.push(state.local_member.clone());
    if min == *sid {
        let evt = GroupNetEvent::NewGroupView(NewGroupViewEvent {
            source: sid.to_string(),
            members,
        });
        log::debug!("Advertising NewGroupView: {:?}", &evt);
        let buf = bincode::serialize(&evt).unwrap();
        let res = format!("{}/{}/{}", GROUP_PREFIX, &state.gid, EVENT_POSTFIX);
        let _ = z.write(&res.into(), buf.into()).wait();
    }
}

async fn net_event_handler(z: Arc<Session>, state: Arc<GroupState>) {
    let sub_info = SubInfo {
        period: None,
        mode: SubMode::Push,
        reliability: Reliability::Reliable,
    };
    let mut sub = z
        .declare_subscriber(&state.event_resource, &sub_info)
        .await
        .unwrap();
    let stream = sub.receiver();
    while let Some(s) = stream.next().await {
        log::debug!("Handling Network Event...");
        match bincode::deserialize::<GroupNetEvent>(&(s.payload.to_vec())) {
            Ok(evt) => match evt {
                GroupNetEvent::Join(je) => {
                    advertise_view(&z, &state).await;
                    log::debug!("Member joining the group:\n{:?}", &je.member);
                    let alive_till = Instant::now().add(je.member.lease);
                    let mut ms = state.members.lock().await;
                    ms.insert(je.member.mid.clone(), (je.member.clone(), alive_till));
                    state.cond.notify_all();
                    drop(ms);
                    let u_evt = &*state.user_events_tx.lock().await;
                    if let Some(tx) = u_evt {
                        tx.send(GroupEvent::Join(je)).unwrap()
                    }
                }
                GroupNetEvent::Leave(le) => {
                    log::debug!("Member leaving:\n{:?}", &le.mid);
                    state.members.lock().await.remove(&le.mid);
                    let u_evt = &*state.user_events_tx.lock().await;
                    if let Some(tx) = u_evt {
                        tx.send(GroupEvent::Leave(le)).unwrap()
                    }
                }
                GroupNetEvent::KeepAlive(kae) => {
                    log::debug!(
                        "KeepAlive for {} != {} -> {}",
                        &kae.mid,
                        state.local_member.mid,
                        kae.mid.ne(&state.local_member.mid)
                    );
                    if kae.mid.ne(&state.local_member.mid) {
                        let mut mm = state.members.lock().await;
                        log::debug!("Members: \n{:?}", &mm);
                        let v = mm.remove(&kae.mid);
                        match v {
                            Some((m, _)) => {
                                log::debug!("Updating leasefor: \n{:?}", &kae.mid);
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
                                let qc = QueryConsolidation {
                                    first_routers: ConsolidationMode::None,
                                    last_router: ConsolidationMode::None,
                                    reception: ConsolidationMode::None,
                                };
                                log::debug!("Issuing Query for {}", &qres);
                                let mut receiver = z
                                    .query(&qres.into(), "", QueryTarget::default(), qc)
                                    .await
                                    .unwrap();

                                while let Some(sample) = receiver.next().await {
                                    match bincode::deserialize::<Member>(
                                        &sample.data.payload.to_vec(),
                                    ) {
                                        Ok(m) => {
                                            let mut expiry = Instant::now();
                                            expiry = expiry.add(m.lease);
                                            log::debug!("Received member information: {:?}", &m);
                                            mm.insert(kae.mid.clone(), (m, expiry));
                                        }
                                        Err(e) => {
                                            log::debug!(
                                                "Unable to deserialize the Member info received:\n {}", e);
                                        }
                                    }
                                }
                                state.cond.notify_all();
                            }
                        }
                    } else {
                        log::debug!("KeepAlive from Local Participant -- Ignoring");
                    }
                }
                GroupNetEvent::NewGroupView(ngve) => {
                    let mut ms = state.members.lock().await;
                    for m in ngve.members {
                        if ms.get(&m.mid).is_none() {
                            let alive_till = Instant::now().add(m.lease);
                            ms.insert(m.mid.clone(), (m, alive_till));
                        }
                    }
                }
            },
            Err(e) => {
                log::warn!("Failed decoding net-event due to:\n{:?}", e);
            }
        }
    }
}

impl Group {
    pub async fn join(z: Arc<Session>, group: &str, with: Member) -> Group {
        let _group_resource = format!("{}/{}", GROUP_PREFIX, group);
        let rid = z
            .declare_resource(&(_group_resource.clone()).into())
            .await
            .unwrap();
        let event_resource = ResKey::RIdWithSuffix(rid, EVENT_POSTFIX.into());
        let state = Arc::new(GroupState {
            gid: String::from(group),
            local_member: with.clone(),
            members: Mutex::new(Default::default()),
            _group_resource,
            _group_resource_id: rid,
            event_resource: event_resource.clone(),
            user_events_tx: Mutex::new(Default::default()),
            cond: Condition::new(),
        });
        let is_auto_liveliness = matches!(with.liveliness, MemberLiveliness::Auto);

        // announce the member:
        log::debug!("Sending Join Message for local member:\n{:?}", &with);
        let join_evt = GroupNetEvent::Join(JoinEvent { member: with });
        let buf = bincode::serialize(&join_evt).unwrap();
        let _ = z.write(&event_resource, buf.into()).await;

        // If the liveliness is manual it is the user who has to assert it.
        if is_auto_liveliness {
            async_std::task::spawn(keep_alive_task(z.clone(), state.clone()));
        }
        let _ = async_std::task::spawn(net_event_handler(z.clone(), state.clone()));
        let _ = async_std::task::spawn(query_handler(z.clone(), state.clone()));
        let _ = spawn_watchdog(state.clone(), Duration::from_secs(1));
        Group { state }
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

    /// Wait for a view size to be established or times out. The resulting predicate
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
}
