// Route builder — assembles all axum routes into a single Router<AppState>.
// Route paths are 1:1 with Python version (see CLAUDE.md §6.2).

pub mod handler;
pub mod routing;
pub mod stats;

use axum::{
    routing::{delete, get, post},
    Json, Router,
};

use crate::nodes::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Health check — no auth, no node headers required
        .route(
            "/healthz",
            get(|| async { Json(serde_json::json!({"ok": true})) }),
        )
        // Stats routes
        .route("/v1/sys", get(stats::get_sys_stats_handler))
        .route("/v1/users", get(stats::list_users_handler))
        .route("/v1/users/online", get(stats::get_online_users_handler))
        .route("/v1/users/stats", get(stats::get_users_stats_handler))
        .route("/v1/stats", get(stats::query_stats_handler))
        // Wildcard captures ">>>" in stat names, e.g.:
        //   user>>>alice@example.com>>>traffic>>>uplink
        .route("/v1/stats/*name", get(stats::get_stat_handler))
        // User management (POST on same /v1/users path, DELETE on /:email)
        .route("/v1/users", post(handler::add_user_handler))
        .route("/v1/users/:email", delete(handler::remove_user_handler))
        // Inbound management
        .route(
            "/v1/inbounds",
            get(handler::list_inbounds_handler).post(handler::add_inbound_handler),
        )
        .route("/v1/inbounds/:tag", delete(handler::remove_inbound_handler))
        // Outbound management
        .route(
            "/v1/outbounds",
            get(handler::list_outbounds_handler).post(handler::add_outbound_handler),
        )
        .route(
            "/v1/outbounds/:tag",
            delete(handler::remove_outbound_handler),
        )
        // Logger
        .route("/v1/logger/restart", post(handler::restart_logger_handler))
        // Routing service
        .route("/v1/routing/test", post(routing::test_route_handler))
        .route("/v1/routing/rules", get(routing::list_rules_handler))
        .route(
            "/v1/routing/rules/:tag",
            delete(routing::remove_rule_handler),
        )
        .route(
            "/v1/routing/balancers/:tag",
            get(routing::get_balancer_handler).put(routing::override_balancer_handler),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Settings, nodes::XrayClientCache};
    use axum::http::{Request, StatusCode};
    use std::sync::Arc;
    use tower::ServiceExt;

    fn make_state(token: &str) -> AppState {
        let settings = Arc::new(Settings {
            bridge_token: token.to_owned(),
            port: 8080,
        });
        let cache = Arc::new(XrayClientCache::new(256));
        AppState { settings, cache }
    }

    #[tokio::test]
    async fn healthz_returns_200() {
        let state = make_state("test-token");
        let app = build_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/healthz")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn missing_auth_returns_401() {
        let state = make_state("secret");
        let app = build_router(state);
        // No Authorization header — should get 401
        let req = Request::builder()
            .method("GET")
            .uri("/v1/sys")
            .header("X-Node-Domain", "node.example.com")
            .header("X-Node-Token", "nodetoken")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn missing_node_domain_returns_400() {
        let state = make_state("secret");
        let app = build_router(state);
        // No X-Node-Domain header — should get 400
        let req = Request::builder()
            .method("GET")
            .uri("/v1/sys")
            .header("Authorization", "Bearer secret")
            .header("X-Node-Token", "nodetoken")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn missing_node_token_returns_400() {
        let state = make_state("secret");
        let app = build_router(state);
        // No X-Node-Token header — should get 400
        let req = Request::builder()
            .method("GET")
            .uri("/v1/sys")
            .header("Authorization", "Bearer secret")
            .header("X-Node-Domain", "node.example.com")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn wrong_auth_token_returns_401() {
        let state = make_state("secret");
        let app = build_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/v1/sys")
            .header("Authorization", "Bearer wrong-token")
            .header("X-Node-Domain", "node.example.com")
            .header("X-Node-Token", "nodetoken")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
