// gRPC calls to xray HandlerService.
// Covers user management (add/remove) and inbound/outbound management.

use prost::Message;
use tonic::metadata::MetadataValue;
use tonic::Request;

use crate::error::AppError;
use crate::nodes::XrayClient;
use crate::proto::xray::{
    app::proxyman::command::{
        handler_service_client::HandlerServiceClient, AddInboundRequest, AddOutboundRequest,
        AddUserOperation, AlterInboundRequest, ListInboundsRequest, ListOutboundsRequest,
        RemoveInboundRequest, RemoveOutboundRequest, RemoveUserOperation,
    },
    common::{protocol::User, serial::TypedMessage},
    core::{InboundHandlerConfig, OutboundHandlerConfig},
};
use crate::xray::account::{build_account_typed_message, ADD_USER_OP_TYPE, REMOVE_USER_OP_TYPE};

fn add_token<T>(mut request: Request<T>, token: &str) -> Request<T> {
    if let Ok(val) = MetadataValue::try_from(token) {
        request.metadata_mut().insert("x-api-token", val);
    }
    request
}

#[allow(clippy::too_many_arguments)]
pub async fn add_user(
    client: &XrayClient,
    tag: &str,
    email: &str,
    proto: &str,
    level: u32,
    uuid: &str,
    flow: &str,
    vmess_security: &str,
    password: &str,
    ss_cipher: &str,
) -> Result<(), AppError> {
    tracing::debug!(method = "AlterInbound/AddUser", target = ?client.name, tag = tag, email = email);

    let account_typed = build_account_typed_message(
        &proto.to_lowercase(),
        uuid,
        flow,
        vmess_security,
        password,
        ss_cipher,
    )?;

    let user = User {
        level,
        email: email.to_owned(),
        account: Some(account_typed),
    };

    let add_op = AddUserOperation { user: Some(user) };
    let op_typed = TypedMessage {
        r#type: ADD_USER_OP_TYPE.to_owned(),
        value: add_op.encode_to_vec(),
    };

    let req = AlterInboundRequest {
        tag: tag.to_owned(),
        operation: Some(op_typed),
    };

    let mut grpc = HandlerServiceClient::new(client.channel.clone());
    grpc.alter_inbound(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    Ok(())
}

pub async fn remove_user(client: &XrayClient, tag: &str, email: &str) -> Result<(), AppError> {
    tracing::debug!(method = "AlterInbound/RemoveUser", target = ?client.name, tag = tag, email = email);

    let remove_op = RemoveUserOperation {
        email: email.to_owned(),
    };
    let op_typed = TypedMessage {
        r#type: REMOVE_USER_OP_TYPE.to_owned(),
        value: remove_op.encode_to_vec(),
    };

    let req = AlterInboundRequest {
        tag: tag.to_owned(),
        operation: Some(op_typed),
    };

    let mut grpc = HandlerServiceClient::new(client.channel.clone());
    grpc.alter_inbound(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    Ok(())
}

pub async fn list_inbounds(client: &XrayClient) -> Result<Vec<serde_json::Value>, AppError> {
    tracing::debug!(method = "ListInbounds", target = ?client.name);
    let mut grpc = HandlerServiceClient::new(client.channel.clone());
    let req = ListInboundsRequest { is_only_tags: true };
    let resp = grpc
        .list_inbounds(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    let result = resp
        .into_inner()
        .inbounds
        .into_iter()
        .map(|ib| serde_json::json!({"tag": ib.tag}))
        .collect();
    Ok(result)
}

pub async fn add_inbound(
    client: &XrayClient,
    config: InboundHandlerConfig,
) -> Result<(), AppError> {
    tracing::debug!(method = "AddInbound", target = ?client.name, tag = %config.tag);
    let mut grpc = HandlerServiceClient::new(client.channel.clone());
    let req = AddInboundRequest {
        inbound: Some(config),
    };
    grpc.add_inbound(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    Ok(())
}

pub async fn remove_inbound(client: &XrayClient, tag: &str) -> Result<(), AppError> {
    tracing::debug!(method = "RemoveInbound", target = ?client.name, tag = tag);
    let mut grpc = HandlerServiceClient::new(client.channel.clone());
    let req = RemoveInboundRequest {
        tag: tag.to_owned(),
    };
    grpc.remove_inbound(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    Ok(())
}

pub async fn list_outbounds(client: &XrayClient) -> Result<Vec<serde_json::Value>, AppError> {
    tracing::debug!(method = "ListOutbounds", target = ?client.name);
    let mut grpc = HandlerServiceClient::new(client.channel.clone());
    let req = ListOutboundsRequest {};
    let resp = grpc
        .list_outbounds(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    let result = resp
        .into_inner()
        .outbounds
        .into_iter()
        .map(|ob| serde_json::json!({"tag": ob.tag}))
        .collect();
    Ok(result)
}

pub async fn add_outbound(
    client: &XrayClient,
    config: OutboundHandlerConfig,
) -> Result<(), AppError> {
    tracing::debug!(method = "AddOutbound", target = ?client.name, tag = %config.tag);
    let mut grpc = HandlerServiceClient::new(client.channel.clone());
    let req = AddOutboundRequest {
        outbound: Some(config),
    };
    grpc.add_outbound(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    Ok(())
}

pub async fn remove_outbound(client: &XrayClient, tag: &str) -> Result<(), AppError> {
    tracing::debug!(method = "RemoveOutbound", target = ?client.name, tag = tag);
    let mut grpc = HandlerServiceClient::new(client.channel.clone());
    let req = RemoveOutboundRequest {
        tag: tag.to_owned(),
    };
    grpc.remove_outbound(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    Ok(())
}
