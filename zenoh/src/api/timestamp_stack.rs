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
use std::sync::Arc;

use zenoh_protocol::{
    core::WhatAmI,
    network::timestamp_stack::{interception_point, Interception, TsStackType},
};

use crate::{net::runtime::IRuntime, session::ZenohId};

/// Context passed to the user-defined timestamp callback.
///
/// This struct provides the callback with information about the current Zenoh node
/// and the interception point at which the timestamp is being generated.
///
/// The struct is `#[non_exhaustive]` to allow adding new fields in the future.
#[non_exhaustive]
#[zenoh_macros::unstable]
pub struct TsStackContext {
    /// The Zenoh ID of the current node.
    pub zid: ZenohId,
    /// The mode of the current node (router, peer, or client).
    pub whatami: WhatAmI,
    // TODO: should be a non-exhaustive enum
    /// The interception point identifier (e.g., `zenoh_protocol::network::timestamp_stack::interception_point::SEND`).
    pub interception_point: InterceptionPoint,
}

/// Type alias for the user-defined timestamp callback.
///
/// The callback receives a [`TsStackContext`] and returns the raw timestamp bytes
/// to be pushed onto the timestamp stack.
#[zenoh_macros::unstable]
pub type GetTimestampCallback = Arc<dyn Fn(TsStackContext) -> Vec<u8> + Send + Sync>;

/// Identifies which interception point a timestamp record was captured at.
#[zenoh_macros::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterceptionPoint {
    Send,
    Route,
    Receive,
}

impl TryFrom<u8> for InterceptionPoint {
    type Error = zenoh_result::Error;

    fn try_from(value: u8) -> zenoh_result::ZResult<Self> {
        match value & !interception_point::IS_CUSTOM_TS {
            interception_point::SEND => Ok(Self::Send),
            interception_point::RECEIVE => Ok(Self::Receive),
            interception_point::ROUTE => Ok(Self::Route),
            _ => bail!("Unknown interception point ID '{value}'"),
        }
    }
}

impl From<InterceptionPoint> for u8 {
    fn from(value: InterceptionPoint) -> Self {
        match value {
            InterceptionPoint::Send => interception_point::SEND,
            InterceptionPoint::Route => interception_point::ROUTE,
            InterceptionPoint::Receive => interception_point::RECEIVE,
        }
    }
}

/// Builder for [`TimestampInstrumentation`] instances
#[zenoh_macros::unstable]
#[derive(Debug, Default, Clone, Copy)]
pub struct TimestampInstrumentationBuilder {
    conf_flags: u8,
}

#[zenoh_macros::unstable]
impl TimestampInstrumentationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record timestamps at the SEND point.
    pub fn set_send(self, enabled: bool) -> Self {
        Self {
            conf_flags: if enabled {
                self.conf_flags | interception_point::SEND
            } else {
                self.conf_flags & !interception_point::SEND
            },
        }
    }

    /// Record timestamps at the ROUTE point.
    pub fn set_route(self, enabled: bool) -> Self {
        Self {
            conf_flags: if enabled {
                self.conf_flags | interception_point::ROUTE
            } else {
                self.conf_flags & !interception_point::ROUTE
            },
        }
    }

    /// Record timestamps at the RECEIVE point.
    pub fn set_receive(self, enabled: bool) -> Self {
        Self {
            conf_flags: if enabled {
                self.conf_flags | interception_point::RECEIVE
            } else {
                self.conf_flags & !interception_point::RECEIVE
            },
        }
    }

    pub fn build(self) -> zenoh_result::ZResult<TimestampInstrumentation> {
        if self.conf_flags == 0 {
            bail!("Invalid instrumentation config: at least one point must be active");
        }
        Ok(TimestampInstrumentation {
            conf_flags: self.conf_flags,
        })
    }
}

/// A config for timestramp instrumentation. Build via [`TimestampInstrumentationBuilder`].
#[zenoh_macros::unstable]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TimestampInstrumentation {
    conf_flags: u8,
}

#[zenoh_macros::unstable]
impl TimestampInstrumentation {
    /// Returns true if the [`InterceptionPoint`] is activated
    #[zenoh_macros::unstable]
    pub fn is_instrumented(&self, point: InterceptionPoint) -> bool {
        self.conf_flags & u8::from(point) != 0
    }

    /// Returns the raw conf_flags bitmask.
    pub(crate) fn conf_flags(&self) -> u8 {
        self.conf_flags
    }

    /// Initialize from raw flags
    pub(crate) fn try_from_flags(conf_flags: u8) -> zenoh_result::ZResult<Self> {
        if conf_flags == 0 {
            bail!("invalid instrumentation flags: at least one point must be active");
        }
        Ok(Self { conf_flags })
    }
}

