//! Usage statistics.

use serde::Deserialize;

use crate::client::Client;
use crate::error::Result;
use crate::transport::ApiRequest;

/// Image storage usage for the account.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Stats {
    /// Stored image counts.
    pub count: Option<StatsCount>,
}

/// Stored image counts.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct StatsCount {
    /// How many images the account may store.
    pub allowed: Option<u64>,
    /// How many images are stored.
    pub current: Option<u64>,
}

impl Client {
    /// Fetches usage statistics.
    ///
    /// The counts update asynchronously: right after uploads or deletes
    /// they can lag behind for a short while (observed live).
    ///
    /// `GET /accounts/{account_id}/images/v1/stats`
    ///
    /// ```no_run
    /// # async fn run(client: cf_images::Client) -> cf_images::Result<()> {
    /// let stats = client.stats().await?;
    /// if let Some(count) = stats.count {
    ///     println!("{:?} of {:?} images used", count.current, count.allowed);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn stats(&self) -> Result<Stats> {
        self.inner.json(ApiRequest::get(["v1", "stats"])).await
    }
}
