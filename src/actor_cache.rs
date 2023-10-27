use std::{
    collections::HashMap,
    sync::Arc,
    time::Instant,
};

use futures::Future;
use lru::LruCache;
use tokio::sync::{Mutex, oneshot};

use crate::activitypub::Actor;
use crate::error::Error;


#[allow(clippy::type_complexity)]
#[derive(Clone)]
pub struct ActorCache {
    cache: Arc<Mutex<LruCache<String, Result<Arc<Actor>, Error>>>>,
    queues: Arc<Mutex<HashMap<String, Vec<oneshot::Sender<Result<Arc<Actor>, Error>>>>>>,
}

impl Default for ActorCache {
    fn default() -> Self {
        ActorCache {
            cache: Arc::new(Mutex::new(
                LruCache::new(std::num::NonZeroUsize::new(64).unwrap())
            )),
            queues: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ActorCache {
    pub async fn get<F, R>(&self, k: &str, f: F) -> Result<Arc<Actor>, Error>
    where
        F: (FnOnce() -> R) + Send + 'static,
        R: Future<Output = Result<Actor, Error>> + Send,
    {
        let begin = Instant::now();

        let mut lru = self.cache.lock().await;
        if let Some(v) = lru.get(k) {
            return v.clone();
        }
        drop(lru);

        let (tx, rx) = oneshot::channel();
        let mut new = false;
        let mut queues = self.queues.lock().await;
        let queue = queues.entry(k.to_string())
            .or_insert_with(|| {
                new = true;
                Vec::with_capacity(1)
            });
        queue.push(tx);
        drop(queues);

        if new {
            let k = k.to_string();
            let cache = self.cache.clone();
            let queues = self.queues.clone();
            tokio::spawn(async move {
                let result = f().await
                    .map(Arc::new);

                let mut lru = cache.lock().await;
                lru.put(k.clone(), result.clone());
                drop(lru);

                let mut queues = queues.lock().await;
                let queue = queues.remove(&k)
                    .expect("queues.remove");
                let queue_len = queue.len();
                let mut notified = 0usize;
                for tx in queue.into_iter() {
                    if let Ok(()) = tx.send(result.clone()) {
                        notified += 1;
                    }
                }

                let end = Instant::now();
                tracing::info!("Notified {notified}/{queue_len} endpoint verifications for actor {k} in {:?}", end - begin);
            });
        }

        rx.await.unwrap()
    }
}
