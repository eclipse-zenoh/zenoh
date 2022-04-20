use std::time::{Duration, Instant};

pub async fn sleep(dur: Duration) {
    async_std::task::sleep(dur).await;
}

pub async fn sleep_until(deadline: Instant) {
    let remaining = deadline.saturating_duration_since(Instant::now());
    sleep(remaining).await;
}
