use std::{fmt, time::Duration};

use futures::Future;

pub async fn timeout<F, T>(dur: Duration, f: F) -> Result<T, TimeoutError>
where
    F: Future<Output = T>,
{
    async_std::future::timeout(dur, f)
        .await
        .map_err(|err| TimeoutError { inner: err })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutError {
    inner: async_std::future::TimeoutError,
}

impl fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
