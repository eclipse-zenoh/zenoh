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
    core::PriorityRange,
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
    Enabled { priorities: Option<PriorityRange> },
}

impl QoS {
    pub(crate) fn new(is_qos: bool, endpoint: &EndPoint) -> ZResult<Self> {
        if !is_qos {
            Ok(Self::Disabled)
        } else {
            const PRIORITY_METADATA_KEY: &str = "priorities";

            let endpoint_metadata = endpoint.metadata();

            let Some(mut priorities) = endpoint_metadata
                .get(PRIORITY_METADATA_KEY)
                .map(|metadata| metadata.split(".."))
            else {
                return Ok(Self::Enabled { priorities: None });
            };
            let start = priorities
                .next()
                .ok_or(zerror!("Invalid priority range syntax"))?
                .parse::<u8>()?;

            let end = priorities
                .next()
                .ok_or(zerror!("Invalid priority range syntax"))?
                .parse::<u8>()?;

            if priorities.next().is_some() {
                bail!("Invalid priority range syntax")
            };

            Ok(Self::Enabled {
                priorities: Some(PriorityRange::new(start, end)?),
            })
        }
    }

    fn try_from_u64(value: u64) -> ZResult<Self> {
        match value {
            0b00_u64 => Ok(QoS::Disabled),
            0b01_u64 => Ok(QoS::Enabled { priorities: None }),
            mut value if value & 0b10_u64 != 0 => {
                value >>= 2;
                let start = (value & 0xff) as u8;
                let end = ((value & 0xff00) >> 8) as u8;

                Ok(QoS::Enabled {
                    priorities: Some(PriorityRange::new(start, end)?),
                })
            }
            _ => Err(zerror!("invalid QoS").into()),
        }
    }

    /// Encodes [`QoS`] as a [`u64`].
    ///
    /// The two least significant bits are used to discrimnate three states:
    ///
    /// 1. QoS is disabled
    /// 2. QoS is enabled but no priority range is available
    /// 3. QoS is enabled and priority information is range, in which case the next 16 least
    ///    significant bits are used to encode the priority range.
    fn to_u64(self) -> u64 {
        match self {
            QoS::Disabled => 0b00_u64,
            QoS::Enabled { priorities: None } => 0b01_u64,
            QoS::Enabled {
                priorities: Some(range),
            } => ((range.end() as u64) << 10) | ((range.start() as u64) << 2) | 0b10_u64,
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
            QoS::Disabled | QoS::Enabled { priorities: None } => None,
            QoS::Enabled {
                priorities: Some(priorities),
            } => Some(*priorities),
        }
    }

    #[cfg(test)]
    pub(crate) fn rand() -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        if rng.gen_bool(0.5) {
            QoS::Disabled
        } else if rng.gen_bool(0.5) {
            QoS::Enabled { priorities: None }
        } else {
            QoS::Enabled {
                priorities: Some(PriorityRange::rand()),
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

        *state_self = match (*state_self, state_other) {
            (QoS::Disabled, _) | (_, QoS::Disabled) => QoS::Disabled,
            (QoS::Enabled { priorities: None }, QoS::Enabled { priorities: None }) => {
                QoS::Enabled { priorities: None }
            }
            (
                qos @ QoS::Enabled {
                    priorities: Some(_),
                },
                QoS::Enabled { priorities: None },
            )
            | (
                QoS::Enabled { priorities: None },
                qos @ QoS::Enabled {
                    priorities: Some(_),
                },
            ) => qos,
            (
                self_qos @ QoS::Enabled {
                    priorities: Some(priorities_self),
                },
                QoS::Enabled {
                    priorities: Some(priorities_other),
                },
            ) => {
                if priorities_other.includes(priorities_self) {
                    self_qos
                } else {
                    return Err(zerror!(
                        "The PriorityRange received in InitAck is not a superset of my PriorityRange"
                    )
                    .into());
                }
            }
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

        *state_self = match (*state_self, state_other) {
            (QoS::Disabled, _) | (_, QoS::Disabled) => QoS::Disabled,
            (QoS::Enabled { priorities: None }, QoS::Enabled { priorities: None }) => {
                QoS::Enabled { priorities: None }
            }
            (
                qos @ QoS::Enabled {
                    priorities: Some(_),
                },
                QoS::Enabled { priorities: None },
            )
            | (
                QoS::Enabled { priorities: None },
                qos @ QoS::Enabled {
                    priorities: Some(_),
                },
            ) => qos,
            (
                QoS::Enabled {
                    priorities: Some(priorities_self),
                },
                other_qos @ QoS::Enabled {
                    priorities: Some(priorities_other),
                },
            ) => {
                if priorities_self.includes(priorities_other) {
                    other_qos
                } else {
                    return Err(zerror!(
                        "The PriorityRange received in InitSyn is not a subset of my PriorityRange"
                    )
                    .into());
                }
            }
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
    use zenoh_protocol::core::PriorityRange;
    use zenoh_result::ZResult;

    use super::{QoS, QoSFsm};
    use crate::unicast::establishment::{AcceptFsm, OpenFsm};

    async fn test_priority_range_negotiation(
        qos_open: &mut QoS,
        qos_accept: &mut QoS,
    ) -> ZResult<()> {
        let fsm = QoSFsm::new();

        let ext = fsm.send_init_syn(&*qos_open).await?;
        fsm.recv_init_syn((qos_accept, ext)).await?;

        let ext = fsm.send_init_ack(&*qos_accept).await?;
        fsm.recv_init_ack((qos_open, ext)).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_1() {
        let qos_open = &mut QoS::Enabled { priorities: None };
        let qos_accept = &mut QoS::Enabled { priorities: None };

        match test_priority_range_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(*qos_open, QoS::Enabled { priorities: None });
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_2() {
        let qos_open = &mut QoS::Enabled { priorities: None };
        let qos_accept = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
        };

        match test_priority_range_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        priorities: Some(PriorityRange::new(1, 3).unwrap())
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_3() {
        let qos_open = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
        };
        let qos_accept = &mut QoS::Enabled { priorities: None };

        match test_priority_range_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        priorities: Some(PriorityRange::new(1, 3).unwrap())
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_4() {
        let qos_open = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
        };
        let qos_accept = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
        };

        match test_priority_range_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        priorities: Some(PriorityRange::new(1, 3).unwrap())
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_5() {
        let qos_open = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
        };
        let qos_accept = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(0, 4).unwrap()),
        };

        match test_priority_range_negotiation(qos_open, qos_accept).await {
            Err(err) => panic!("expected `Ok(())`, got: {err}"),
            Ok(()) => {
                assert_eq!(*qos_open, *qos_accept);
                assert_eq!(
                    *qos_open,
                    QoS::Enabled {
                        priorities: Some(PriorityRange::new(1, 3).unwrap())
                    }
                );
            }
        };
    }

    #[tokio::test]
    async fn test_priority_range_negotiation_scenario_6() {
        let qos_open = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(1, 3).unwrap()),
        };
        let qos_accept = &mut QoS::Enabled {
            priorities: Some(PriorityRange::new(2, 3).unwrap()),
        };

        if let Ok(()) = test_priority_range_negotiation(qos_open, qos_accept).await {
            panic!("expected `Err(_)`, got `Ok(())`")
        }
    }
}
