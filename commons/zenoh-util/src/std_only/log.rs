use tracing_subscriber::EnvFilter;

/// This is an utility function to enable the tracing formatting subscriber from
/// the `RUST_LOG` environment variable.
///
/// # Safety
/// Calling this function initializes a `lazy_static` in the `tracing` crate
/// such static is not deallocated prior to process existing, thus tools such as `valgrind`
/// will report a memory leak.
/// Refer to this issue: https://github.com/tokio-rs/tracing/issues/2069
pub fn init_log() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("z=info"));

    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_level(true)
        .with_target(true);

    let subscriber = subscriber.finish();
    let _ = tracing::subscriber::set_global_default(subscriber);
}
