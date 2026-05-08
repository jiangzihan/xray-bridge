// HTTP handlers for routing service endpoints.
// POST /v1/routing/test, GET/DELETE /v1/routing/rules*, GET/PUT /v1/routing/balancers*.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::{
    auth::BearerAuth,
    error::AppError,
    nodes::{AppState, NodeContext},
    xray::routing::{
        get_balancer_info, list_routing_rules, override_balancer_target, remove_routing_rule,
        test_route,
    },
};

#[derive(Deserialize)]
pub struct TestRouteBody {
    #[serde(default)]
    pub target_domain: String,
    #[serde(default = "default_network")]
    pub network: String,
    #[serde(default)]
    pub inbound_tag: String,
    #[serde(default)]
    pub user: String,
}

fn default_network() -> String {
    "TCP".to_owned()
}

#[derive(Deserialize)]
pub struct OverrideBalancerBody {
    pub target: String,
}

fn operation_result(action: &str, detail: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "action": action,
        "persisted": false,
        "detail": detail,
    })
}

pub async fn test_route_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
    Json(body): Json<TestRouteBody>,
) -> Result<impl IntoResponse, AppError> {
    let result = test_route(
        &client,
        &body.target_domain,
        &body.network,
        &body.inbound_tag,
        &body.user,
    )
    .await?;
    Ok((StatusCode::OK, Json(result)))
}

pub async fn list_rules_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let rules = list_routing_rules(&client).await?;
    Ok((StatusCode::OK, Json(rules)))
}

pub async fn remove_rule_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    Path(tag): Path<String>,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    remove_routing_rule(&client, &tag).await?;
    let result = operation_result("remove-routing-rule", serde_json::json!({"tag": tag}));
    Ok((StatusCode::OK, Json(result)))
}

pub async fn get_balancer_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    Path(tag): Path<String>,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let info = get_balancer_info(&client, &tag).await?;
    Ok((StatusCode::OK, Json(info)))
}

pub async fn override_balancer_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    Path(tag): Path<String>,
    _auth: BearerAuth,
    Json(body): Json<OverrideBalancerBody>,
) -> Result<impl IntoResponse, AppError> {
    override_balancer_target(&client, &tag, &body.target).await?;
    let result = operation_result(
        "override-balancer-target",
        serde_json::json!({
            "balancer_tag": tag,
            "target": body.target,
        }),
    );
    Ok((StatusCode::OK, Json(result)))
}
