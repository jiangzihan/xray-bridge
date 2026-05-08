// gRPC calls to xray StatsService.
// Mirrors Python XrayClient: query_stats, get_stat, get_sys_stats,
// get_all_online_users, get_users_stats, list_users.

use serde::{Deserialize, Serialize};
use tonic::metadata::MetadataValue;
use tonic::Request;

use crate::error::AppError;
use crate::nodes::XrayClient;
use crate::proto::xray::app::stats::command::{
    stats_service_client::StatsServiceClient, GetAllOnlineUsersRequest, GetStatsRequest,
    GetUsersStatsRequest, QueryStatsRequest, SysStatsRequest,
};

/// Structured stat record after parsing the `>>>` separated name.
#[derive(Debug, Serialize, Deserialize)]
pub struct StatRecord {
    pub name: String,
    pub parts: Vec<String>,
    pub scope: Option<String>,
    pub id: Option<String>,
    pub category: Option<String>,
    pub metric: Option<String>,
    pub value: i64,
}

/// Parse xray stat name into structured fields.
/// Format: `scope>>>id>>>category>>>metric`
/// Missing parts become `None`. Matches Python's `parse_stat_name` exactly.
pub fn parse_stat_name(name: &str) -> StatRecord {
    let parts: Vec<String> = name.split(">>>").map(str::to_owned).collect();
    let scope = parts.first().cloned();
    let id = parts.get(1).cloned();
    let category = parts.get(2).cloned();
    let metric = parts.get(3).cloned();
    StatRecord {
        name: name.to_owned(),
        parts,
        scope,
        id,
        category,
        metric,
        value: 0,
    }
}

fn add_token<T>(mut request: Request<T>, token: &str) -> Request<T> {
    if let Ok(val) = MetadataValue::try_from(token) {
        request.metadata_mut().insert("x-api-token", val);
    }
    request
}

pub async fn query_stats(
    client: &XrayClient,
    pattern: &str,
    reset: bool,
) -> Result<Vec<StatRecord>, AppError> {
    tracing::debug!(method = "QueryStats", target = ?client.name, pattern = pattern);
    let mut grpc = StatsServiceClient::new(client.channel.clone());
    let req = add_token(
        Request::new(QueryStatsRequest {
            pattern: pattern.to_owned(),
            reset,
        }),
        &client.token,
    );
    let resp = grpc.query_stats(req).await.map_err(AppError::from)?;
    let records = resp
        .into_inner()
        .stat
        .into_iter()
        .map(|s| {
            let mut rec = parse_stat_name(&s.name);
            rec.value = s.value;
            rec
        })
        .collect();
    Ok(records)
}

