use std::future::Future;

pub use async_std::{self, main, test};

pub use spawn::*;
mod spawn;

pub use task::*;
mod task;

pub use time::*;
mod time;

pub use sleep::*;
mod sleep;

pub fn block_on<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    async_std::task::block_on(future)
}

pub async fn yield_now() {
    async_std::task::yield_now().await
}
