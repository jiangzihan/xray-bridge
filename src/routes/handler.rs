// HTTP handlers for management endpoints.
// POST/DELETE /v1/users, /v1/inbounds*, /v1/outbounds*, /v1/logger/restart.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    auth::BearerAuth,
    error::AppError,
    nodes::{AppState, NodeContext},
    proto::xray::{
        common::serial::TypedMessage,
        core::{InboundHandlerConfig, OutboundHandlerConfig},
    },
    xray::handler::{
        add_inbound, add_outbound, add_user, list_inbounds, list_outbounds, remove_inbound,
        remove_outbound, remove_user,
    },
    xray::routing::restart_logger,
};

// ---------------------------------------------------------------------------
// Request body schemas
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AddUserBody {
    pub tag: String,
    pub email: String,
    #[serde(default = "default_proto")]
    pub proto: String,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub uuid: String,
    #[serde(default)]
    pub flow: String,
    #[serde(default = "default_vmess_security")]
    pub vmess_security: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_ss_cipher")]
    pub ss_cipher: String,
}

fn default_proto() -> String {
    "vless".to_owned()
}
fn default_vmess_security() -> String {
    "AUTO".to_owned()
}
fn default_ss_cipher() -> String {
    "CHACHA20_POLY1305".to_owned()
}

#[derive(Deserialize)]
pub struct InboundConfigBody {
    pub config: Value,
}

#[derive(Deserialize)]
pub struct RemoveUserQuery {
    pub tag: String,
    #[serde(default = "default_ignore_missing")]
    pub ignore_missing: bool,
}

fn default_ignore_missing() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Response envelope helpers
// ---------------------------------------------------------------------------

fn operation_result(action: &str, detail: Value) -> Value {
    serde_json::json!({
        "ok": true,
        "action": action,
        "persisted": false,
        "detail": detail,
    })
}

// ---------------------------------------------------------------------------
// JSON → InboundHandlerConfig / OutboundHandlerConfig
// ---------------------------------------------------------------------------

/// Convert a serde_json::Value to InboundHandlerConfig.
/// The caller guarantees `v` has a "tag" field already validated.
/// Optional "receiver_settings" and "proxy_settings" follow the TypedMessage
/// contract: {"type": "...", "value": "<base64 protobuf bytes>"}.
fn json_to_inbound_config(v: Value) -> Result<InboundHandlerConfig, AppError> {
    let tag = v
        .get("tag")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::UnprocessableEntity("config.tag is required".to_owned()))?
        .to_owned();

    let receiver_settings = extract_typed_message(&v, "receiver_settings")?;
    let proxy_settings = extract_typed_message(&v, "proxy_settings")?;

    Ok(InboundHandlerConfig {
        tag,
        receiver_settings,
        proxy_settings,
    })
}

fn json_to_outbound_config(v: Value) -> Result<OutboundHandlerConfig, AppError> {
    let tag = v
        .get("tag")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::UnprocessableEntity("config.tag is required".to_owned()))?
        .to_owned();

    let sender_settings = extract_typed_message(&v, "sender_settings")?;
    let proxy_settings = extract_typed_message(&v, "proxy_settings")?;

    Ok(OutboundHandlerConfig {
        tag,
        sender_settings,
        proxy_settings,
        ..Default::default()
    })
}

/// Extract an optional TypedMessage from a JSON object field.
/// Expected shape: {"type": "...", "value": "<base64>"}
fn extract_typed_message(obj: &Value, field: &str) -> Result<Option<TypedMessage>, AppError> {
    let Some(sub) = obj.get(field) else {
        return Ok(None);
    };

    let type_name = sub
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::UnprocessableEntity(format!("{field}.type must be a string")))?
        .to_owned();

    let value_b64 = sub.get("value").and_then(Value::as_str).ok_or_else(|| {
        AppError::UnprocessableEntity(format!("{field}.value must be a base64 string"))
    })?;

    let value = B64.decode(value_b64).map_err(|e| {
        AppError::UnprocessableEntity(format!("{field}.value base64 decode error: {e}"))
    })?;

    Ok(Some(TypedMessage {
        r#type: type_name,
        value,
    }))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn add_user_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
    Json(body): Json<AddUserBody>,
) -> Result<impl IntoResponse, AppError> {
    add_user(
        &client,
        &body.tag,
        &body.email,
        &body.proto,
        body.level,
        &body.uuid,
        &body.flow,
        &body.vmess_security,
        &body.password,
        &body.ss_cipher,
    )
    .await?;
    let result = operation_result(
        "add-user",
        serde_json::json!({
            "tag": body.tag,
            "email": body.email,
            "proto": body.proto,
        }),
    );
    Ok((StatusCode::CREATED, Json(result)))
}

