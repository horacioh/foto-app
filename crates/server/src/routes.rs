use crate::AppState;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use tower_http::trace::TraceLayer;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .nest("/v1", v1())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn v1() -> Router<AppState> {
    Router::new()
        // accounts & keys (phase 1)
        .route("/accounts", post(planned(1)))
        .route("/accounts/me/keys", get(planned(1)).put(planned(1)))
        .route("/devices", get(planned(1)).post(planned(1)))
        .route("/devices/{id}", axum::routing::delete(planned(1)))
        // albums & feeds (phase 1 for `library`, phase 2 for sharing)
        .route("/albums", get(planned(1)).post(planned(1)))
        .route("/albums/{id}/feed", get(planned(1)).post(planned(1)))
        .route("/albums/{id}/members", get(planned(2)).post(planned(2)))
        .route("/albums/{id}/members/{account}", axum::routing::delete(planned(2)))
        // storage nodes & grants (phase 1 register, phase 2 grants, phase 3 pairing)
        .route("/nodes", get(planned(1)).post(planned(1)))
        .route("/nodes/{id}/heartbeat", post(planned(1)))
        .route("/nodes/pair", post(planned(3)))
        .route("/grants", post(planned(2)))
        // billing (phase 7, `--features billing`)
        .route("/billing/plans", get(planned(7)))
        .route("/billing/webhook", post(planned(7)))
}

fn planned(phase: u8) -> axum::routing::MethodRouter<AppState> {
    axum::routing::any(move || async move { NotImplemented { phase } })
}

struct NotImplemented {
    phase: u8,
}

impl IntoResponse for NotImplemented {
    fn into_response(self) -> Response {
        (
            StatusCode::NOT_IMPLEMENTED,
            Json(serde_json::json!({
                "error": "not implemented",
                "planned_phase": self.phase,
                "see": "docs/ARCHITECTURE.md#11-delivery-phases",
            })),
        )
            .into_response()
    }
}

async fn health(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "core": photos_core::version(),
        "public_url": state.config.public_url,
        "database": state.config.database_url.is_some(),
        "billing": cfg!(feature = "billing"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app() -> Router {
        router(AppState::new(Config { public_url: "http://test".into(), database_url: None }))
    }

    #[tokio::test]
    async fn health_reports_core_version() {
        let res = app()
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["core"], photos_core::version());
        assert_eq!(json["database"], false);
    }

    #[tokio::test]
    async fn planned_routes_answer_501_with_phase() {
        let res = app()
            .oneshot(
                Request::builder().method("POST").uri("/v1/grants").body(Body::empty()).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_IMPLEMENTED);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["planned_phase"], 2);
    }
}
