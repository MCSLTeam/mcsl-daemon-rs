use log::trace;
use tokio::task::JoinSet;

use super::driver::{Driver, StopToken};
use std::sync::Arc;
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

    pub async fn watch(mut self) {
        let tokens: Vec<StopToken> = self.drivers.iter().map(|d| d.stop_token()).collect();
        let shutdown = async move {
            tokio::signal::ctrl_c()
                .await
                .expect("graceful shutdown can't install ctrl+c signal handler");
            tokens.into_iter().for_each(|t| t.notify_one());
        };

        let mut join_set = JoinSet::new();
        for driver in self.drivers.drain(..) {
            join_set.spawn(async move {
                driver.run().await;
            });
        }

        join_set.spawn(shutdown);
        trace!("graceful shutdown start watching");
        join_set.join_all().await;
    }
}
