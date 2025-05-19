use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

type SyncCallback<T> = Arc<dyn Fn(T) + Send + Sync>;
type AsyncCallback<T> = Arc<dyn Fn(T) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
/// 生成唯一 ID
fn generate_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn log_panic(panic_value: Box<dyn Any + Send>) -> String {
    if let Some(msg) = panic_value.downcast_ref::<String>() {
        format!("Panic message: {}", msg)
    } else if let Some(msg) = panic_value.downcast_ref::<&str>() {
        format!("Panic message: {}", msg)
    } else {
        // 尝试使用 Debug 打印，但只能在具体类型上
        format!("Panic value type_id={:?}", (*panic_value).type_id())
    }
}

/// **通用的同步/异步回调类型**
#[derive(Clone)]
enum CallbackFn<T>
where
    T: Clone,
{
    Sync(SyncCallback<T>),
    Async(AsyncCallback<T>),
}

#[derive(Clone, Default)]
pub enum TListener {
    #[default]
    Simple,
    Count(Arc<AtomicUsize>),
    Once(Arc<AtomicBool>),
}

impl TListener {
    pub fn count(count: usize) -> Self {
        TListener::Count(Arc::new(AtomicUsize::new(count)))
    }

    pub fn once() -> Self {
        TListener::Once(Arc::new(AtomicBool::new(false)))
    }
}

/// **通用的监听器包装器**
#[derive(Clone)]
struct ListenerWrapper<T>
where
    T: Clone,
{
    id: u64,
    callback: CallbackFn<T>,
    t_callback: TListener,
    is_removed: Arc<AtomicBool>,
}

