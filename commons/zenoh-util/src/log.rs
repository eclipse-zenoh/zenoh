//
// Copyright (c) 2024 ZettaScale Technology
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
use std::{env, fmt, str::FromStr};

use tracing::{field::Field, span, Event, Metadata, Subscriber};
use tracing_subscriber::{
    filter::LevelFilter,
    layer::{Context, Filter, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};

const ALREADY_INITIALIZED: &str = "Already initialized logging";

#[non_exhaustive]
#[derive(Debug, Default, Clone, Copy)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    #[default]
    Warn,
    Error,
    Off,
}

#[derive(Debug, Clone)]
pub struct InvalidLogLevel(String);

impl fmt::Display for InvalidLogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid log level {:?}", self.0)
    }
}

impl std::error::Error for InvalidLogLevel {}

impl FromStr for LogLevel {
    type Err = InvalidLogLevel;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "trace" => Self::Trace,
            "debug" => Self::Debug,
            "info" => Self::Info,
            "warn" | "warning" => Self::Warn,
            "error" => Self::Error,
            "off" => Self::Off,
            _ => return Err(InvalidLogLevel(s.into())),
        })
    }
}

/// Initialize zenoh logging using the value of `ZENOH_LOG` environment variable.
///
/// `ZENOH_LOG` is parsed use [`LogLevel::from_str`], possible values are `"debug"`/`"WARN"`/etc.
/// If `ZENOH_LOG` is not provided, a default `WARN` level is used.
///
/// See [`init_logging_with_level`] if you prefer setting the level directly in your code.
/// This function is roughly a wrapper around
/// ```ignore
/// let level = std::env::var("ZENOH_LOG")
///     .map(|var| var.parse().unwrap())
///     .unwrap_or_default();
/// init_logging_with_level(level);
/// ```
///
/// Logs are printed on stdout and are formatted like the following:
/// ```text
/// 2024-06-19T09:46:18.808602Z  INFO main ThreadId(01) zenoh::net::runtime: Using ZID: 1a615ea88fe1dc531a9d8701775d5bee
/// 2024-06-19T09:46:18.814577Z  INFO main ThreadId(01) zenoh::net::runtime::orchestrator: Zenoh can be reached at: tcp/[fe80::1]:58977
/// ```
///
/// # Advanced use
///
/// zenoh logging uses `tracing` crates internally; this function is just a convenient wrapper
/// around `tracing-subscriber`. If you want to control the formatting, or have a finer grain on
/// log filtering, we advise using `tracing-subscriber` directly.
/// However, to make migration and on the fly debugging easier, [`RUST_LOG`][1] environment variable
/// can still be used, and will override `ZENOH_LOG` configuration.
///
/// # Panics
///
/// This function may panic if the following operations fail:
/// - parsing `ZENOH_LOG`/`RUST_LOG` (see [Advanced use](#advanced use)) environment variable
/// - register the global tracing subscriber, because another one has already been registered
/// These errors mostly being the result of unintended use, fast failure is assumed to be more
/// suitable than unexpected behavior, especially as logging should be initialized at program start.
///
/// # Use in tests
///
/// This function should **not** be used in tests, as it would panic as soon as there is more
/// than one test executed in the same unit, because only the first one to execute would be able to
/// register the global tracing subscriber.
/// Moreover, `tracing` and Rust logging in general requires special care about testing because of
/// libtest output capturing; see
/// [`SubscriberBuilder::with_test_writer`](tracing_subscriber::fmt::SubscriberBuilder::with_test_writer).
/// That's why we advise you to use a dedicated library like [`test-log`][3]
/// (with `"tracing"` feature enabled).
///
/// # Memory leak
///
/// [`tracing`] use a global `static` [subscriber](`tracing::subscriber::set_global_default`),
/// which is not deallocated prior to process exiting.
/// Tools such as `valgrind` will then report memory leaks in *still reachable* category.
///
/// However, when `RUST_LOG` is provided (see [Advanced use](#advanced use)),
/// [`tracing_subscriber::EnvFilter`] is then used, and causes also *still reachable* blocks,
/// but also *possibly lost* blocks, which are [known false-positives][2].
/// Those "leaks" can be "suppressed" from `valgrind` report using the following suppression:
/// ```text
/// {
///    zenoh_init_logging
///    Memcheck:Leak
///    ...
///    fun:*zenoh*init_logging*
// }
/// ```
///
/// [1]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives
/// [2]: https://github.com/rust-lang/regex/issues/1205
/// [3]: https://crates.io/crates/test-log
pub fn init_logging() {
    try_init_logging().expect(ALREADY_INITIALIZED);
}