/// A single interception record in a timestamp stack.
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampStackRecord {
    point: InterceptionPoint,
    is_custom_ts: bool,
    timestamp: Vec<u8>,
}

impl TimestampStackRecord {
    /// Which interception point this record was captured at.
    #[zenoh_macros::unstable]
    pub fn point(&self) -> InterceptionPoint {
        self.point
    }

    /// Whether the timestamp was produced by a user-defined callback.
    ///
    /// Returns `true` when the timestamp bytes were produced by a user-defined callback,
    /// and `false` when the timestamp is a standard UHLC timestamp.
    #[zenoh_macros::unstable]
    pub fn is_custom(&self) -> bool {
        self.is_custom_ts
    }

    /// Raw timestamp bytes (format defined by the wire protocol).
    #[zenoh_macros::unstable]
    pub fn timestamp(&self) -> &[u8] {
        &self.timestamp
    }
}

/// The timestamp stack carried by a received message.
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampStack {
    instrumentation: TimestampInstrumentation,
    records: Vec<TimestampStackRecord>,
}

#[zenoh_macros::unstable]
impl TimestampStack {
    /// Which interception points were configured for this message.
    pub fn instrumentation(&self) -> TimestampInstrumentation {
        self.instrumentation
    }

    /// The ordered list of interception records.
    pub fn records(&self) -> &[TimestampStackRecord] {
        &self.records
    }

    /// Create a new empty `TimestampStackView` with the given instrumentation config.
    pub(crate) fn new(instrumentation: TimestampInstrumentation) -> Self {
        Self {
            instrumentation,
            records: Vec::new(),
        }
    }
}

/// Push a timestamp interception record onto the stack if the corresponding
/// `conf_flags` bit is set for this interception point.
///
/// This is a no-op if `ext_ts_stack` is `None`, if the flag is not present, or if `runtime` is `None`.
#[cfg(feature = "unstable")]
pub(crate) fn push_ts_interception<const ID: u8, T: IRuntime + ?Sized>(
    ext_ts_stack: &mut Option<TsStackType<ID>>,
    runtime: Option<&T>,
    point: u8,
) {
    let (Some(runtime), Some(ts_stack)) = (runtime, ext_ts_stack) else {
        return;
    };
    if ts_stack.ts_stack.conf_flags & point != 0 {
        let context = TsStackContext {
            zid: runtime.zid(),
            whatami: runtime.whatami(),
            interception_point: point
                .try_into()
                .expect("internal calls should provide valid interception point IDs"),
        };
        let (timestamp, is_custom) = runtime.get_ts_stack_timestamp(context);
        ts_stack.ts_stack.stack.push(Interception {
            flags: point
                | if is_custom {
                    interception_point::IS_CUSTOM_TS
                } else {
                    0
                },
            timestamp,
        });
    }
}

#[cfg(feature = "unstable")]
impl TryFrom<&zenoh_protocol::network::timestamp_stack::TimestampStack> for TimestampStack {
    type Error = zenoh_result::Error;

    fn try_from(
        ts: &zenoh_protocol::network::timestamp_stack::TimestampStack,
    ) -> zenoh_result::ZResult<Self> {
        let mut instance = Self {
            instrumentation: TimestampInstrumentation::try_from_flags(ts.conf_flags)?,
            records: Vec::new(),
        };
        for record in &ts.stack {
            // Skip unknonw/malformed measurement configs
            let point: InterceptionPoint = match record.flags.try_into() {
                Ok(p) => p,
                Err(_) => {
                    tracing::warn!("
                        Skipping instrumentation measurement with unknown or malformed instrumentation flags '{:b}'",
                        record.flags
                    );
                    continue;
                }
            };
            let is_custom = (record.flags & interception_point::IS_CUSTOM_TS) != 0;
            instance.records.push(TimestampStackRecord {
                point,
                is_custom_ts: is_custom,
                timestamp: record.timestamp.clone(),
            });
        }
        Ok(instance)
    }
}

#[cfg(feature = "unstable")]
impl From<&TimestampStack> for zenoh_protocol::network::timestamp_stack::TimestampStack {
    fn from(value: &TimestampStack) -> Self {
        zenoh_protocol::network::timestamp_stack::TimestampStack {
            conf_flags: value.instrumentation.conf_flags,
            stack: value
                .records
                .iter()
                .map(|r| Interception {
                    flags: u8::from(r.point)
                        | if r.is_custom_ts {
                            interception_point::IS_CUSTOM_TS
                        } else {
                            0
                        },
                    timestamp: r.timestamp.clone(),
                })
                .collect(),
        }
    }
}
