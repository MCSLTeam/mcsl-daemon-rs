use std::any::Any;
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;

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
enum CallbackFn<T> {
    Sync(SyncCallback<T>),
    Async(AsyncCallback<T>),
}

/// **通用的监听器包装器**
#[derive(Clone)]
struct ListenerWrapper<T> {
    id: u64,
    callback: CallbackFn<T>,
}

#[macro_export]
macro_rules! event_decl {
    ($event_name:ident, $($arg_name:ident : $arg_type:ty),*) => {
        
        
        pub struct $event_name {
            listeners: std::sync::Mutex<Vec<ListenerWrapper<($($arg_type),*)>>>,
        }

        impl $event_name {
            pub fn new() -> Self {
                Self {
                    listeners: std::sync::Mutex::new(Vec::new())
                }
            }

            pub fn add_sync_listener<F>(&self, callback: F) -> u64
            where
                F: Fn($($arg_type),*) + Send + Sync + 'static,
            {
                let mut listeners = self.listeners.lock().unwrap();
                let id = generate_id();
                listeners.push(ListenerWrapper {
                    id,
                    callback: CallbackFn::Sync(Arc::new(move |args| {
                        let ($($arg_name),*) = args;
                        callback($($arg_name),*);
                    })),
                });
                id
            }

            pub fn add_async_listener<F, Fut>(&self, callback: F) -> u64
            where
                F: Fn($($arg_type),*) -> Fut + Send + Sync + 'static,
                Fut: Future<Output = ()> + Send + 'static,
            {
                let mut listeners = self.listeners.lock().unwrap();
                let id = generate_id();
                listeners.push(ListenerWrapper {
                    id,
                    callback: CallbackFn::Async(Arc::new(move |args| {
                        let ($($arg_name),*) = args;
                        Box::pin(callback($($arg_name),*))
                    })),
                });
                id
            }

            pub fn remove_listener(&self, id: u64) -> bool {
                let mut listeners = self.listeners.lock().unwrap();
                if let Some(pos) = listeners.iter().position(|wrapper| wrapper.id == id) {
                    listeners.remove(pos);
                    true
                } else {
                    false
                }
            }
            
            pub fn invoke(&self, $($arg_name: $arg_type),*) {
                let listeners = {
                    let guard = self.listeners.lock().unwrap();
                    guard.clone()
                };
            
                tokio::spawn(async move {
                    for wrapper in listeners.iter() {
                        let _wrapper_id = wrapper.id;
                        
                        match &wrapper.callback {
                            CallbackFn::Sync(cb) => cb(($($arg_name),*)),
                            CallbackFn::Async(cb) => {
                                let fut = cb(($($arg_name),*));
                                let _ = tokio::spawn(fut).await;
                            }
                        }
                        
                    }
                });
            }

            pub fn invoke_safe(&self, $($arg_name: $arg_type),*) {
                let listeners = {
                    let guard = self.listeners.lock().unwrap();
                    guard.clone()
                };
            
                tokio::spawn(async move {
                    let name = stringify!($event_name).to_string();
                    let mut join_set = tokio::task::JoinSet::new();
                    
                    for wrapper in listeners.iter() {
                        let wrapper_id = wrapper.id;
                        
                        match &wrapper.callback {
                            CallbackFn::Sync(cb) => {
                                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    cb(($($arg_name),*));
                                }));
                                if let Err(panic) = result {
                                    log::warn!("EventSystem: {} sync callback(id={}) panicked; {}", name, wrapper_id, log_panic(panic));
                                }
                            }
                            CallbackFn::Async(cb) => {
                                let fut = cb(($($arg_name),*));
                                join_set.spawn(fut);
                            }
                        }
                        
                    }
                    
                    while let Some(res) = join_set.join_next().await{
                        if let Err(join_err) = res{
                            if join_err.is_panic(){
                                log::warn!("EventSystem: {} async callback panicked; {}", name, log_panic(join_err.into_panic()));
                            } else if join_err.is_cancelled() {
                                log::warn!("EventSystem: {} async callback cancelled", name);
                            }
                        }
                    }
                });
            }

            pub async fn invoke_async(&self, $($arg_name: $arg_type),*) {
                let listeners = {
                    let guard = self.listeners.lock().unwrap();
                    guard.clone()
                };
                let mut set = tokio::task::JoinSet::new();

                for wrapper in listeners.iter() {
                    match &wrapper.callback {
                        CallbackFn::Sync(cb) => {
                            cb(($($arg_name),*));
                        }
                        CallbackFn::Async(cb) => {
                            let fut = cb(($($arg_name),*));
                            set.spawn(fut);
                        }
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    event_decl!(TestEvent, num: i32, msg: &'static str);

    #[tokio::test]
    async fn test_sync_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        event.add_sync_listener(move |num, msg| {
            assert_eq!(num, 42);
            assert_eq!(msg, "Hello");
            counter_clone.fetch_add(1, Ordering::Relaxed);
        });

        event.invoke(42, "Hello");
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
    
    #[tokio::test]
    async fn test_panic_listener() {
        let _ = pretty_env_logger::try_init();
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);
        let counter_clone2 = Arc::clone(&counter);

        event.add_sync_listener(move |num, msg| {
            assert_eq!(num, 42);
            assert_eq!(msg, "Hello");
            counter_clone.fetch_add(1, Ordering::Relaxed);
            panic!("Test panic");
        });

        event.add_sync_listener(move |num, msg| {
            assert_eq!(num, 42);
            assert_eq!(msg, "Hello");
            counter_clone2.fetch_add(1, Ordering::Relaxed);
        });

        event.invoke_safe(42, "Hello");
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn test_panic_async_listener() {
        let _ = pretty_env_logger::try_init();
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);
        let counter_clone2 = Arc::clone(&counter);

        event.add_async_listener(move |num, msg| {
            let counter_clone = Arc::clone(&counter_clone);
            async move {
                assert_eq!(num, 42);
                assert_eq!(msg, "Hello");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                counter_clone.fetch_add(1, Ordering::Relaxed);
                panic!("Test panic async");
            }
        });

        event.add_async_listener(move |num, msg| {
            let counter_clone2 = Arc::clone(&counter_clone2);
            async move {
                assert_eq!(num, 42);
                assert_eq!(msg, "Hello");
                
                counter_clone2.fetch_add(1, Ordering::Relaxed);
            }
        });

        event.invoke_safe(42, "Hello");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }
    

    #[tokio::test]
    async fn test_async_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        event.add_async_listener(move |num, msg| {
            let counter_clone = Arc::clone(&counter_clone);
            async move {
                assert_eq!(num, 42);
                assert_eq!(msg, "Hello");
                counter_clone.fetch_add(1, Ordering::Relaxed);
            }
        });

        event.invoke_async(42, "Hello").await;

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_remove_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let listener_id = event.add_sync_listener(move |_, _| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        });

        assert!(event.remove_listener(listener_id));
        event.invoke(10, "Test");
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_multiple_listeners() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));

        for _ in 0..5 {
            let counter_clone = Arc::clone(&counter);
            event.add_sync_listener(move |_, _| {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            });
        }

        event.invoke(10, "Test");
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 5);
    }
}