/// Initialize zenoh logging using the provided logging level.
///
/// See [`init_logging`] if you prefer a dynamic setting the level using an environment variable.
/// Logs are printed on stdout and are formatted like the following:
/// ```text
/// 2024-06-19T09:46:18.808602Z  INFO main ThreadId(01) zenoh::net::runtime: Using ZID: 1a615ea88fe1dc531a9d8701775d5bee
/// 2024-06-19T09:46:18.814577Z  INFO main ThreadId(01) zenoh::net::runtime::orchestrator: Zenoh can be reached at: tcp/[fe80::1]:58977
/// ```
///
/// # Advanced use
///
/// zenoh logging uses `tracing` crates internally; this function is just a convenient wrapper
/// around `tracing-subscriber`. If you want to control the formatting, or have a finer grain on
/// log filtering, we advise using `tracing-subscriber` directly.
/// However, to make migration and on the fly debugging easier, [`RUST_LOG`][1] environment variable
/// can still be used, and will override the provided level.
///
/// # Panics
///
/// This function may panic if the following operations fail:
/// - parsing `RUST_LOG` (see [Advanced use](#advanced use)) environment variable
/// - register the global tracing subscriber, because another one has already been registered
/// These errors mostly being the result of unintended use, fast failure is assumed to be more
/// suitable than unexpected behavior, especially as logging should be initialized at program start.
///
/// # Use in tests
///
/// This function should **not** be used in unit tests, as it would panic as soon as there is more
/// than one test executed in the same unit, because only the first one to execute would be able to
/// register the global tracing subscriber.
/// Moreover, `tracing` and Rust logging in general requires special care about testing because of
/// libtest output capturing; see
/// [`SubscriberBuilder::with_test_writer`](tracing_subscriber::fmt::SubscriberBuilder::with_test_writer).
/// That's why we advise you to use a dedicated library like [`test-log`][3]
/// (with `"tracing"` feature enabled).
///
/// # Memory leak
///
/// [`tracing`] use a global `static` [subscriber](`tracing::subscriber::set_global_default`),
/// which is not deallocated prior to process exiting.
/// Tools such as `valgrind` will then report memory leaks in *still reachable* category.
///
/// However, when `RUST_LOG` is provided (see [Advanced use](#advanced use)),
/// [`tracing_subscriber::EnvFilter`] is then used, and causes also *still reachable* blocks,
/// but also *possibly lost* blocks, which are [known false-positives][2].
/// Those "leaks" can be "suppressed" from `valgrind` report using the following suppression:
/// ```text
/// {
///    zenoh_init_logging
///    Memcheck:Leak
///    ...
///    fun:*zenoh*init_logging*
// }
/// ```
///
/// [1]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#directives
/// [2]: https://github.com/rust-lang/regex/issues/1205
/// [3]: https://crates.io/crates/test-log
pub fn init_logging_with_level(level: LogLevel) {
    try_init_logging_with_level(level).expect(ALREADY_INITIALIZED);
}

/// [`init_logging`], but doesn't panic if `tracing` global subscriber is already set.
///
/// This function is mainly meant to be used in plugins, which can be loaded both as static or
/// dynamic libraries. In fact, dynamic library has its own `tracing` global subscriber which need
/// to be initialized, but it would lead to a double initialization for a static library, hence
/// this fallible version.
/// Returns true if the logging was initialized.
pub fn try_init_logging() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let level = env::var("ZENOH_LOG")
        .map(|var| var.parse().expect("invalid ZENOH_LOG"))
        .unwrap_or_default();
    try_init_logging_with_level(level)
}