impl<T: Clone> ListenerWrapper<T> {
    pub fn new(id: u64, t_callback: TListener, callback: CallbackFn<T>) -> Self {
        Self {
            id,
            t_callback,
            callback,
            is_removed: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// 消耗一次listener wrapper，返回true表示次数耗尽，需要remove listener
fn consume_wrapper<T: Clone>(wrapper: &ListenerWrapper<T>) -> bool {
    // 前置检查：若已被标记为移除，直接返回 false
    if wrapper.is_removed.load(Ordering::Relaxed) {
        return false;
    }

    let should_remove = match &wrapper.t_callback {
        TListener::Simple => false,
        TListener::Count(counter) => {
            let old = counter.fetch_sub(1, Ordering::SeqCst);
            old == 1 // 旧值为 1 时触发移除(执行一次后); event_decl中已经过滤了counter初值为0的情况
        }
        TListener::Once(consumed) => {
            let was_consumed = consumed.swap(true, Ordering::SeqCst);
            !was_consumed // 如果是首次消费，返回 true（需要移除）
        }
    };

    if should_remove {
        // 原子标记为已移除
        wrapper.is_removed.store(true, Ordering::Relaxed);
        true
    } else {
        false
    }
}

/// TODO 使用map替代vec以实现 begin_invoke_wrapper的就地删除
#[macro_export]
macro_rules! event_decl {
    ($event_name:ident, $($arg_name:ident : $arg_type:ty),*) => {


        pub struct $event_name {
            listeners: Arc<std::sync::Mutex<Vec<ListenerWrapper<($($arg_type),*)>>>>,
        }

        impl $event_name {
            pub fn new() -> Self {
                Self {
                    listeners: Arc::new(std::sync::Mutex::new(Vec::new()))
                }
            }

            pub fn add_sync_listener<F>(&self, callback: F, t_callback: TListener) -> Option<u64>
            where
                F: Fn($($arg_type),*) + Send + Sync + 'static,
            {
                if let TListener::Count(counter) = &t_callback {
                    if counter.load(Ordering::Relaxed) == 0 {
                        return None;
                    }
                }

                let mut listeners = self.listeners.lock().unwrap();
                let id = generate_id();
                listeners.push(ListenerWrapper::new(
                    id,
                    t_callback,
                    CallbackFn::Sync(Arc::new(move |args| {
                        let ($($arg_name),*) = args;
                        callback($($arg_name),*);
                    }))
                ));
                Some(id)
            }

            pub fn add_async_listener<F, Fut>(&self, callback: F, t_callback: TListener) -> Option<u64>
            where
                F: Fn($($arg_type),*) -> Fut + Send + Sync + 'static,
                Fut: Future<Output = ()> + Send + 'static,
            {
                if let TListener::Count(counter) = &t_callback {
                    if counter.load(Ordering::Relaxed) == 0 {
                        return None;
                    }
                }

                let mut listeners = self.listeners.lock().unwrap();
                let id = generate_id();
                listeners.push(ListenerWrapper::new(
                    id,
                    t_callback,
                    CallbackFn::Async(Arc::new(move |args| {
                        let ($($arg_name),*) = args;
                        Box::pin(callback($($arg_name),*))
                    }))
                ));
                Some(id)
            }

            fn _remove_listener(
                listeners: Arc<std::sync::Mutex<Vec<ListenerWrapper<($($arg_type),*)>>>>,
                id: u64
            ) -> bool {
                let mut guard = listeners.lock().unwrap();
                if let Some(pos) = guard.iter().position(|w| w.id == id) {
                    let removed_wrapper = guard.remove(pos);
                    // 确保标记一致性
                    removed_wrapper.is_removed.store(true, Ordering::Relaxed);
                    true
                } else {
                    false
                }
            }

            pub fn remove_listener(&self, id: u64) -> bool {
                Self::_remove_listener(self.listeners.clone(), id)
            }

            pub fn invoke(&self, $($arg_name: $arg_type),*)
            {
                let listeners_snapshot = {
                    let guard = self.listeners.lock().unwrap();
                    guard.clone()
                };
                let listeners = self.listeners.clone();

                for wrapper in listeners_snapshot.iter() {
                    // 跳过已标记移除的条目
                    if wrapper.is_removed.load(Ordering::Relaxed) {
                        continue;
                    }

                    // 消费回调并判断是否需要移除
                    // if begin_invoke_wrapper(&wrapper) {
                    //     Self::_remove_listener(listeners.clone(), wrapper.id);
                    //     continue;
                    // }
                    let should_remove = consume_wrapper(&wrapper);

                    // 正常处理回调逻辑
                    match &wrapper.callback {
                        CallbackFn::Sync(cb) => cb(($($arg_name.clone()),*)),
                        CallbackFn::Async(cb) => {
                            let fut = cb(($($arg_name.clone()),*));
                            tokio::spawn(fut);
                        }
                    }

                    if should_remove {
                        Self::_remove_listener(listeners.clone(), wrapper.id);
                    }
                }
            }


            pub async fn invoke_async(&self, $($arg_name: $arg_type),*) {
                let listeners_snapshot = {
                    let guard = self.listeners.lock().unwrap();
                    guard.clone()
                };
                let listeners = self.listeners.clone();
                let mut set = tokio::task::JoinSet::new();

                for wrapper in listeners_snapshot.iter() {
                    // 跳过已标记移除的条目
                    if wrapper.is_removed.load(Ordering::Relaxed) {
                        continue;
                    }

                    // 消耗callback
                    let should_remove = consume_wrapper(&wrapper);

                    // 执行Callback
                    match &wrapper.callback {
                        CallbackFn::Sync(cb) => {
                            cb(($($arg_name.clone()),*)); // TODO 非Copy类型的clone处理, 去除非必要的clone() <==(建议)
                        }
                        CallbackFn::Async(cb) => {
                            let fut = cb(($($arg_name.clone()),*)); // TODO 非Copy类型的clone处理, 去除非必要的clone() <==(建议)
                            set.spawn(fut);
                        }
                    }

                    if should_remove {
                        Self::_remove_listener(listeners.clone(), wrapper.id);
                    }
                }
                set.join_all().await;
            }
        }

        impl Default for $event_name {
            fn default() -> Self {
                Self::new()
            }
        }

        unsafe impl Sync for $event_name {}
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // TODO限制invoke时async cb的并发度
    event_decl!(TestEvent, num: i32, msg: &'static str, data: String);

    #[tokio::test]
    async fn test_sync_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        event.add_sync_listener(
            move |num, msg, data| {
                assert_eq!(num, 42);
                assert_eq!(msg, "Hello");
                assert_eq!(data, "World".to_string());
                counter_clone.fetch_add(1, Ordering::Relaxed);
            },
            TListener::default(),
        );

        event.invoke(42, "Hello", "World".to_string());
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        event.invoke_async(42, "Hello", "World".to_string()).await;
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_async_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        event.add_async_listener(
            move |num, msg, data| {
                let counter_clone = Arc::clone(&counter_clone);
                async move {
                    assert_eq!(num, 42);
                    assert_eq!(msg, "Hello");
                    assert_eq!(data, "World".to_string());
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                }
            },
            TListener::default(),
        );
        event.invoke(42, "Hello", "World".to_string());
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        event.invoke_async(42, "Hello", "World".to_string()).await;
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_remove_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let listener_id = event
            .add_sync_listener(
                move |_, _, _| {
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                },
                TListener::default(),
            )
            .unwrap();

        assert!(event.remove_listener(listener_id));
        event.invoke(10, "Test", "World".to_string());
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_multiple_listeners() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..5 {
            let counter_clone = Arc::clone(&counter);
            event.add_sync_listener(
                move |_, _, _| {
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                },
                TListener::default(),
            );
        }

        event.invoke(10, "Test", "World".to_string());
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }

    #[tokio::test]
    async fn test_add_count_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);
        let counter_clone2 = Arc::clone(&counter);

        event.add_sync_listener(
            move |_, _, _| {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            },
            TListener::count(50),
        );

        event.add_async_listener(
            move |_, _, _| {
                let counter_clone = Arc::clone(&counter_clone2);
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                }
            },
            TListener::count(25),
        );

        for _ in 0..100 {
            event.invoke(10, "Test", "World".to_string());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(counter.load(Ordering::Relaxed), 50 + 25);
    }

    #[tokio::test]
    async fn test_add_once_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);
        let counter_clone2 = Arc::clone(&counter);

        event.add_sync_listener(
            move |_, _, _| {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            },
            TListener::once(),
        );

        event.add_async_listener(
            move |_, _, _| {
                let counter_clone = Arc::clone(&counter_clone2);
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    counter_clone.fetch_add(1, Ordering::Relaxed);
                }
            },
            TListener::once(),
        );

        for _ in 0..50 {
            event.invoke(10, "Test", "World".to_string());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(counter.load(Ordering::Relaxed), 1 + 1);
    }
}
