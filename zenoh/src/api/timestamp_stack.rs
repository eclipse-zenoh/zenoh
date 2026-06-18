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
pub struct TimestampContext {
    /// The Zenoh ID of the current node.
    pub zid: ZenohId,
    /// The mode of the current node (router, peer, or client).
    pub whatami: WhatAmI,
}

#[zenoh_macros::unstable]
pub(crate) type GetTimestampCallback = Box<dyn Fn(TimestampContext) -> Vec<u8> + Send + Sync>;

/// Identifies which interception point a timestamp record was captured at.
///
/// The three points represent the lifecycle of a message traveling through
/// the Zenoh topology:
///
/// - [`Send`](InterceptionPoint::Send): triggered when the message leaves
///   the publishing/replying application.
/// - [`Route`](InterceptionPoint::Route): triggered at each intermediate
///   node (router or peer) that forwards the message.
/// - [`Receive`](InterceptionPoint::Receive): triggered when the message
///   is delivered to the subscribing/querying application.
///
/// # Example
///
/// ```
/// use std::convert::TryFrom;
/// use zenoh::timestamp_stack::InterceptionPoint;
///
/// assert!(InterceptionPoint::try_from(1u8).is_ok());
/// assert!(InterceptionPoint::try_from(0xFFu8).is_err());
/// ```
#[zenoh_macros::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterceptionPoint {
    /// Timestamp recorded when the message leaves the publishing or replying
    /// node — the application layer's "send" side.
    Send,
    /// Timestamp recorded when the message passes through a routing node
    /// (router or peer forwarding layer).
    Route,
    /// Timestamp recorded when the message arrives at a subscribing or
    /// queryable node — the application layer's "receive" side.
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

/// Builder for [`TimestampInstrumentation`] instances.
///
/// Collects which interception points should be instrumented and produces
/// an immutable [`TimestampInstrumentation`] via [`build`](Self::build).
///
/// At least one interception point must be enabled; calling [`build`](Self::build)
/// with none enabled returns an error.
///
/// # Example
///
/// ```
/// use zenoh::timestamp_stack::{
///     InterceptionPoint, TimestampInstrumentationBuilder,
/// };
///
/// let instr = TimestampInstrumentationBuilder::new()
///     .set_send(true)
///     .set_route(true)
///     .set_receive(true)
///     .build()
///     .unwrap();
///
/// assert!(instr.is_instrumented(InterceptionPoint::Send));
/// assert!(instr.is_instrumented(InterceptionPoint::Route));
/// assert!(instr.is_instrumented(InterceptionPoint::Receive));
/// ```
#[zenoh_macros::unstable]
#[derive(Debug, Default, Clone, Copy)]
pub struct TimestampInstrumentationBuilder {
    conf_flags: u8,
}

#[zenoh_macros::unstable]
impl TimestampInstrumentationBuilder {
    /// Creates a new builder with no interception points enabled.
    ///
    /// At least one point must be enabled via [`set_send`](Self::set_send),
    /// [`set_route`](Self::set_route), or [`set_receive`](Self::set_receive)
    /// before calling [`build`](Self::build).
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable timestamp recording at the [`Send`](InterceptionPoint::Send)
    /// interception point.
    ///
    /// The SEND point is triggered when a message is about to leave the
    /// publishing or replying node.
    pub fn set_send(self, enabled: bool) -> Self {
        Self {
            conf_flags: if enabled {
                self.conf_flags | interception_point::SEND
            } else {
                self.conf_flags & !interception_point::SEND
            },
        }
    }

    /// Enable or disable timestamp recording at the [`Route`](InterceptionPoint::Route)
    /// interception point.
    ///
    /// The ROUTE point is triggered on every intermediate node (router or peer)
    /// that forwards the message through its routing layer.
    pub fn set_route(self, enabled: bool) -> Self {
        Self {
            conf_flags: if enabled {
                self.conf_flags | interception_point::ROUTE
            } else {
                self.conf_flags & !interception_point::ROUTE
            },
        }
    }

    /// Enable or disable timestamp recording at the [`Receive`](InterceptionPoint::Receive)
    /// interception point.
    ///
    /// The RECEIVE point is triggered when a message is delivered to a
    /// subscribing or queryable application.
    pub fn set_receive(self, enabled: bool) -> Self {
        Self {
            conf_flags: if enabled {
                self.conf_flags | interception_point::RECEIVE
            } else {
                self.conf_flags & !interception_point::RECEIVE
            },
        }
    }