fn try_init_logging_with_level(
    level: LogLevel,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let builder = tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .with_target(true);
    if let Ok(rust_log) = env::var("RUST_LOG") {
        let env_filter = EnvFilter::builder()
            .parse(rust_log)
            .expect("invalid RUST_LOG");
        builder.with_env_filter(env_filter).try_init()
    } else {
        let level_filter = match level {
            LogLevel::Trace => LevelFilter::TRACE,
            LogLevel::Debug => LevelFilter::DEBUG,
            LogLevel::Info => LevelFilter::INFO,
            LogLevel::Warn => LevelFilter::WARN,
            LogLevel::Error => LevelFilter::ERROR,
            LogLevel::Off => LevelFilter::OFF,
        };
        builder.with_max_level(level_filter).try_init()
    }
}

/// The data extracted from a [`tracing::Event`].
///
/// Span and event fields are flatten into `fields`, except `message` which has its own slot
/// for convenience.
/// While fields are formatted into a string, message keeps its `&dyn fmt::Debug` type to allow
/// using `write!` instead of `format!`.
pub struct LogEvent {
    pub metadata: &'static Metadata<'static>,
    pub message: Option<String>,
    pub fields: Vec<(&'static str, String)>,
}

#[derive(Clone)]
struct SpanFields(Vec<(&'static str, String)>);

struct CallbackLayer<F>(F);

impl<S, F> Layer<S> for CallbackLayer<F>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    F: Fn(LogEvent) + 'static,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut extensions = span.extensions_mut();
        let mut fields = vec![];
        attrs.record(&mut |field: &Field, value: &dyn fmt::Debug| {
            fields.push((field.name(), format!("{value:?}")))
        });
        extensions.insert(SpanFields(fields));
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        let span = ctx.span(id).unwrap();
        let mut extensions = span.extensions_mut();
        let fields = extensions.get_mut::<SpanFields>().unwrap();
        values.record(&mut |field: &Field, value: &dyn fmt::Debug| {
            fields.0.push((field.name(), format!("{value:?}")))
        });
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        tracing::debug!("plop");
        let mut log_event = LogEvent {
            metadata: event.metadata(),
            message: None,
            fields: vec![],
        };
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                let fields = extensions.get::<SpanFields>().unwrap();
                log_event.fields.extend(fields.0.iter().cloned());
            }
        }
        event.record(&mut |field: &Field, value: &dyn fmt::Debug| {
            if field.name() == "message" {
                log_event.message = Some(format!("{value:?}"));
            } else {
                log_event.fields.push((field.name(), format!("{value:?}")))
            }
        });
        self.0(log_event);
    }
}

struct CallbackFilter<E, L> {
    enabled: E,
    max_level_hint: L,
}

impl<S, E, L> Filter<S> for CallbackFilter<E, L>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    E: Fn(&Metadata) -> bool + 'static,
    L: Fn() -> Option<LevelFilter> + 'static,
{
    fn enabled(&self, meta: &Metadata<'_>, _: &Context<'_, S>) -> bool {
        (self.enabled)(meta)
    }
    fn max_level_hint(&self) -> Option<LevelFilter> {
        (self.max_level_hint)()
    }
}

/// Initialize zenoh logging using the provided callbacks.
///
/// This function is mainly meant to be used in zenoh bindings, to provide a bridge between Rust
/// `tracing` implementation and a native logging implementation.
///
/// [`LogEvent`] contains more or less all the data of a `tracing` event.
/// `max_level_hint` will be called only once, and `enabled` once per callsite (span/event).
/// [`tracing::callsite::rebuild_interest_cache`] can be called to reset the cache, and have
/// `max_level_hint`/`enabled` called again.
pub fn init_log_with_callbacks(
    enabled: impl Fn(&Metadata) -> bool + Send + Sync + 'static,
    max_level_hint: impl Fn() -> Option<LevelFilter> + Send + Sync + 'static,
    callback: impl Fn(LogEvent) + Send + Sync + 'static,
) {
    let layer = CallbackLayer(callback).with_filter(CallbackFilter {
        enabled,
        max_level_hint,
    });
    tracing_subscriber::registry().with(layer).init();
}
