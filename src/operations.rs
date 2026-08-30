use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, Weak},
};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

pub use crate::http::{probe_loop, router};

/// Serializes remote mutations for one Sunshine host without blocking
/// unrelated hosts. The registry stores weak references so removed hosts and
/// one-off identifiers do not grow it indefinitely.
#[derive(Clone, Default)]
pub struct HostMutationLocks {
    locks: Arc<Mutex<HashMap<String, Weak<AsyncMutex<()>>>>>,
}

impl HostMutationLocks {
    pub async fn lock(&self, host_id: &str) -> OwnedMutexGuard<()> {
        let lock = {
            let mut locks = recover_lock(&self.locks);
            locks.retain(|_, lock| lock.strong_count() > 0);
            match locks.get(host_id).and_then(Weak::upgrade) {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(AsyncMutex::new(()));
                    locks.insert(host_id.to_string(), Arc::downgrade(&lock));
                    lock
                }
            }
        };
        lock.lock_owned().await
    }

    #[cfg(test)]
    fn entry_count(&self) -> usize {
        recover_lock(&self.locks).len()
    }
}

fn recover_lock<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn same_host_waits_but_different_hosts_run_concurrently() {
        let locks = HostMutationLocks::default();
        let first = locks.lock("host-a").await;

        let same_locks = locks.clone();
        let mut same_host = tokio::spawn(async move {
            let _guard = same_locks.lock("host-a").await;
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut same_host)
                .await
                .is_err()
        );

        tokio::time::timeout(Duration::from_secs(1), locks.lock("host-b"))
            .await
            .expect("a different host must not be blocked");

        drop(first);
        tokio::time::timeout(Duration::from_secs(1), same_host)
            .await
            .expect("the waiter must proceed after the host lock is released")
            .expect("the waiter task must not panic");

        let expired = locks.lock("expired-host").await;
        assert_eq!(locks.entry_count(), 1);
        drop(expired);
        let _current = locks.lock("current-host").await;
        assert_eq!(locks.entry_count(), 1);
    }
}
