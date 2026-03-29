use std::sync::Arc;

use crate::error::AppError;
use crate::services::retry::{self, TRAKT_RETRY};
use serde::Deserialize;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct TraktClient {
    client_id: Arc<Zeroizing<String>>,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct TraktRatingsResponse {
    pub rating: f64,
    pub votes: u64,
}

impl TraktClient {
    pub fn new(client_id: String, http: reqwest::Client) -> Self {
        Self {
            client_id: Arc::new(Zeroizing::new(client_id)),
            http,
        }
    }

    /// Build a GET request with the required Trakt API headers.
    ///
    /// Trakt's API requires `Content-Type: application/json` even on GET
    /// requests (no body). This is a quirk of their API spec.
    fn request(&self, url: &str) -> reqwest::RequestBuilder {
        self.http
            .get(url)
            .header("Content-Type", "application/json")
            .header("trakt-api-version", "2")
            .header("trakt-api-key", self.client_id.as_str())
    }

    pub async fn get_movie_rating(&self, imdb_id: &str) -> Result<TraktRatingsResponse, AppError> {
        let url = format!("https://api.trakt.tv/movies/{imdb_id}/ratings");
        let resp = retry::send_with_retry(&TRAKT_RETRY, || self.request(&url).send())
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn get_show_rating(&self, imdb_id: &str) -> Result<TraktRatingsResponse, AppError> {
        let url = format!("https://api.trakt.tv/shows/{imdb_id}/ratings");
        let resp = retry::send_with_retry(&TRAKT_RETRY, || self.request(&url).send())
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    pub async fn get_episode_rating(
        &self,
        show_id: &str,
        season: u32,
        episode: u32,
    ) -> Result<TraktRatingsResponse, AppError> {
        let url = format!(
            "https://api.trakt.tv/shows/{show_id}/seasons/{season}/episodes/{episode}/ratings"
        );
        let resp = retry::send_with_retry(&TRAKT_RETRY, || self.request(&url).send())
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }
}