    /// Consumes the builder and produces a [`TimestampInstrumentation`].
    ///
    /// Returns an error if no interception points were enabled.
    pub fn build(self) -> zenoh_result::ZResult<TimestampInstrumentation> {
        if self.conf_flags == 0 {
            bail!("Invalid instrumentation config: at least one point must be active");
        }
        Ok(TimestampInstrumentation {
            conf_flags: self.conf_flags,
        })
    }
}

/// A config for timestamp instrumentation. Build via [`TimestampInstrumentationBuilder`].
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

/// A timestamp measurement, either UHLC or the result of a custom user-defined callback.
///
/// The semantics depend on whether a [custom timestamp callback](GetTimestampCallback)
/// was registered on the session that performed the measurement:
///
/// - **UHLC**: the default — a Zenoh UHLC hybrid logical clock timestamp.
/// - **Custom**: arbitrary bytes produced by the user-defined callback.
///
/// # Example
///
/// ```
/// use zenoh::timestamp_stack::InstrumentationTimestamp;
///
/// // InstrumentationTimestamp is usually constructed automatically.
/// let ts = InstrumentationTimestamp::Custom(b"my_format".to_vec());
/// // Pattern match to handle both variants:
/// match &ts {
///     InstrumentationTimestamp::UHLC(uhlc_ts) => println!("UHLC: {:?}", uhlc_ts),
///     InstrumentationTimestamp::Custom(bytes) => println!("Custom({} bytes)", bytes.len()),
/// }
/// ```
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstrumentationTimestamp {
    /// A Zenoh [UHLC] hybrid logical clock timestamp.
    ///
    /// [UHLC]: https://github.com/eclipse-zenoh/uhlc-rs
    UHLC(uhlc::Timestamp),
    /// A timestamp in a format defined by the user's
    /// [custom timestamp callback](GetTimestampCallback).
    Custom(Vec<u8>),
}

/// A single interception record in a timestamp stack.
///
/// Each record captures the [`InterceptionPoint`] where the measurement was
/// taken and the [`InstrumentationTimestamp`] that was recorded. Records are
/// obtained by iterating over [`TimestampStack::records`].
///
/// # Example
///
/// ```
/// use zenoh::timestamp_stack::{
///     InstrumentationTimestamp, InterceptionPoint,
/// };
///
/// // Records are typically obtained from a received TimestampStack.
/// // This shows how to inspect a hypothetical record:
/// fn inspect(record: &zenoh::timestamp_stack::TimestampStackRecord) {
///     println!("Point: {:?}", record.point());
///     if record.is_custom() {
///         println!("Custom timestamp: {:?}", record.timestamp());
///     } else {
///         println!("UHLC timestamp: {:?}", record.timestamp());
///     }
/// }
/// ```
#[zenoh_macros::unstable]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampStackRecord {
    point: InterceptionPoint,
    timestamp: InstrumentationTimestamp,
}

impl TimestampStackRecord {
    /// Which interception point this record was captured at.
    #[zenoh_macros::unstable]
    pub fn point(&self) -> InterceptionPoint {
        self.point
    }

    /// Whether the timestamp was produced by a user-defined callback.
    ///
    /// Returns `true` when the timestamp was produced by a user-defined callback,
    /// and `false` when the timestamp is a standard UHLC timestamp.
    #[zenoh_macros::unstable]
    pub fn is_custom(&self) -> bool {
        matches!(self.timestamp, InstrumentationTimestamp::Custom(_))
    }

    /// Raw timestamp bytes (format defined by the wire protocol).
    #[zenoh_macros::unstable]
    pub fn timestamp(&self) -> &InstrumentationTimestamp {
        &self.timestamp
    }
}

