use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

enum TimedCacheState<T>
where
    T: Clone,
{
    Cached((Instant, T)),
    None,
}

#[derive(Clone)]
pub struct AsyncTimedCache<T, F>
where
    T: Clone,
    F: Fn() -> Pin<Box<dyn Future<Output = T>>> + 'static,
{
    state: Arc<Mutex<TimedCacheState<T>>>,
    func: F,
    duration: Duration,
}

impl<T, F> AsyncTimedCache<T, F>
where
    T: Clone,
    F: Fn() -> Pin<Box<dyn Future<Output = T>>> + 'static,
{
    #[allow(dead_code)]
    pub fn new(func: F, duration: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(TimedCacheState::None)),
            func,
            duration,
        }
    }

    #[allow(dead_code)]
    pub async fn get(&self) -> T {
        let mut state_guard = self.state.lock().await;
        if let TimedCacheState::Cached((ref last_modified, ref value)) = *state_guard {
            if last_modified.elapsed() < self.duration {
                return value.clone();
            }
        }
        let value = (self.func)().await;
        let new_state = TimedCacheState::Cached((Instant::now(), value.clone()));
        *state_guard = new_state;
        value
    }
}
