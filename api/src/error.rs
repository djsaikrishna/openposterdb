use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid id type: {0}")]
    InvalidIdType(String),

    #[error("id not found: {0}")]
    IdNotFound(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("API error: {0}")]
    Api(#[source] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("image error: {0}")]
    Image(#[from] image::ImageError),

    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),

    #[error("database error: {0}")]
    DbError(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    /// Recover a typed `AppError` from moka's `Arc`-wrapped cache error.
    ///
    /// `moka`'s `try_get_with` returns the init closure's error as
    /// `Arc<AppError>`, and on concurrent calls for the same key it hands
    /// every waiter a *clone* of that `Arc`. Calling `Arc::try_unwrap` then
    /// fails for all waiters (strong count > 1), so naively collapsing the
    /// `Err` arm to [`AppError::Other`] turns a client-facing 404/400 into a
    /// spurious 500 whenever requests race. Reconstruct the client-facing
    /// variants by reference instead so the HTTP status is preserved; only
    /// genuinely-internal variants (which are 500 regardless) collapse to
    /// [`AppError::Other`].
    pub fn from_cached(arc: Arc<AppError>) -> AppError {
        // Sole owner (no concurrent waiters): move the exact error out,
        // preserving full detail for internal variants too.
        match Arc::try_unwrap(arc) {
            Ok(err) => err,
            Err(arc) => match arc.as_ref() {
                AppError::InvalidIdType(msg) => AppError::InvalidIdType(msg.clone()),
                AppError::IdNotFound(msg) => AppError::IdNotFound(msg.clone()),
                AppError::BadRequest(msg) => AppError::BadRequest(msg.clone()),
                AppError::Forbidden(msg) => AppError::Forbidden(msg.clone()),
                AppError::Unauthorized => AppError::Unauthorized,
                other => AppError::Other(other.to_string()),
            },
        }
    }
}

/// Strip the query string from a reqwest error's embedded URL.
///
/// reqwest stores the full request URL — including the query string — inside its
/// errors and renders it in both `Display` and `Debug` (`… for url (…)`). Our
/// upstream clients pass API keys as query parameters (`?api_key=…` / `?apikey=…`
/// for TMDB, OMDb, MDBList, fanart), so an upstream 404/timeout/decode error
/// would otherwise leak the key into the logs. Dropping the query keeps the
/// scheme/host/path — useful for debugging and never secret — while removing the
/// credential. Trakt sends its key as a header, which reqwest never embeds, so it
/// is unaffected.
///
/// Applied at every boundary where a `reqwest::Error` becomes loggable: the
/// `From` impl below (covers every `?`/`error_for_status()?` site) and the
/// direct construction in `services::retry`.
pub(crate) fn redact_url_secrets(mut err: reqwest::Error) -> reqwest::Error {
    if let Some(url) = err.url_mut() {
        url.set_query(None);
    }
    err
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Api(redact_url_secrets(err))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::InvalidIdType(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::IdNotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".into()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".into()),
        };
        if status.is_server_error() {
            tracing::error!(%status, error = %self);
        } else {
            tracing::info!(%status, error = %self);
        }
        (status, axum::Json(json!({ "error": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    use http_body_util::BodyExt;

    fn status_of(err: AppError) -> StatusCode {
        err.into_response().status()
    }

    async fn body_of(err: AppError) -> serde_json::Value {
        let resp = err.into_response();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn invalid_id_type_is_400() {
        assert_eq!(
            status_of(AppError::InvalidIdType("x".into())),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn id_not_found_is_404() {
        assert_eq!(
            status_of(AppError::IdNotFound("x".into())),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn unauthorized_is_401() {
        assert_eq!(status_of(AppError::Unauthorized), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn forbidden_is_403() {
        assert_eq!(
            status_of(AppError::Forbidden("x".into())),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn bad_request_is_400() {
        assert_eq!(
            status_of(AppError::BadRequest("x".into())),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn db_error_is_500() {
        assert_eq!(
            status_of(AppError::DbError("x".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn other_is_500() {
        assert_eq!(
            status_of(AppError::Other("x".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn internal_errors_redact_details() {
        let body = body_of(AppError::DbError("connection refused to 10.0.0.5:5432".into())).await;
        assert_eq!(body["error"], "Internal server error");
    }

    #[tokio::test]
    async fn internal_error_other_redacts_details() {
        let body = body_of(AppError::Other("secret internal info".into())).await;
        assert_eq!(body["error"], "Internal server error");
    }

    #[tokio::test]
    async fn client_errors_preserve_message() {
        let body = body_of(AppError::BadRequest("missing field".into())).await;
        assert_eq!(body["error"], "missing field");
    }

    #[test]
    fn from_cached_sole_owner_preserves_variant() {
        let arc = Arc::new(AppError::IdNotFound("no logo available".into()));
        assert_eq!(
            status_of(AppError::from_cached(arc)),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn from_cached_shared_arc_preserves_client_status() {
        // The concurrent-waiter case: moka hands each waiter a clone of the
        // same Arc, so strong_count > 1 and Arc::try_unwrap fails. This must
        // still yield a 404, not a spurious 500.
        let arc = Arc::new(AppError::IdNotFound("no logo available".into()));
        let _waiter = arc.clone();
        assert_eq!(
            status_of(AppError::from_cached(arc)),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn from_cached_shared_arc_collapses_internal_to_500() {
        let arc = Arc::new(AppError::DbError("connection refused".into()));
        let _waiter = arc.clone();
        assert_eq!(
            status_of(AppError::from_cached(arc)),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn api_error_strips_api_key_from_url() {
        // Regression for the MDBList `?apikey=…` key leaking into logs: reqwest
        // embeds the full request URL (query string included) in its error, and
        // we format `AppError::Api` straight into the logs. Converting through
        // `From<reqwest::Error>` must drop the query so the key can't escape.
        //
        // A local server returns 404 so `error_for_status()` produces exactly the
        // error shape seen in production (`HTTP status … for url (…?apikey=…)`),
        // with no external network so the test stays deterministic.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // An empty router 404s every path — all we need to drive `error_for_status`.
        tokio::spawn(async move {
            let _ = axum::serve(listener, axum::Router::new()).await;
        });

        let url = format!("http://{addr}/imdb/show/tt33306109?apikey=SUPERSECRETKEY");
        let raw = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .expect("request reaches the local server")
            .error_for_status()
            .expect_err("server returns 404");

        // Sanity-check that reqwest really leaks the key by default; otherwise this
        // test could pass vacuously and never catch a regression.
        let unsanitized = format!("{raw}");
        assert!(
            unsanitized.contains("SUPERSECRETKEY"),
            "precondition: reqwest is expected to embed the key in its error, got: {unsanitized}"
        );

        let app: AppError = raw.into();
        let display = format!("{app}");
        let debug = format!("{app:?}");
        for rendered in [&display, &debug] {
            assert!(!rendered.contains("SUPERSECRETKEY"), "api key leaked: {rendered}");
            assert!(!rendered.contains("apikey"), "query string leaked: {rendered}");
        }
        // The non-secret host/path is preserved so failures are still debuggable.
        assert!(display.contains(&addr.to_string()), "host should be kept: {display}");
    }
}
