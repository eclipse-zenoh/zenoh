//
// Copyright (c) 2022 ZettaScale Technology
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
use core::marker::PhantomData;
use std::str::FromStr;

use async_trait::async_trait;
use serde::Serialize;
use zenoh_buffers::{
    reader::{DidntRead, Reader},
    writer::{DidntWrite, Writer},
};
use zenoh_codec::{RCodec, WCodec, Zenoh080};
use zenoh_core::{bail, zerror};
use zenoh_link::EndPoint;
use zenoh_protocol::{
    core::{PriorityRange, Reliability},
    transport::{init, open},
};
use zenoh_result::{Error as ZError, ZResult};

use crate::unicast::establishment::{AcceptFsm, OpenFsm};

// Extension Fsm
pub(crate) struct QoSFsm<'a> {
    _a: PhantomData<&'a ()>,
}

impl<'a> QoSFsm<'a> {
    pub(crate) const fn new() -> Self {
        Self { _a: PhantomData }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) enum QoS {
    Disabled,
    Enabled {
        reliability: Option<Reliability>,
        priorities: Option<PriorityRange>,
    },
}

impl QoS {
    pub(crate) fn new(is_qos: bool, endpoint: &EndPoint) -> ZResult<Self> {
        if !is_qos {
            Ok(QoS::Disabled)
        } else {
            const RELIABILITY_METADATA_KEY: &str = "reliability";
            const PRIORITY_METADATA_KEY: &str = "priorities";

            let metadata = endpoint.metadata();

            let reliability = metadata
                .get(RELIABILITY_METADATA_KEY)
                .map(Reliability::from_str)
                .transpose()?;

            let priorities = metadata
                .get(PRIORITY_METADATA_KEY)
                .map(|metadata| {
                    let mut metadata = metadata.split("..");

                    let start = metadata
                        .next()
                        .ok_or(zerror!("Invalid priority range syntax"))?
                        .parse::<u8>()?;

                    let end = metadata
                        .next()
                        .ok_or(zerror!("Invalid priority range syntax"))?
                        .parse::<u8>()?;

                    if metadata.next().is_some() {
                        bail!("Invalid priority range syntax")
                    };

                    PriorityRange::new(start, end)
                })
                .transpose()?;

            Ok(QoS::Enabled {
                priorities,
                reliability,
            })
        }
    }

    fn try_from_u64(value: u64) -> ZResult<Self> {
        match value {
            0b000_u64 => Ok(QoS::Disabled),
            0b001_u64 => Ok(QoS::Enabled {
                priorities: None,
                reliability: None,
            }),
            value if value & 0b110_u64 != 0 => {
                let tag = value & 0b111_u64;

                let priorities = if tag & 0b010_u64 != 0 {
                    let start = ((value >> 3) & 0xff) as u8;
                    let end = ((value >> (3 + 8)) & 0xff) as u8;

                    Some(PriorityRange::new(start, end)?)
                } else {
                    None
                };

                let reliability = if tag & 0b100_u64 != 0 {
                    let bit = ((value >> (3 + 8 + 8)) & 0x1) as u8 == 1;

                    Some(Reliability::from(bit))
                } else {
                    None
                };

                Ok(QoS::Enabled {
                    priorities,
                    reliability,
                })
            }
            _ => Err(zerror!("invalid QoS").into()),
        }
    }

    /// Encodes [`QoS`] as a [`u64`].
    ///
    /// The three least significant bits are used to discrimnate five states:
    ///
    /// 1. QoS is disabled
    /// 2. QoS is enabled but no priority range and no reliability setting are available
    /// 3. QoS is enabled and priority range is available but reliability is unavailable
    /// 4. QoS is enabled and reliability is available but priority range is unavailable
    /// 5. QoS is enabled and both priority range and reliability are available
    fn to_u64(self) -> u64 {
        match self {
            QoS::Disabled => 0b000_u64,
            QoS::Enabled {
                priorities,
                reliability,
            } => {
                if reliability.is_none() && priorities.is_none() {
                    return 0b001_u64;
                }

                let mut value = 0b000_u64;

                if let Some(priorities) = priorities {
                    value |= 0b010_u64;
                    value |= (priorities.start() as u64) << 3;
                    value |= (priorities.end() as u64) << (3 + 8);
                }

                if let Some(reliability) = reliability {
                    value |= 0b100_u64;
                    value |= (bool::from(reliability) as u64) << (3 + 8 + 8);
                }

                value
            }
        }
    }

