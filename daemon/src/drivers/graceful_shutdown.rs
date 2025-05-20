use log::debug;
use tokio::task::JoinSet;

use super::driver::Driver;
use std::sync::Arc;
use tokio::sync::Notify;

pub struct GracefulShutdown {
    drivers: Vec<Arc<dyn Driver>>,
}

impl GracefulShutdown {
    pub fn new() -> Self {
        Self { drivers: vec![] }
    }
}

impl GracefulShutdown {
    pub fn add_driver(&mut self, driver: impl Driver + 'static) {
        self.drivers.push(Arc::new(driver));
    }

    pub async fn watch(mut self, stop_notify: Arc<Notify>) {
        let shutdown = async move {
            tokio::signal::ctrl_c()
                .await
                .expect("graceful shutdown can't install ctrl+c signal handler");
            stop_notify.notify_waiters();
        };

        let mut join_set = JoinSet::new();
        for driver in self.drivers.drain(..) {
            join_set.spawn(async move {
                driver.run().await;
            });
        }

        join_set.spawn(shutdown);
        debug!("graceful shutdown start watching");
        join_set.join_all().await;
    }
}