/// The timestamp stack carried by a received message.
///
/// When timestamp instrumentation is enabled on a publication or query, the
/// resulting message carries a [`TimestampStack`] that accumulates records at
/// each interception point it passes through. The stack can be accessed from
/// a [`Sample`] via [`Sample::timestamp_stack`] or from a [`Query`] via
/// [`Query::timestamp_stack`].
///
/// Use [`instrumentation`](Self::instrumentation) to inspect which points were
/// configured, and [`records`](Self::records) to iterate over the ordered list
/// of timestamps collected along the message path.
///
/// # Example
///
/// ```no_run
/// # #[tokio::main]
/// # async fn main() {
/// use zenoh::timestamp_stack::{
///     InstrumentationTimestamp, InterceptionPoint, TimestampInstrumentationBuilder,
/// };
///
/// let session = zenoh::open(zenoh::Config::default()).await.unwrap();
/// let subscriber = session.declare_subscriber("key/expr").await.unwrap();
/// let publisher = session.declare_publisher("key/expr").await.unwrap();
///
/// let instr = TimestampInstrumentationBuilder::new()
///     .set_send(true)
///     .set_receive(true)
///     .build()
///     .unwrap();
///
/// publisher
///     .put("payload")
///     .timestamp_instrumentation(instr)
///     .await
///     .unwrap();
///
/// let sample = subscriber.recv_async().await.unwrap();
/// if let Some(stack) = sample.timestamp_stack() {
///     // Check which points were configured
///     let config = stack.instrumentation();
///     assert!(config.is_instrumented(InterceptionPoint::Send));
///
///     // Iterate records in traversal order
///     for record in stack.records() {
///         match record.point() {
///             InterceptionPoint::Send => println!("sent at {:?}", record.timestamp()),
///             InterceptionPoint::Receive => println!("received at {:?}", record.timestamp()),
///             _ => {}
///         }
///     }
/// }
/// # }
/// ```
///
/// [`Sample`]: crate::sample::Sample
/// [`Sample::timestamp_stack`]: crate::sample::Sample::timestamp_stack
/// [`Query`]: crate::query::Query
/// [`Query::timestamp_stack`]: crate::query::Query::timestamp_stack
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
/// This is a no-op if `ext_ts_stack` is `None`, if the flag is not present,
/// or if `get_runtime` returns `None`.
///
/// The `get_runtime` closure is only invoked when `ext_ts_stack` is `Some`
/// and the relevant `conf_flags` bit is set, avoiding unnecessary work
/// (e.g. upgrading a weak runtime reference) in the common no-op case.
#[cfg(feature = "unstable")]
pub(crate) fn push_ts_interception<const ID: u8, T: IRuntime + ?Sized, R, F>(
    ext_ts_stack: &mut Option<TsStackType<ID>>,
    get_runtime: F,
    point: u8,
) where
    R: std::ops::Deref<Target = T>,
    F: FnOnce() -> Option<R>,
{
    let Some(ts_stack) = ext_ts_stack else {
        return;
    };
    if ts_stack.ts_stack.conf_flags & point != 0 {
        let Some(runtime) = get_runtime() else {
            return;
        };
        let context = TimestampContext {
            zid: runtime.zid(),
            whatami: runtime.whatami(),
        };
        let (timestamp, is_custom) = runtime.get_ts_stack_timestamp(context);
        if timestamp.is_empty() {
            // Skip writing empty Vec
            return;
        }
        if ts_stack.ts_stack.stack.len() >= zenoh_protocol::network::timestamp_stack::MAX_STACK_SIZE
        {
            // Avoid producing an invalid frame that will be rejected by the codec decoder.
            return;
        }
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
            // Skip unknown/malformed measurement configs
            let point: InterceptionPoint = match record.flags.try_into() {
                Ok(p) => p,
                Err(_) => {
                    // FIXME: find a way to hold unknown interception points
                    //        (required for forward-compatibility)
                    tracing::warn!(
                        "Skipping instrumentation measurement with unknown or malformed instrumentation flags '{:b}'",
                        record.flags
                    );
                    continue;
                }
            };
            let is_custom = (record.flags & interception_point::IS_CUSTOM_TS) != 0;
            let timestamp = match is_custom {
                true => InstrumentationTimestamp::Custom(record.timestamp.clone()),
                false => {
                    use zenoh_buffers::reader::HasReader;
                    use zenoh_codec::{RCodec, Zenoh080};

                    let mut reader = (&record.timestamp).reader();
                    let Ok(ts): Result<uhlc::Timestamp, _> = Zenoh080.read(&mut reader) else {
                        tracing::warn!(
                            "Skipping instrumentation measurement with malformed uhlc timestamp"
                        );
                        continue;
                    };
                    InstrumentationTimestamp::UHLC(ts)
                }
            };
            instance
                .records
                .push(TimestampStackRecord { point, timestamp });
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
                        | if matches!(r.timestamp, InstrumentationTimestamp::Custom(_)) {
                            interception_point::IS_CUSTOM_TS
                        } else {
                            0
                        },
                    timestamp: match &r.timestamp {
                        InstrumentationTimestamp::UHLC(ts) => {
                            use zenoh_codec::{WCodec, Zenoh080};

                            let mut buf = Vec::new();
                            Zenoh080
                                .write(&mut buf, ts)
                                .expect("serializing valid UHLC timestamp should not fail");
                            buf
                        }
                        InstrumentationTimestamp::Custom(ts) => ts.clone(),
                    },
                })
                .collect(),
        }
    }
}