/// Get a single stat counter value. Returns 0 for NOT_FOUND (xray lazy creates counters).
pub async fn get_stat(client: &XrayClient, name: &str, reset: bool) -> Result<i64, AppError> {
    tracing::debug!(method = "GetStats", target = ?client.name, name = name);
    let mut grpc = StatsServiceClient::new(client.channel.clone());
    let req = add_token(
        Request::new(GetStatsRequest {
            name: name.to_owned(),
            reset,
        }),
        &client.token,
    );
    match grpc.get_stats(req).await {
        Ok(resp) => Ok(resp.into_inner().stat.map(|s| s.value).unwrap_or(0)),
        Err(status) => {
            // Not-found means counter not yet created — return 0, consistent with Python
            let details = status.message().to_lowercase();
            if status.code() == tonic::Code::NotFound
                || (status.code() == tonic::Code::Unknown && details.contains("not found"))
            {
                Ok(0)
            } else {
                Err(AppError::from(status))
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SysStats {
    #[serde(rename = "NumGoroutine")]
    pub num_goroutine: u32,
    #[serde(rename = "NumGC")]
    pub num_gc: u32,
    #[serde(rename = "Alloc")]
    pub alloc: u64,
    #[serde(rename = "TotalAlloc")]
    pub total_alloc: u64,
    #[serde(rename = "Sys")]
    pub sys: u64,
    #[serde(rename = "Mallocs")]
    pub mallocs: u64,
    #[serde(rename = "Frees")]
    pub frees: u64,
    #[serde(rename = "LiveObjects")]
    pub live_objects: u64,
    #[serde(rename = "PauseTotalNs")]
    pub pause_total_ns: u64,
    #[serde(rename = "Uptime")]
    pub uptime: u32,
}

pub async fn get_sys_stats(client: &XrayClient) -> Result<SysStats, AppError> {
    tracing::debug!(method = "GetSysStats", target = ?client.name);
    let mut grpc = StatsServiceClient::new(client.channel.clone());
    let req = add_token(Request::new(SysStatsRequest {}), &client.token);
    let resp = grpc.get_sys_stats(req).await.map_err(AppError::from)?;
    let r = resp.into_inner();
    Ok(SysStats {
        num_goroutine: r.num_goroutine,
        num_gc: r.num_gc,
        alloc: r.alloc,
        total_alloc: r.total_alloc,
        sys: r.sys,
        mallocs: r.mallocs,
        frees: r.frees,
        live_objects: r.live_objects,
        pause_total_ns: r.pause_total_ns,
        uptime: r.uptime,
    })
}

pub async fn get_all_online_users(client: &XrayClient) -> Result<Vec<String>, AppError> {
    tracing::debug!(method = "GetAllOnlineUsers", target = ?client.name);
    let mut grpc = StatsServiceClient::new(client.channel.clone());
    let req = add_token(Request::new(GetAllOnlineUsersRequest {}), &client.token);
    let resp = grpc
        .get_all_online_users(req)
        .await
        .map_err(AppError::from)?;
    Ok(resp.into_inner().users)
}

#[derive(Debug, Serialize)]
pub struct UserTraffic {
    pub email: String,
    pub uplink: i64,
    pub downlink: i64,
}

pub async fn get_users_stats(
    client: &XrayClient,
    reset: bool,
) -> Result<Vec<UserTraffic>, AppError> {
    tracing::debug!(method = "GetUsersStats", target = ?client.name, reset = reset);
    let mut grpc = StatsServiceClient::new(client.channel.clone());
    let req = add_token(
        Request::new(GetUsersStatsRequest {
            include_traffic: true,
            reset,
        }),
        &client.token,
    );
    let resp = grpc.get_users_stats(req).await.map_err(AppError::from)?;
    let users = resp
        .into_inner()
        .users
        .into_iter()
        .map(|u| {
            let (uplink, downlink) = u.traffic.map(|t| (t.uplink, t.downlink)).unwrap_or((0, 0));
            UserTraffic {
                email: u.email,
                uplink,
                downlink,
            }
        })
        .collect();
    Ok(users)
}

/// Derive user list from stats counters — only users with traffic records.
/// Mirrors Python's list_users: query_stats(pattern="user>>>") then extract part[1].
pub async fn list_users(client: &XrayClient) -> Result<Vec<String>, AppError> {
    let records = query_stats(client, "user>>>", false).await?;
    let mut emails: std::collections::BTreeSet<String> = Default::default();
    for rec in records {
        if let Some(email) = rec.id {
            emails.insert(email);
        }
    }
    Ok(emails.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stat_name_four_parts() {
        let rec = parse_stat_name("user>>>alice@example.com>>>traffic>>>uplink");
        assert_eq!(rec.scope.as_deref(), Some("user"));
        assert_eq!(rec.id.as_deref(), Some("alice@example.com"));
        assert_eq!(rec.category.as_deref(), Some("traffic"));
        assert_eq!(rec.metric.as_deref(), Some("uplink"));
        assert_eq!(rec.parts.len(), 4);
    }

    #[test]
    fn parse_stat_name_three_parts() {
        let rec = parse_stat_name("user>>>alice@example.com>>>traffic");
        assert_eq!(rec.scope.as_deref(), Some("user"));
        assert_eq!(rec.id.as_deref(), Some("alice@example.com"));
        assert_eq!(rec.category.as_deref(), Some("traffic"));
        assert!(rec.metric.is_none());
        assert_eq!(rec.parts.len(), 3);
    }

    #[test]
    fn parse_stat_name_two_parts() {
        let rec = parse_stat_name("inbound>>>vless-ws");
        assert_eq!(rec.scope.as_deref(), Some("inbound"));
        assert_eq!(rec.id.as_deref(), Some("vless-ws"));
        assert!(rec.category.is_none());
        assert!(rec.metric.is_none());
        assert_eq!(rec.parts.len(), 2);
    }

    #[test]
    fn parse_stat_name_one_part() {
        let rec = parse_stat_name("outbound");
        assert_eq!(rec.scope.as_deref(), Some("outbound"));
        assert!(rec.id.is_none());
        assert_eq!(rec.parts.len(), 1);
    }

    #[test]
    fn parse_stat_name_empty() {
        let rec = parse_stat_name("");
        // Empty string split by ">>>" gives [""]
        assert_eq!(rec.scope.as_deref(), Some(""));
        assert!(rec.id.is_none());
        assert_eq!(rec.parts.len(), 1);
        assert_eq!(rec.name, "");
    }
}
