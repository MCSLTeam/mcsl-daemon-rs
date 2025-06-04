use futures::future::BoxFuture;
use kanal::{bounded_async, AsyncReceiver, AsyncSender};
use log::error;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::time::{self, Instant};

// 处理器函数类型
type ProcessorFn<I, O> = dyn Fn(I) -> BoxFuture<'static, O> + Send + Sync + 'static;

pub struct TaskPool<I: Send + 'static, O: Send + 'static> {
    task_tx: AsyncSender<I>,
    task_rx: AsyncReceiver<I>,
    output_tx: tokio::sync::mpsc::UnboundedSender<O>,
    active_workers: Arc<AtomicUsize>,
    total_workers: Arc<AtomicUsize>,
    max_workers: usize,
    processor: Arc<ProcessorFn<I, O>>,
    idle_timeout: Duration,
}

impl<I: Send + 'static, O: Send + 'static> TaskPool<I, O> {
    /// 创建任务池
    pub fn new<F>(
        processor: F,
        max_workers: usize,
        pending_tasks: usize,
        output_tx: tokio::sync::mpsc::UnboundedSender<O>,
        idle_timeout_secs: u64,
    ) -> Self
    where
        F: Fn(I) -> BoxFuture<'static, O> + Send + Sync + 'static,
    {
        let (task_tx, task_rx) = bounded_async(pending_tasks);

        let processor = Arc::new(processor) as Arc<ProcessorFn<I, _>>;
        let idle_timeout = Duration::from_secs(idle_timeout_secs);

        Self {
            task_tx,
            task_rx,
            output_tx,
            active_workers: Arc::new(AtomicUsize::new(0)),
            total_workers: Arc::new(AtomicUsize::new(0)),
            max_workers,
            processor,
            idle_timeout,
        }
    }

    fn ensure_workers(&self) {
        let active_workers = self.active_workers.load(Ordering::Acquire);
        let total_workers = self.total_workers.load(Ordering::Acquire);
        if active_workers == total_workers
            && total_workers < self.max_workers
            && self
                .total_workers
                .compare_exchange(
                    total_workers,
                    total_workers + 1,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_ok()
        {
            self.spawn_worker();
        }
    }

    fn spawn_worker(&self) {
        let processor = self.processor.clone();
        let idle_timeout = self.idle_timeout;
        let output_tx = self.output_tx.clone();
        let task_rx = self.task_rx.clone();
        let active_workers = self.active_workers.clone();
        let total_workers = self.total_workers.clone();
        tokio::spawn({
            async move {
                let mut last_active = Instant::now();
                loop {
                    match time::timeout(idle_timeout, task_rx.recv()).await {
                        Ok(Ok(task)) => {
                            active_workers.fetch_add(1, Ordering::SeqCst);
                            let result = processor(task).await;
                            if output_tx.send(result).is_err() {
                                error!("Failed to send result, output channel closed");
                                active_workers.fetch_sub(1, Ordering::SeqCst);
                                break;
                            } else {
                                last_active = Instant::now();
                                active_workers.fetch_sub(1, Ordering::SeqCst);
                            }
                        }
                        Ok(Err(_)) => break,
                        Err(_) => {
                            if last_active.elapsed() >= idle_timeout {
                                break;
                            }
                        }
                    }
                }
                total_workers.fetch_sub(1, Ordering::SeqCst);
            }
        });
    }

    #[allow(dead_code)]
    pub async fn submit(&self, task: I) -> Result<(), kanal::SendError<I>> {
        self.ensure_workers();
        // 确保新的worker已经处于监听状态
        tokio::task::yield_now().await;
        self.task_tx.send(task).await
    }

    pub async fn try_submit(&self, task: I) -> Result<(), kanal::TrySendError<I>> {
        self.ensure_workers();
        // 确保新的worker已经处于监听状态
        tokio::task::yield_now().await;
        self.task_tx.try_send(task)
    }

    #[allow(dead_code)]
    pub fn active_worker_count(&self) -> usize {
        self.active_workers.load(Ordering::Relaxed)
    }

    #[allow(dead_code)]
    pub fn total_worker_count(&self) -> usize {
        self.total_workers.load(Ordering::Relaxed)
    }
}

impl<I: Send + 'static, O: Send + 'static> Drop for TaskPool<I, O> {
    fn drop(&mut self) {
        let _ = self.task_tx.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;
    use tokio::time::{sleep, Duration};

    // 定义 dummy_processor，返回 BoxFuture
    fn dummy_processor(input: i32) -> BoxFuture<'static, i32> {
        Box::pin(async move {
            sleep(Duration::from_millis(100)).await; // 模拟异步工作
            input * 2
        })
    }

    /// 测试基本功能：提交任务并接收结果
    #[tokio::test]
    async fn test_task_pool_basic() {
        let (tx, mut rx) = unbounded_channel();
        let pool = TaskPool::new(dummy_processor, 2, 1, tx, 1);

        pool.submit(1).await.unwrap();
        let result = rx.recv().await.unwrap();
        assert_eq!(result, 2); // 输入 1，期望输出 2
    }

    /// 测试并发性：提交多个任务，确保并发处理
    #[tokio::test]
    async fn test_task_pool_concurrency() {
        let (tx, mut rx) = unbounded_channel();
        let pool = TaskPool::new(dummy_processor, 2, 1, tx, 1);

        pool.submit(1).await.unwrap();
        pool.submit(2).await.unwrap();

        let mut results = Vec::new();
        for _ in 0..2 {
            results.push(rx.recv().await.unwrap());
        }
        results.sort(); // 结果顺序可能不定，排序后比较
        assert_eq!(results, vec![2, 4]); // 输入 1 和 2，期望输出 2 和 4
    }

    /// 测试空闲超时：工作者在空闲一段时间后减少
    #[tokio::test]
    async fn test_task_pool_idle_timeout() {
        let (tx, rx) = unbounded_channel();
        let pool = TaskPool::new(dummy_processor, 2, 1, tx, 1);

        pool.submit(1).await.unwrap();
        sleep(Duration::from_millis(150)).await; // 等待任务完成
        assert_eq!(pool.total_worker_count(), 1); // 工作者应该存在

        sleep(Duration::from_secs(2)).await; // 等待空闲超时（假设超时为 1 秒）
        assert_eq!(pool.total_worker_count(), 0); // 工作者应该减少到 0
    }

    /// 测试错误处理：输出通道关闭时工作者退出
    #[tokio::test]
    async fn test_task_pool_output_channel_closed() {
        let (tx, rx) = unbounded_channel();
        let pool = TaskPool::new(dummy_processor, 2, 1, tx, 1);

        drop(rx); // 关闭输出通道
        pool.submit(1).await.unwrap();

        sleep(Duration::from_millis(150)).await; // 等待工作者反应
        assert_eq!(pool.active_worker_count(), 0); // 工作者应该退出
    }

    /// 测试资源清理：TaskPool 丢弃后工作者退出
    #[tokio::test]
    async fn test_task_pool_drop() {
        let (tx, rx) = unbounded_channel();
        let pool = TaskPool::new(dummy_processor, 2, 1, tx, 1);

        pool.submit(1).await.unwrap();
        sleep(Duration::from_millis(150)).await; // 等待任务完成
        assert_eq!(pool.active_worker_count(), 0);
        assert!(pool.total_worker_count() > 0); // 工作者应该存在

        drop(pool); // 丢弃 TaskPool
        sleep(Duration::from_millis(150)).await; // 等待工作者退出
    }
}