    fn to_ext(self) -> Option<init::ext::QoS> {
        if self.is_enabled() {
            Some(init::ext::QoS::new(self.to_u64()))
        } else {
            None
        }
    }

    fn try_from_ext(ext: Option<init::ext::QoS>) -> ZResult<Self> {
        if let Some(ext) = ext {
            QoS::try_from_u64(ext.value)
        } else {
            Ok(QoS::Disabled)
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        matches!(self, QoS::Enabled { .. })
    }

    pub(crate) fn priorities(&self) -> Option<PriorityRange> {
        match self {
            QoS::Disabled
            | QoS::Enabled {
                priorities: None, ..
            } => None,
            QoS::Enabled {
                priorities: Some(priorities),
                ..
            } => Some(*priorities),
        }
    }

    pub(crate) fn reliability(&self) -> Option<Reliability> {
        match self {
            QoS::Disabled
            | QoS::Enabled {
                reliability: None, ..
            } => None,
            QoS::Enabled {
                reliability: Some(reliability),
                ..
            } => Some(*reliability),
        }
    }

    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        if rng.gen_bool(0.5) {
            QoS::Disabled
        } else {
            let priorities = rng.gen_bool(0.5).then(PriorityRange::rand);
            let reliability = rng
                .gen_bool(0.5)
                .then(|| Reliability::from(rng.gen_bool(0.5)));

            QoS::Enabled {
                priorities,
                reliability,
            }
        }
    }
}

// Codec
impl<W> WCodec<&QoS, &mut W> for Zenoh080
where
    W: Writer,
{
    type Output = Result<(), DidntWrite>;

    fn write(self, writer: &mut W, x: &QoS) -> Self::Output {
        self.write(writer, &x.to_u64())
    }
}

impl<R> RCodec<QoS, &mut R> for Zenoh080
where
    R: Reader,
{
    type Error = DidntRead;

    fn read(self, reader: &mut R) -> Result<QoS, Self::Error> {
        QoS::try_from_u64(self.read(reader)?).map_err(|_| DidntRead)
    }
}

#[async_trait]
impl<'a> OpenFsm for &'a QoSFsm<'a> {
    type Error = ZError;

    type SendInitSynIn = &'a QoS;
    type SendInitSynOut = Option<init::ext::QoS>;
    async fn send_init_syn(
        self,
        state: Self::SendInitSynIn,
    ) -> Result<Self::SendInitSynOut, Self::Error> {
        Ok(state.to_ext())
    }

