// gRPC calls to xray RoutingService and LoggerService.

use tonic::metadata::MetadataValue;
use tonic::Request;

use crate::error::AppError;
use crate::nodes::XrayClient;
use crate::proto::xray::{
    app::{
        log::command::{logger_service_client::LoggerServiceClient, RestartLoggerRequest},
        router::command::{
            routing_service_client::RoutingServiceClient, GetBalancerInfoRequest, ListRuleRequest,
            OverrideBalancerTargetRequest, RemoveRuleRequest, RoutingContext, TestRouteRequest,
        },
    },
    common::net::Network,
};

fn add_token<T>(mut request: Request<T>, token: &str) -> Request<T> {
    if let Ok(val) = MetadataValue::try_from(token) {
        request.metadata_mut().insert("x-api-token", val);
    }
    request
}

/// Convert network string to xray Network enum int value.
/// Matches Python's _network_enum: TCP=2, UDP=3.
fn network_from_str(s: &str) -> i32 {
    match s.to_uppercase().as_str() {
        "TCP" => Network::Tcp as i32,
        "UDP" => Network::Udp as i32,
        _ => Network::Tcp as i32,
    }
}

pub async fn test_route(
    client: &XrayClient,
    target_domain: &str,
    network: &str,
    inbound_tag: &str,
    user: &str,
) -> Result<serde_json::Value, AppError> {
    tracing::debug!(method = "TestRoute", target = ?client.name);
    let mut grpc = RoutingServiceClient::new(client.channel.clone());

    let ctx = RoutingContext {
        target_domain: target_domain.to_owned(),
        network: network_from_str(network),
        inbound_tag: inbound_tag.to_owned(),
        user: user.to_owned(),
        ..Default::default()
    };

    let req = TestRouteRequest {
        routing_context: Some(ctx),
        field_selectors: vec![],
        publish_result: false,
    };

    let resp = grpc
        .test_route(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;

    let r = resp.into_inner();
    Ok(serde_json::json!({
        "OutboundTag":       r.outbound_tag,
        "InboundTag":        r.inbound_tag,
        "Network":           r.network,
        "TargetDomain":      r.target_domain,
        "TargetPort":        r.target_port,
        "Protocol":          r.protocol,
        "User":              r.user,
        "OutboundGroupTags": r.outbound_group_tags,
    }))
}

pub async fn list_routing_rules(client: &XrayClient) -> Result<Vec<serde_json::Value>, AppError> {
    tracing::debug!(method = "ListRule", target = ?client.name);
    let mut grpc = RoutingServiceClient::new(client.channel.clone());
    let req = ListRuleRequest {};
    let resp = grpc
        .list_rule(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    let rules = resp
        .into_inner()
        .rules
        .into_iter()
        .map(|r| serde_json::json!({"tag": r.tag, "ruleTag": r.rule_tag}))
        .collect();
    Ok(rules)
}

pub async fn remove_routing_rule(client: &XrayClient, rule_tag: &str) -> Result<(), AppError> {
    tracing::debug!(method = "RemoveRule", target = ?client.name, rule_tag = rule_tag);
    let mut grpc = RoutingServiceClient::new(client.channel.clone());
    let req = RemoveRuleRequest {
        rule_tag: rule_tag.to_owned(),
    };
    grpc.remove_rule(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    Ok(())
}

pub async fn get_balancer_info(
    client: &XrayClient,
    tag: &str,
) -> Result<serde_json::Value, AppError> {
    tracing::debug!(method = "GetBalancerInfo", target = ?client.name, tag = tag);
    let mut grpc = RoutingServiceClient::new(client.channel.clone());
    let req = GetBalancerInfoRequest {
        tag: tag.to_owned(),
    };
    let resp = grpc
        .get_balancer_info(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    let b = resp.into_inner().balancer.unwrap_or_default();
    let override_target = b.r#override.map(|o| o.target).unwrap_or_default();
    let principle_targets: Vec<String> = b.principle_target.map(|p| p.tag).unwrap_or_default();
    Ok(serde_json::json!({
        "override_target":    override_target,
        "principle_targets": principle_targets,
    }))
}

pub async fn override_balancer_target(
    client: &XrayClient,
    balancer_tag: &str,
    target: &str,
) -> Result<(), AppError> {
    tracing::debug!(method = "OverrideBalancerTarget", target = ?client.name, balancer_tag = balancer_tag);
    let mut grpc = RoutingServiceClient::new(client.channel.clone());
    let req = OverrideBalancerTargetRequest {
        balancer_tag: balancer_tag.to_owned(),
        target: target.to_owned(),
    };
    grpc.override_balancer_target(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    Ok(())
}

pub async fn restart_logger(client: &XrayClient) -> Result<(), AppError> {
    tracing::debug!(method = "RestartLogger", target = ?client.name);
    let mut grpc = LoggerServiceClient::new(client.channel.clone());
    let req = RestartLoggerRequest {};
    grpc.restart_logger(add_token(Request::new(req), &client.token))
        .await
        .map_err(AppError::from)?;
    Ok(())
}
