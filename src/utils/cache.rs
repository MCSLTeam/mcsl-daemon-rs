use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// 使用 async trait 方法
pub trait AsyncFetchable: Clone {
    async fn fetch() -> Self;
}

#[derive(Clone)]
enum TimedCacheState<T>
where
    T: Clone,
{
    Cached((Instant, T)),
    None,
}

#[derive(Clone)]
pub struct AsyncTimedCache<T: AsyncFetchable> {
    state: Arc<Mutex<TimedCacheState<T>>>,
    duration: Duration,
}

impl<T: AsyncFetchable> AsyncTimedCache<T> {
    pub fn new(duration: Duration) -> Self {
        Self {
            state: Arc::new(Mutex::new(TimedCacheState::None)),
            duration,
        }
    }

    pub async fn get(&self) -> T {
        let mut state_guard = self.state.lock().await;
        match &*state_guard {
            TimedCacheState::Cached((last_modified, value))
                if last_modified.elapsed() < self.duration =>
            {
                value.clone()
            }
            _ => {
                let value = T::fetch().await;
                *state_guard = TimedCacheState::Cached((Instant::now(), value.clone()));
                value
            }
        }
    }
}

// 为String类型实现AsyncFetchable特征（示例）
impl AsyncFetchable for String {
    async fn fetch() -> Self {
        tokio::time::sleep(Duration::from_secs(1)).await;
        "Hello, world!".to_string()
    }
}

#[tokio::test]
async fn test_async_cache() {
    let cache = AsyncTimedCache::<String>::new(Duration::from_secs(2));
    let value = cache.get().await;
    assert_eq!(value, "Hello, world!");
    tokio::time::sleep(Duration::from_secs(2)).await;
    let value = cache.get().await;
    assert_eq!(value, "Fetched data");
}