    type RecvInitAckIn = (&'a mut QoS, Option<init::ext::QoS>);
    type RecvInitAckOut = ();
    async fn recv_init_ack(
        self,
        input: Self::RecvInitAckIn,
    ) -> Result<Self::RecvInitAckOut, Self::Error> {
        let (state_self, other_ext) = input;

        let state_other = QoS::try_from_ext(other_ext)?;

        let (
            QoS::Enabled {
                reliability: self_reliability,
                priorities: self_priorities,
            },
            QoS::Enabled {
                reliability: other_reliability,
                priorities: other_priorities,
            },
        ) = (*state_self, state_other)
        else {
            *state_self = QoS::Disabled;
            return Ok(());
        };

        let priorities = match (self_priorities, other_priorities) {
            (None, priorities) | (priorities, None) => priorities,
            (Some(self_priorities), Some(other_priorities)) => {
                if other_priorities.includes(self_priorities) {
                    Some(self_priorities)
                } else {
                    return Err(zerror!(
                        "The PriorityRange received in InitAck is not a superset of my PriorityRange"
                    )
                    .into());
                }
            }
        };

        let reliability = match (self_reliability, other_reliability) {
            (None, reliability) | (reliability, None) => reliability,
            (Some(self_reliability), Some(other_reliability)) => {
                if other_reliability.implies(self_reliability) {
                    Some(self_reliability)
                } else {
                    return Err(zerror!(
                        "The Reliability received in InitAck cannot be substituted with my Reliability"
                    )
                    .into());
                }
            }
        };

        *state_self = QoS::Enabled {
            reliability,
            priorities,
        };

        Ok(())
    }

    type SendOpenSynIn = &'a QoS;
    type SendOpenSynOut = Option<open::ext::QoS>;
    async fn send_open_syn(
        self,
        _state: Self::SendOpenSynIn,
    ) -> Result<Self::SendOpenSynOut, Self::Error> {
        Ok(None)
    }

    type RecvOpenAckIn = (&'a mut QoS, Option<open::ext::QoS>);
    type RecvOpenAckOut = ();
    async fn recv_open_ack(
        self,
        _state: Self::RecvOpenAckIn,
    ) -> Result<Self::RecvOpenAckOut, Self::Error> {
        Ok(())
    }
}

#[async_trait]
impl<'a> AcceptFsm for &'a QoSFsm<'a> {
    type Error = ZError;

    type RecvInitSynIn = (&'a mut QoS, Option<init::ext::QoS>);
    type RecvInitSynOut = ();
    async fn recv_init_syn(
        self,
        input: Self::RecvInitSynIn,
    ) -> Result<Self::RecvInitSynOut, Self::Error> {
        let (state_self, other_ext) = input;

        let state_other = QoS::try_from_ext(other_ext)?;

        let (
            QoS::Enabled {
                reliability: self_reliability,
                priorities: self_priorities,
            },
            QoS::Enabled {
                reliability: other_reliability,
                priorities: other_priorities,
            },
        ) = (*state_self, state_other)
        else {
            *state_self = QoS::Disabled;
            return Ok(());
        };

        let priorities = match (self_priorities, other_priorities) {
            (None, priorities) | (priorities, None) => priorities,
            (Some(self_priorities), Some(other_priorities)) => {
                if self_priorities.includes(other_priorities) {
                    Some(other_priorities)
                } else {
                    return Err(zerror!(
                        "The PriorityRange received in InitSyn is not a subset of my PriorityRange"
                    )
                    .into());
                }
            }
        };

        let reliability = match (self_reliability, other_reliability) {
            (None, reliability) | (reliability, None) => reliability,
            (Some(self_reliability), Some(other_reliability)) => {
                if self_reliability.implies(other_reliability) {
                    Some(other_reliability)
                } else {
                    return Err(zerror!(
                        "The Reliability received in InitSyn cannot be substituted for my Reliability"
                    )
                    .into());
                }
            }
        };

        *state_self = QoS::Enabled {
            reliability,
            priorities,
        };

        Ok(())
    }

    type SendInitAckIn = &'a QoS;
    type SendInitAckOut = Option<init::ext::QoS>;
    async fn send_init_ack(
        self,
        state: Self::SendInitAckIn,
    ) -> Result<Self::SendInitAckOut, Self::Error> {
        Ok(state.to_ext())
    }

    type RecvOpenSynIn = (&'a mut QoS, Option<open::ext::QoS>);
    type RecvOpenSynOut = ();
    async fn recv_open_syn(
        self,
        _state: Self::RecvOpenSynIn,
    ) -> Result<Self::RecvOpenSynOut, Self::Error> {
        Ok(())
    }

    type SendOpenAckIn = &'a QoS;
    type SendOpenAckOut = Option<open::ext::QoS>;
    async fn send_open_ack(
        self,
        _state: Self::SendOpenAckIn,
    ) -> Result<Self::SendOpenAckOut, Self::Error> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use zenoh_protocol::core::{PriorityRange, Reliability};
    use zenoh_result::ZResult;

    use super::{QoS, QoSFsm};
    use crate::unicast::establishment::{AcceptFsm, OpenFsm};

    async fn test_negotiation(qos_open: &mut QoS, qos_accept: &mut QoS) -> ZResult<()> {
        let fsm = QoSFsm::new();

        let ext = fsm.send_init_syn(&*qos_open).await?;
        fsm.recv_init_syn((qos_accept, ext)).await?;

        let ext = fsm.send_init_ack(&*qos_accept).await?;
        fsm.recv_init_ack((qos_open, ext)).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_1() {
        let qos_open = &mut QoS::Enabled {
            priorities: None,
            reliability: None,
        };
        let qos_accept = &mut QoS::Enabled {
            priorities: None,
            reliability: None,
        };

        match test_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        priorities: None,
                        reliability: None
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_2() {
        let qos_open = &mut QoS::Enabled {
            priorities: None,
            reliability: None,
        };
        let qos_accept = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
            reliability: None,
        };

        match test_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        priorities: Some(PriorityRange::new(1, 3).unwrap()),
                        reliability: None
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_3() {
        let qos_open = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
            reliability: None,
        };
        let qos_accept = &mut QoS::Enabled {
            priorities: None,
            reliability: None,
        };

        match test_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        priorities: Some(PriorityRange::new(1, 3).unwrap()),
                        reliability: None
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_4() {
        let qos_open = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
            reliability: None,
        };
        let qos_accept = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
            reliability: None,
        };

        match test_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        priorities: Some(PriorityRange::new(1, 3).unwrap()),
                        reliability: None
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_5() {
        let qos_open = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
            reliability: None,
        };
        let qos_accept = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(0, 4).unwrap()),
            reliability: None,
        };

        match test_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        priorities: Some(PriorityRange::new(1, 3).unwrap()),
                        reliability: None
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_6() {
        let qos_open = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
            reliability: None,
        };
        let qos_accept = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(2, 3).unwrap()),
            reliability: None,
        };