pub async fn remove_user_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    Path(email): Path<String>,
    Query(q): Query<RemoveUserQuery>,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    match remove_user(&client, &q.tag, &email).await {
        Ok(()) => {}
        Err(AppError::Grpc {
            ref code,
            ref details,
        }) => {
            let is_not_found = *code == tonic::Code::NotFound
                || (*code == tonic::Code::Unknown && details.to_lowercase().contains("not found"));
            if is_not_found && q.ignore_missing {
                let result = operation_result(
                    "remove-user",
                    serde_json::json!({
                        "tag": q.tag,
                        "email": email,
                        "note": "not found, ignored",
                    }),
                );
                return Ok((StatusCode::OK, Json(result)));
            }
            return Err(AppError::Grpc {
                code: *code,
                details: details.clone(),
            });
        }
        Err(e) => return Err(e),
    }
    let result = operation_result(
        "remove-user",
        serde_json::json!({
            "tag": q.tag,
            "email": email,
        }),
    );
    Ok((StatusCode::OK, Json(result)))
}

pub async fn list_inbounds_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let inbounds = list_inbounds(&client).await?;
    Ok((StatusCode::OK, Json(inbounds)))
}

pub async fn add_inbound_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
    Json(body): Json<InboundConfigBody>,
) -> Result<impl IntoResponse, AppError> {
    if body.config.get("tag").is_none() {
        return Err(AppError::UnprocessableEntity(
            "config 必须包含 'tag' 字段".to_owned(),
        ));
    }
    let tag = body
        .config
        .get("tag")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let config = json_to_inbound_config(body.config)?;
    add_inbound(&client, config).await?;
    let result = operation_result("add-inbound", serde_json::json!({"tag": tag}));
    Ok((StatusCode::CREATED, Json(result)))
}

pub async fn remove_inbound_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    Path(tag): Path<String>,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    remove_inbound(&client, &tag).await?;
    let result = operation_result("remove-inbound", serde_json::json!({"tag": tag}));
    Ok((StatusCode::OK, Json(result)))
}

pub async fn list_outbounds_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    let outbounds = list_outbounds(&client).await?;
    Ok((StatusCode::OK, Json(outbounds)))
}

pub async fn add_outbound_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
    Json(body): Json<InboundConfigBody>,
) -> Result<impl IntoResponse, AppError> {
    if body.config.get("tag").is_none() {
        return Err(AppError::UnprocessableEntity(
            "config 必须包含 'tag' 字段".to_owned(),
        ));
    }
    let tag = body
        .config
        .get("tag")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let config = json_to_outbound_config(body.config)?;
    add_outbound(&client, config).await?;
    let result = operation_result("add-outbound", serde_json::json!({"tag": tag}));
    Ok((StatusCode::CREATED, Json(result)))
}

pub async fn remove_outbound_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    Path(tag): Path<String>,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    remove_outbound(&client, &tag).await?;
    let result = operation_result("remove-outbound", serde_json::json!({"tag": tag}));
    Ok((StatusCode::OK, Json(result)))
}

pub async fn restart_logger_handler(
    State(_state): State<AppState>,
    NodeContext(client): NodeContext,
    _auth: BearerAuth,
) -> Result<impl IntoResponse, AppError> {
    restart_logger(&client).await?;
    let result = operation_result("restart-logger", serde_json::json!({}));
    Ok((StatusCode::OK, Json(result)))
}
