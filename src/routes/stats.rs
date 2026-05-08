// HTTP handlers for stat endpoints (GET /v1/sys, /v1/users*, /v1/stats*).
// All handlers return errors via AppError implementing IntoResponse.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::{
    auth::BearerAuth,
    error::AppError,
    nodes::{AppState, NodeContext},
    xray::stats::{
        get_all_online_users, get_stat, get_sys_stats, get_users_stats, list_users,
        parse_stat_name, query_stats,
    },
};

#[derive(Deserialize)]
pub struct ResetQuery {
    #[serde(default)]
    pub reset: bool,
}

#[derive(Deserialize)]
pub struct StatsQuery {
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub reset: bool,
}

pub async fn get_sys_stats_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let stats = get_sys_stats(&client).await?;
    Ok((StatusCode::OK, Json(stats)))
}

pub async fn list_users_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let users = list_users(&client).await?;
    Ok((StatusCode::OK, Json(users)))
}

pub async fn get_online_users_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let users = get_all_online_users(&client).await?;
    Ok((StatusCode::OK, Json(users)))
}

pub async fn get_users_stats_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    Query(q): Query<ResetQuery>,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let stats = get_users_stats(&client, q.reset).await?;
    Ok((StatusCode::OK, Json(stats)))
}

pub async fn query_stats_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    Query(q): Query<StatsQuery>,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let records = query_stats(&client, &q.pattern, q.reset).await?;
    Ok((StatusCode::OK, Json(records)))
}

/// GET /v1/stats/{*name} — name may contain ">>>" characters.
/// axum wildcard `*name` captures the full path segment including slashes.
pub async fn get_stat_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    Path(name): Path<String>,
    Query(q): Query<ResetQuery>,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let value = get_stat(&client, &name, q.reset).await?;
    let mut rec = parse_stat_name(&name);
    rec.value = value;
    Ok((StatusCode::OK, Json(rec)))
}
