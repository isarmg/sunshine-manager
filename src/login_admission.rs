use std::{
    collections::HashMap,
    hash::Hash,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

use crate::{auth::token_hash, error::AppError};

const WINDOW: Duration = Duration::from_secs(5 * 60);
const SOURCE_ATTEMPTS: u32 = 20;
const ACCOUNT_ATTEMPTS: u32 = 10;
const MAX_BUCKETS: usize = 4_096;
const ARGON2_CONCURRENCY: usize = 2;

#[derive(Clone)]
pub struct LoginAdmission {
    buckets: Arc<Mutex<Buckets>>,
    argon2: Arc<Semaphore>,
    window: Duration,
    source_attempts: u32,
    account_attempts: u32,
    max_buckets: usize,
}

#[derive(Default)]
struct Buckets {
    sources: HashMap<String, Bucket>,
    accounts: HashMap<Vec<u8>, Bucket>,
}

#[derive(Clone, Copy)]
struct Bucket {
    started_at: Instant,
    attempts: u32,
}

impl Default for LoginAdmission {
    fn default() -> Self {
        Self::with_limits(
            WINDOW,
            SOURCE_ATTEMPTS,
            ACCOUNT_ATTEMPTS,
            MAX_BUCKETS,
            ARGON2_CONCURRENCY,
        )
    }
}

impl LoginAdmission {
    fn with_limits(
        window: Duration,
        source_attempts: u32,
        account_attempts: u32,
        max_buckets: usize,
        argon2_concurrency: usize,
    ) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(Buckets::default())),
            argon2: Arc::new(Semaphore::new(argon2_concurrency.max(1))),
            window,
            source_attempts: source_attempts.max(1),
            account_attempts: account_attempts.max(1),
            max_buckets: max_buckets.max(2),
        }
    }

    pub async fn admit(
        &self,
        source: Option<IpAddr>,
        normalized_account: &str,
    ) -> Result<(), AppError> {
        let now = Instant::now();
        let source = source
            .map(|address| address.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let account = token_hash(normalized_account);
        let mut buckets = self.buckets.lock().await;
        prune(&mut buckets.sources, now, self.window);
        prune(&mut buckets.accounts, now, self.window);

        let source_retry = retry_after(
            &buckets.sources,
            &source,
            now,
            self.window,
            self.source_attempts,
        );
        let account_retry = retry_after(
            &buckets.accounts,
            &account,
            now,
            self.window,
            self.account_attempts,
        );
        if let Some(retry_after) = source_retry.into_iter().chain(account_retry).max() {
            return Err(AppError::TooManyRequests { retry_after });
        }

        record(
            &mut buckets.sources,
            source,
            now,
            self.window,
            self.max_buckets,
        );
        record(
            &mut buckets.accounts,
            account,
            now,
            self.window,
            self.max_buckets,
        );
        Ok(())
    }

    pub async fn clear_account(&self, normalized_account: &str) {
        self.buckets
            .lock()
            .await
            .accounts
            .remove(&token_hash(normalized_account));
    }

    pub async fn argon2_permit(&self) -> Result<OwnedSemaphorePermit, AppError> {
        tokio::time::timeout(Duration::from_secs(2), self.argon2.clone().acquire_owned())
            .await
            .map_err(|_| AppError::TooManyRequests { retry_after: 1 })?
            .map_err(|_| AppError::Internal(anyhow::anyhow!("Argon2 admission closed")))
    }
}

fn retry_after<K: Eq + Hash>(
    buckets: &HashMap<K, Bucket>,
    key: &K,
    now: Instant,
    window: Duration,
    limit: u32,
) -> Option<u64> {
    let bucket = buckets.get(key)?;
    if bucket.attempts < limit {
        return None;
    }
    let remaining = window.saturating_sub(now.saturating_duration_since(bucket.started_at));
    Some(
        remaining
            .as_secs()
            .saturating_add(u64::from(remaining.subsec_nanos() > 0))
            .max(1),
    )
}

fn record<K: Clone + Eq + Hash>(
    buckets: &mut HashMap<K, Bucket>,
    key: K,
    now: Instant,
    window: Duration,
    max_buckets: usize,
) {
    if let Some(bucket) = buckets.get_mut(&key) {
        if now.saturating_duration_since(bucket.started_at) >= window {
            *bucket = Bucket {
                started_at: now,
                attempts: 1,
            };
        } else {
            bucket.attempts = bucket.attempts.saturating_add(1);
        }
        return;
    }
    if buckets.len() >= max_buckets
        && let Some(oldest) = buckets
            .iter()
            .min_by_key(|(_, bucket)| bucket.started_at)
            .map(|(key, _)| key.clone())
    {
        buckets.remove(&oldest);
    }
    buckets.insert(
        key,
        Bucket {
            started_at: now,
            attempts: 1,
        },
    );
}

fn prune<K: Eq + Hash>(buckets: &mut HashMap<K, Bucket>, now: Instant, window: Duration) {
    buckets.retain(|_, bucket| now.saturating_duration_since(bucket.started_at) < window);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn source_and_account_budgets_are_bounded_and_success_clears_only_the_account() {
        let admission = LoginAdmission::with_limits(Duration::from_secs(60), 3, 2, 4, 1);
        let source = Some("192.0.2.10".parse().unwrap());

        admission.admit(source, "first@example.com").await.unwrap();
        admission.admit(source, "first@example.com").await.unwrap();
        assert!(matches!(
            admission.admit(source, "first@example.com").await,
            Err(AppError::TooManyRequests { retry_after }) if retry_after > 0
        ));

        admission.clear_account("first@example.com").await;
        admission.admit(source, "first@example.com").await.unwrap();
        assert!(matches!(
            admission.admit(source, "second@example.com").await,
            Err(AppError::TooManyRequests { .. })
        ));
    }
}
