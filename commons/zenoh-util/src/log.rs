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
use std::{fmt, thread, thread::ThreadId};

use tracing::{field::Field, span, Event, Metadata, Subscriber};
use tracing_subscriber::{
    layer::{Context, SubscriberExt},
    registry::LookupSpan,
    EnvFilter,
};

/// A utility function to enable the tracing formatting subscriber.
///
/// The [`tracing_subscriber`]` is initialized from the `RUST_LOG` environment variable.
/// If `RUST_LOG` is not set, then logging is not enabled.
///
/// # Safety
///
/// Calling this function initializes a `lazy_static` in the [`tracing`] crate.
/// Such static is not deallocated prior to process exiting, thus tools such as `valgrind`
/// will report a memory leak.
/// Refer to this issue: <https://github.com/tokio-rs/tracing/issues/2069>
pub fn try_init_log_from_env() {
    if let Ok(env_filter) = EnvFilter::try_from_default_env() {
        init_env_filter(env_filter);
    }
}

/// A utility function to enable the tracing formatting subscriber.
///
/// The [`tracing_subscriber`] is initialized from the `RUST_LOG` environment variable.
/// If `RUST_LOG` is not set, then fallback directives are used.
///
/// # Safety
/// Calling this function initializes a `lazy_static` in the [`tracing`] crate.
/// Such static is not deallocated prior to process existing, thus tools such as `valgrind`
/// will report a memory leak.
/// Refer to this issue: <https://github.com/tokio-rs/tracing/issues/2069>
pub fn init_log_from_env_or<S>(fallback: S)
where
    S: AsRef<str>,
{
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(fallback));
    init_env_filter(env_filter);
}

fn init_env_filter(env_filter: EnvFilter) {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .with_target(true);

    let subscriber = subscriber.finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}

pub struct LogRecord {
    pub target: String,
    pub level: tracing::Level,
    pub file: Option<&'static str>,
    pub line: Option<u32>,
    pub thread_id: ThreadId,
    pub thread_name: Option<String>,
    pub message: Option<String>,
    pub attributes: Vec<(&'static str, String)>,
}

#[derive(Clone)]
struct SpanFields(Vec<(&'static str, String)>);

struct Layer<Enabled, Callback> {
    enabled: Enabled,
    callback: Callback,
}

impl<S, E, C> tracing_subscriber::Layer<S> for Layer<E, C>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    E: Fn(&Metadata) -> bool + 'static,
    C: Fn(LogRecord) + 'static,
{
    fn enabled(&self, metadata: &Metadata<'_>, _: Context<'_, S>) -> bool {
        (self.enabled)(metadata)
    }

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
        let thread = thread::current();
        let mut record = LogRecord {
            target: event.metadata().target().into(),
            level: *event.metadata().level(),
            file: event.metadata().file(),
            line: event.metadata().line(),
            thread_id: thread.id(),
            thread_name: thread.name().map(Into::into),
            message: None,
            attributes: vec![],
        };
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope.from_root() {
                let extensions = span.extensions();
                let fields = extensions.get::<SpanFields>().unwrap();
                record.attributes.extend(fields.0.iter().cloned());
            }
        }
        event.record(&mut |field: &Field, value: &dyn fmt::Debug| {
            if field.name() == "message" {
                record.message = Some(format!("{value:?}"));
            } else {
                record.attributes.push((field.name(), format!("{value:?}")))
            }
        });
        (self.callback)(record);
    }
}

pub fn init_log_with_callback(
    enabled: impl Fn(&Metadata) -> bool + Send + Sync + 'static,
    callback: impl Fn(LogRecord) + Send + Sync + 'static,
) {
    let subscriber = tracing_subscriber::registry().with(Layer { enabled, callback });
    let _ = tracing::subscriber::set_global_default(subscriber);
}

#[cfg(feature = "test")]
// Used to verify memory leaks for valgrind CI.
// `EnvFilter` internally uses a static reference that is not cleaned up yielding to false positive in valgrind.
// This function enables logging without calling `EnvFilter` for env configuration.
pub fn init_log_test() {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .with_target(true);

    let subscriber = subscriber.finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}