        if let Ok(()) = test_negotiation(qos_open, qos_accept).await {
            panic!("expected `Err(_)`, got `Ok(())`")
        }
    }

    #[tokio::test]
    async fn test_reliability_negotiation_scenario_2() {
        let qos_open = &mut QoS::Enabled {
            reliability: None,
            priorities: None,
        };
        let qos_accept = &mut QoS::Enabled {
            reliability: Some(Reliability::BestEffort),
            priorities: None,
        };

        match test_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        reliability: Some(Reliability::BestEffort),
                        priorities: None
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_reliability_negotiation_scenario_3() {
        let qos_open = &mut QoS::Enabled {
            reliability: Some(Reliability::Reliable),
            priorities: None,
        };
        let qos_accept = &mut QoS::Enabled {
            reliability: Some(Reliability::BestEffort),
            priorities: None,
        };

        match test_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        reliability: Some(Reliability::Reliable),
                        priorities: None
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_reliability_negotiation_scenario_4() {
        let qos_open = &mut QoS::Enabled {
            reliability: Some(Reliability::Reliable),
            priorities: None,
        };
        let qos_accept = &mut QoS::Enabled {
            reliability: Some(Reliability::Reliable),
            priorities: None,
        };

        match test_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        reliability: Some(Reliability::Reliable),
                        priorities: None
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_reliability_negotiation_scenario_5() {
        let qos_open = &mut QoS::Enabled {
            reliability: Some(Reliability::BestEffort),
            priorities: None,
        };
        let qos_accept = &mut QoS::Enabled {
            reliability: Some(Reliability::Reliable),
            priorities: None,
        };

        if let Ok(()) = test_negotiation(qos_open, qos_accept).await {
            panic!("expected `Err(_)`, got `Ok(())`")
        }
    }

    #[tokio::test]
    async fn test_priority_range_and_reliability_negotiation_scenario_1() {
        let qos_open = &mut QoS::Enabled {
            reliability: Some(Reliability::Reliable),
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
        };
        let qos_accept = &mut QoS::Enabled {
            reliability: Some(Reliability::BestEffort),
            priorities: Some(PriorityRange::new(1, 4).unwrap()),
        };

        match test_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        reliability: Some(Reliability::Reliable),
                        priorities: Some(PriorityRange::new(1, 3).unwrap())
                    }
                );
            }
        };
    }
}
