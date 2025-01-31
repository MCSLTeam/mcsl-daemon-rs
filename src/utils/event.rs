use std::any::Any;
use std::sync::Arc;
use std::pin::Pin;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
enum CallbackFn<T> where T: Clone{
    Sync(SyncCallback<T>),
    Async(AsyncCallback<T>),
}

#[derive(Clone)]
#[derive(Default)]
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
struct ListenerWrapper<T> where T: Clone {
    id: u64,
    callback: CallbackFn<T>,
    t_callback: TListener,
    is_removed : Arc<AtomicBool>,
}

impl<T:Clone> ListenerWrapper<T> {
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
            old == 0 // 旧值为 0 时触发移除
        }
        TListener::Once(consumed) => {
            consumed.fetch_or(true, Ordering::SeqCst) // 第二次尝试调用时移除
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

#[macro_export]
macro_rules! event_decl {
    ($event_name:ident, $($arg_name:ident : $arg_type:ty),*) => {


        pub struct $event_name {
            listeners: std::sync::Arc<std::sync::Mutex<Vec<ListenerWrapper<($($arg_type),*)>>>>,
        }

        impl $event_name {
            pub fn new() -> Self {
                Self {
                    listeners: std::sync::Arc::new(std::sync::Mutex::new(Vec::new()))
                }
            }

            pub fn add_sync_listener<F>(&self, callback: F, t_callback: TListener) -> u64
            where
                F: Fn($($arg_type),*) + Send + Sync + 'static,
            {
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
                id
            }

            pub fn add_async_listener<F, Fut>(&self, callback: F, t_callback: TListener) -> u64
            where
                F: Fn($($arg_type),*) -> Fut + Send + Sync + 'static,
                Fut: Future<Output = ()> + Send + 'static,
            {
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
                id
            }
            
            fn _remove_listener(
                listeners: Arc<Mutex<Vec<ListenerWrapper<($($arg_type),*)>>>>,
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
            
                tokio::spawn(async move {
                    for wrapper in listeners_snapshot.iter() {
                        // 跳过已标记移除的条目
                        if wrapper.is_removed.load(Ordering::Relaxed) {
                            continue;
                        }
                        
                        // 消费回调并判断是否需要移除
                        if consume_wrapper(&wrapper) {
                            Self::_remove_listener(listeners.clone(), wrapper.id);
                        } else {
                            // 正常处理回调逻辑
                            match &wrapper.callback {
                                CallbackFn::Sync(cb) => cb(($($arg_name.clone()),*)),
                                CallbackFn::Async(cb) => {
                                    let fut = cb(($($arg_name.clone()),*));
                                    let _ = tokio::spawn(fut).await;
                                }
                            }
                        }
                    }
                });
            }
            
            /// 仅在需要Debug时才调用，release下不要出现此函数。因为包含了panic::catch_unwind，以及需要保留一些调试符号来显示unwind信息
            pub fn invoke_safe(&self, $($arg_name: $arg_type),*) {
                let listeners_snapshot = {
                    let guard = self.listeners.lock().unwrap();
                    guard.clone()
                };
                let listeners = self.listeners.clone();
        
                tokio::spawn(async move {
                    let name = stringify!($event_name).to_string();
                    let mut join_set = tokio::task::JoinSet::new();
        
                    for wrapper in listeners_snapshot.iter() {
                        // 跳过已标记移除的条目
                        if wrapper.is_removed.load(Ordering::Relaxed) {
                            continue;
                        }
                        
                        // 消耗callback
                        if consume_wrapper(wrapper) {
                            Self::_remove_listener(listeners.clone(), wrapper.id);
                            continue;
                        }
                        
                        // 执行callback
                        match &wrapper.callback {
                            CallbackFn::Sync(cb) => {
                                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    cb(($($arg_name.clone()),*)); // TODO 非Copy类型的clone处理, 去除非必要的clone() <==(建议)
                                }));
                                if let Err(panic) = result {
                                    log::warn!("EventSystem: {} sync callback(id={}) panicked; {}", name, wrapper.id, log_panic(panic));
                                }
                            }
                            CallbackFn::Async(cb) => {
                                let fut = cb(($($arg_name.clone()),*)); // TODO 非Copy类型的clone处理, 去除非必要的clone() <==(建议)
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
                    if consume_wrapper(wrapper) {
                        Self::_remove_listener(listeners.clone(), wrapper.id);
                        continue;
                    }
                    
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

// mod to_expand{
//     use super::*;
//     event_decl!(TestEvent, num: i32, msg: &'static str, data: Arc<String>);
// }

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    event_decl!(TestEvent, num: i32, msg: &'static str, data: String);

    #[tokio::test]
    async fn test_sync_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        event.add_sync_listener(move |num, msg, data| {
            assert_eq!(num, 42);
            assert_eq!(msg, "Hello");
            assert_eq!(data, "World".to_string());
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }, TListener::default());

        event.invoke(42, "Hello", "World".to_string());
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

        event.add_sync_listener(move |num, msg, data| {
            assert_eq!(num, 42);
            assert_eq!(msg, "Hello");
            assert_eq!(data, "World".to_string());
            counter_clone.fetch_add(1, Ordering::Relaxed);
            panic!("Test panic");
        }, TListener::default());

        event.add_sync_listener(move |num, msg, data| {
            assert_eq!(num, 42);
            assert_eq!(msg, "Hello");
            assert_eq!(data, "World".to_string());
            counter_clone2.fetch_add(1, Ordering::Relaxed);
        }, TListener::default());

        event.invoke_safe(42, "Hello", "World".to_string());
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

        event.add_async_listener(move |num, msg, data| {
            let counter_clone = Arc::clone(&counter_clone);
            async move {
                assert_eq!(num, 42);
                assert_eq!(msg, "Hello");
                assert_eq!(data, "World".to_string());
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                counter_clone.fetch_add(1, Ordering::Relaxed);
                panic!("Test panic async");
            }
        }, TListener::default());

        event.add_async_listener(move |num, msg, data| {
            let counter_clone2 = Arc::clone(&counter_clone2);
            async move {
                assert_eq!(num, 42);
                assert_eq!(msg, "Hello");
                assert_eq!(data, "World".to_string());

                counter_clone2.fetch_add(1, Ordering::Relaxed);
            }
        }, TListener::default());

        event.invoke_safe(42, "Hello", "World".to_string());
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }


    #[tokio::test]
    async fn test_async_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        event.add_async_listener(move |num, msg,data| {
            let counter_clone = Arc::clone(&counter_clone);
            async move {
                assert_eq!(num, 42);
                assert_eq!(msg, "Hello");
                assert_eq!(data, "World".to_string());
                counter_clone.fetch_add(1, Ordering::Relaxed);
            }
        }, TListener::default());

        event.invoke_async(42, "Hello", "World".to_string()).await;

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_remove_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let listener_id = event.add_sync_listener(move |_, _, _| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }, TListener::default());

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
            event.add_sync_listener(move |_, _, _| {
                counter_clone.fetch_add(1, Ordering::Relaxed);
            }, TListener::default());
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
        
        event.add_sync_listener(move |_, _, _| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }, TListener::count(50));
        
        for _ in 0..50{
            event.invoke(10, "Test", "World".to_string());
        }
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 50);

        for _ in 0..50{
            event.invoke(10, "Test", "World".to_string());
        }
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 50);
    }

    #[tokio::test]
    async fn test_add_once_listener() {
        let event = TestEvent::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        event.add_sync_listener(move |_, _, _| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        }, TListener::once());

        
        event.invoke(10, "Test", "World".to_string());
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 1);

        event.invoke(10, "Test", "World".to_string());
        tokio::task::yield_now().await;
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}

