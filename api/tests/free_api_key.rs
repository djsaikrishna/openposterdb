mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::TestAppOptions;
use http_body_util::BodyExt;
use tower::ServiceExt;

fn json_body(json: serde_json::Value) -> Body {
    Body::from(json.to_string())
}

/// Helper: enable or disable the free API key setting via admin API.
async fn set_free_api_key_enabled(app: &axum::Router, token: &str, enabled: bool) {
    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "image_source": "t",
            "free_api_key_enabled": enabled
        })))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// --- GET /api/admin/settings includes free_api_key_enabled ---

#[tokio::test]
async fn settings_defaults_free_api_key_disabled() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/admin/settings")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["free_api_key_enabled"], false);
}

#[tokio::test]
async fn settings_enable_free_api_key_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    set_free_api_key_enabled(&app, &token, true).await;

    let req = Request::builder()
        .uri("/api/admin/settings")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["free_api_key_enabled"], true);
}

#[tokio::test]
async fn settings_disable_free_api_key_and_read_back() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Enable then disable
    set_free_api_key_enabled(&app, &token, true).await;
    set_free_api_key_enabled(&app, &token, false).await;

    let req = Request::builder()
        .uri("/api/admin/settings")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["free_api_key_enabled"], false);
}

// --- GET /api/auth/status includes free_api_key_enabled ---

#[tokio::test]
async fn auth_status_includes_free_api_key_enabled_default_false() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/auth/status")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["free_api_key_enabled"], false);
}

#[tokio::test]
async fn auth_status_reflects_enabled_free_api_key() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    set_free_api_key_enabled(&app, &token, true).await;

    let req = Request::builder()
        .uri("/api/auth/status")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["free_api_key_enabled"], true);
}

// --- Poster route with free API key ---

#[tokio::test]
async fn free_key_rejected_when_disabled() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/poster-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn free_key_accepted_when_enabled() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    set_free_api_key_enabled(&app, &token, true).await;

    // The TMDB call will fail since we use a fake API key, but the
    // response should NOT be 401 — the free key itself is valid.
    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/poster-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "free key should not return 401 when enabled"
    );
}

#[tokio::test]
async fn free_key_rejected_after_disabling() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Enable
    set_free_api_key_enabled(&app, &token, true).await;

    // Verify it works
    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/poster-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);

    // Disable
    set_free_api_key_enabled(&app, &token, false).await;

    // Should be rejected again
    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/poster-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- key-login with free API key always fails ---

#[tokio::test]
async fn key_login_rejects_free_api_key() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Enable the free key for poster serving
    set_free_api_key_enabled(&app, &token, true).await;

    // Attempt to login with the free key — should fail since it has no DB row
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/key-login")
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({"api_key": "t0-free-rpdb"})))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- Free key does not track last_used ---

#[tokio::test]
async fn free_key_does_not_track_last_used() {
    let (app, state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    set_free_api_key_enabled(&app, &token, true).await;

    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/poster-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let _res = app.oneshot(req).await.unwrap();

    // pending_last_used should be empty — free key doesn't insert into it
    assert!(
        state.pending_last_used.is_empty(),
        "free API key should not track last_used"
    );
}

// --- FREE_KEY_ENABLED env var override ---

#[tokio::test]
async fn env_var_true_forces_free_key_on() {
    let (app, _state) = common::setup_test_app_with_options(TestAppOptions {
        free_key_enabled: Some(true),
        ..Default::default()
    })
    .await;

    // Free key should work without any DB toggle
    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/poster-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "free key should be accepted when FREE_KEY_ENABLED=true"
    );
}

#[tokio::test]
async fn env_var_false_forces_free_key_off() {
    let (app, _state) = common::setup_test_app_with_options(TestAppOptions {
        free_key_enabled: Some(false),
        ..Default::default()
    })
    .await;
    let token = common::setup_admin(&app).await;

    // Try to enable via DB — should be silently ignored
    set_free_api_key_enabled(&app, &token, true).await;

    // Free key should still be rejected
    let req = Request::builder()
        .uri("/t0-free-rpdb/imdb/poster-default/tt0111161.jpg")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "free key should be rejected when FREE_KEY_ENABLED=false"
    );
}

#[tokio::test]
async fn env_var_sets_locked_in_settings_response() {
    let (app, _state) = common::setup_test_app_with_options(TestAppOptions {
        free_key_enabled: Some(true),
        ..Default::default()
    })
    .await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/admin/settings")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["free_api_key_enabled"], true);
    assert_eq!(json["free_api_key_locked"], true);
}

#[tokio::test]
async fn no_env_var_not_locked_in_settings_response() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    let req = Request::builder()
        .uri("/api/admin/settings")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["free_api_key_locked"], false);
}

// --- GET /api/free-key/settings (public, reflects global defaults) ---

#[tokio::test]
async fn free_key_settings_rejected_when_disabled() {
    let (app, _state) = common::setup_test_app().await;

    let req = Request::builder()
        .uri("/api/free-key/settings")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn free_key_settings_returns_defaults_when_enabled() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    set_free_api_key_enabled(&app, &token, true).await;

    let req = Request::builder()
        .uri("/api/free-key/settings")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // Default render settings are present and enum-coded the same as the image API.
    assert_eq!(json["ratings_order"], "mal,imdb,lb,rt,mc,rta,tmdb,trakt,mdblist,ebert");
    assert_eq!(json["ratings_exclude"], "");
    assert_eq!(json["image_source"], "t");
    assert_eq!(json["lang"], "en");
    // Operational flags from the admin response must NOT leak onto the public endpoint.
    assert!(json.get("free_api_key_locked").is_none());
    assert!(json.get("fanart_available").is_none());
}

#[tokio::test]
async fn free_key_settings_reflects_admin_changes() {
    let (app, _state) = common::setup_test_app().await;
    let token = common::setup_admin(&app).await;

    // Enable the free key and set a custom global order + poster badge style.
    let req = Request::builder()
        .method("PUT")
        .uri("/api/admin/settings")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(json_body(serde_json::json!({
            "image_source": "t",
            "free_api_key_enabled": true,
            "ratings_order": "imdb,tmdb",
            "ratings_exclude": "rt",
            "poster_badge_style": "h",
        })))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // The public endpoint should reflect those values (cache invalidated on update).
    let req = Request::builder()
        .uri("/api/free-key/settings")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ratings_order"], "imdb,tmdb");
    assert_eq!(json["ratings_exclude"], "rt");
    assert_eq!(json["poster_badge_style"], "h");
}

#[tokio::test]
async fn free_key_settings_reflects_env_override() {
    let (app, _state) = common::setup_test_app_with_options(TestAppOptions {
        free_key_enabled: Some(true),
        ..Default::default()
    })
    .await;

    // No DB toggle needed — the env override forces the free key on.
    let req = Request::builder()
        .uri("/api/free-key/settings")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn env_var_auth_status_reflects_override() {
    let (app, _state) = common::setup_test_app_with_options(TestAppOptions {
        free_key_enabled: Some(true),
        ..Default::default()
    })
    .await;

    let req = Request::builder()
        .uri("/api/auth/status")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["free_api_key_enabled"], true);
}
