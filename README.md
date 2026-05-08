# xray-bridge

HTTP→gRPC bridge for Xray node management API. Single stateless binary — no Python, no Docker, no protoc at runtime.

## What It Does

Translates standard HTTPS/JSON requests into Xray gRPC API calls, letting Cloudflare Workers, control panels, and scripts manage any number of Xray nodes through a single bridge endpoint.

```
[Your control panel / CF Worker]
        │
        │ HTTPS + JSON
        │ Authorization: Bearer <BRIDGE_TOKEN>
        │ X-Node-Domain / X-Node-Token / X-Node-Port (per request)
        ▼
[xray-bridge (this service)]
        │
        │ gRPC + TLS
        │ x-api-token: <node-token>
        ▼
[Xray node (nodeus6 / nodeus7 / ...)]
```

The bridge is **stateless**: node connection parameters are passed in HTTP headers on each request. One bridge instance can manage any number of nodes — no restart needed when adding nodes.

---

## Quick Start

### Prerequisites

- Rust stable (1.70+): `rustup update stable`
- `BRIDGE_TOKEN` environment variable

### Local Development

```bash
cd /home/fido/work/2026/xray-bridge

# Copy example env file and set your token
cp .env.example .env
# Edit .env and set BRIDGE_TOKEN

# Build and run
cargo run

# Or with hot reload
RUST_LOG=debug cargo watch -x run
```

The bridge listens on `http://localhost:8080` by default. Set `PORT` to change.

### Release Static Binary (deploy to any amd64 Debian 10/11/12)

```bash
# One-time: install musl target and tools
rustup target add x86_64-unknown-linux-musl
sudo apt install musl-tools          # Debian/Ubuntu

# Build fully-static binary
cargo build --release --target x86_64-unknown-linux-musl

# Verify it is statically linked
file target/x86_64-unknown-linux-musl/release/xray-bridge
# → ELF 64-bit LSB executable, x86-64, ..., statically linked

# Deploy to remote server
scp target/x86_64-unknown-linux-musl/release/xray-bridge root@your-server:/usr/local/bin/
```

On the server:

```bash
export BRIDGE_TOKEN=$(openssl rand -hex 32)
xray-bridge
```

---

## Configuration

| Variable | Required | Default | Description |
|---|---|---|---|
| `BRIDGE_TOKEN` | yes | — | Bridge authentication token |
| `PORT` | no | `8080` | HTTP listening port |
| `RUST_LOG` | no | `info,xray_bridge=debug` | Log filter |

---

## API Quick Reference

Every `/v1/*` request requires:

| Header | Required | Default | Purpose |
|---|---|---|---|
| `Authorization: Bearer <BRIDGE_TOKEN>` | yes | — | Bridge authentication |
| `X-Node-Domain` | yes | — | Target xray node FQDN |
| `X-Node-Token` | yes | — | Node API token (`x-api-token` gRPC metadata) |
| `X-Node-Port` | no | `443` | gRPC port |
| `X-Node-Name` | no | — | Audit log label only |

### Health Check

```bash
curl -s http://localhost:8080/healthz
# {"ok":true}
```

---

## curl Examples

Set up variables first:

```bash
BRIDGE=http://localhost:8080
BRIDGE_TOKEN=eb517415f34be7d45972539f4baa29acad42552063930c387f4eca1d70550474

NODE_DOMAIN=nodeus7.cc-proxy.cc
NODE_TOKEN=1514c55e3d3382cae9cd7dc3e2f2bfa00c8d0fcdbe5f6feda71dcf97113c0b33
EMAIL=jiangzihan@gmail.com
NEW_UUID=$(python3 -c "import uuid; print(uuid.uuid4())")
echo "Generated UUID: $NEW_UUID"


# 添加用户
curl -X POST $BRIDGE/v1/users \
-H "Authorization: Bearer $BRIDGE_TOKEN" \
-H "X-Node-Domain: $NODE_DOMAIN" \
-H "X-Node-Token: $NODE_TOKEN" \
-H "X-Node-Name: nodeus7" \
-H "Content-Type: application/json" \
-d "{
  \"tag\":   \"vless-ws\",
  \"email\": \"$EMAIL\",
  \"uuid\":  \"$NEW_UUID\",
  \"proto\": \"vless\"
}"


# 查询用户流量
curl -s -G $BRIDGE/v1/stats \
-H "Authorization: Bearer $BRIDGE_TOKEN" \
-H "X-Node-Domain: $NODE_DOMAIN" \
-H "X-Node-Token: $NODE_TOKEN" \
--data-urlencode "pattern=$EMAIL"

# 删除用户
curl -X DELETE "$BRIDGE/v1/users/$EMAIL?tag=vless-ws" \
-H "Authorization: Bearer $BRIDGE_TOKEN" \
-H "X-Node-Domain: $NODE_DOMAIN" \
-H "X-Node-Token: $NODE_TOKEN" 
```

### Add a VLESS User

```bash
curl -X POST $BRIDGE/v1/users \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "X-Node-Domain: $NODE_DOMAIN" \
  -H "X-Node-Token: $NODE_TOKEN" \
  -H "X-Node-Name: nodeus7" \
  -H "Content-Type: application/json" \
  -d "{
    \"tag\":   \"vless-ws\",
    \"email\": \"$EMAIL\",
    \"uuid\":  \"$NEW_UUID\",
    \"proto\": \"vless\"
  }"
```

Success (HTTP 201):
```json
{
  "ok": true,
  "action": "add-user",
  "persisted": false,
  "detail": {"tag": "vless-ws", "email": "alice@example.com", "proto": "vless"}
}
```

### Query Traffic Stats (fuzzy)

```bash
curl -s -G $BRIDGE/v1/stats \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "X-Node-Domain: $NODE_DOMAIN" \
  -H "X-Node-Token: $NODE_TOKEN" \
  --data-urlencode "pattern=$EMAIL"
```

Returns structured array:
```json
[
  {
    "name": "user>>>alice@example.com>>>traffic>>>uplink",
    "parts": ["user", "alice@example.com", "traffic", "uplink"],
    "scope": "user",
    "id": "alice@example.com",
    "category": "traffic",
    "metric": "uplink",
    "value": 4743835
  }
]
```

### Get Single Stat (exact name, contains `>>>`)

```bash
curl -s "$BRIDGE/v1/stats/user>>>$EMAIL>>>traffic>>>uplink" \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "X-Node-Domain: $NODE_DOMAIN" \
  -H "X-Node-Token: $NODE_TOKEN"
```

Returns `value: 0` for counters not yet created (lazy, not an error).

### List Users with Traffic Records

```bash
curl -s $BRIDGE/v1/users \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "X-Node-Domain: $NODE_DOMAIN" \
  -H "X-Node-Token: $NODE_TOKEN"
# ["alice@example.com", "bob@example.com"]
```

### Remove a User

```bash
curl -X DELETE "$BRIDGE/v1/users/$EMAIL?tag=vless-ws" \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "X-Node-Domain: $NODE_DOMAIN" \
  -H "X-Node-Token: $NODE_TOKEN"
```

Idempotent by default (`ignore_missing=true`). Add `&ignore_missing=false` for strict 404 on missing user.

### System Stats

```bash
curl -s $BRIDGE/v1/sys \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "X-Node-Domain: $NODE_DOMAIN" \
  -H "X-Node-Token: $NODE_TOKEN"
```

### Add VMess User

```bash
curl -X POST $BRIDGE/v1/users \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "X-Node-Domain: $NODE_DOMAIN" \
  -H "X-Node-Token: $NODE_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"tag\":            \"vmess-in\",
    \"email\":          \"$EMAIL\",
    \"uuid\":           \"$NEW_UUID\",
    \"proto\":          \"vmess\",
    \"vmess_security\": \"AUTO\"
  }"
```

### Add Trojan User

```bash
curl -X POST $BRIDGE/v1/users \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "X-Node-Domain: $NODE_DOMAIN" \
  -H "X-Node-Token: $NODE_TOKEN" \
  -H "Content-Type: application/json" \
  -d "{
    \"tag\":      \"trojan-in\",
    \"email\":    \"$EMAIL\",
    \"proto\":    \"trojan\",
    \"password\": \"supersecret\"
  }"
```

### Test Route Decision

```bash
curl -X POST $BRIDGE/v1/routing/test \
  -H "Authorization: Bearer $BRIDGE_TOKEN" \
  -H "X-Node-Domain: $NODE_DOMAIN" \
  -H "X-Node-Token: $NODE_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"target_domain": "google.com", "network": "TCP"}'
```

---

## Error Reference

| HTTP | `code` field | Meaning |
|---|---|---|
| 200/201 | — | Success |
| 400 | `INVALID_ARGUMENT` | Missing `X-Node-Domain` or `X-Node-Token` |
| 401 | `UNAUTHENTICATED` | Missing/wrong `Authorization: Bearer` |
| 404 | `NOT_FOUND` or `UNKNOWN` | Resource not found |
| 409 | `ALREADY_EXISTS` | User/inbound already exists |
| 422 | `INVALID_ARGUMENT` | Missing required body field (e.g. `tag`) |
| 501 | `UNIMPLEMENTED` | Node xray version too old for this API |
| 502 | `UNAVAILABLE` or `UNKNOWN` | Node unreachable / wrong token / gRPC blocked |
| 500 | `INTERNAL_ERROR` | Unexpected bridge error |

---

## Smoke Tests

```bash
# Tests that work without a real node (auth/header validation):
BRIDGE_TOKEN=mytoken cargo run &
sleep 1
BRIDGE_TOKEN=mytoken bash scripts/smoke.sh

# Full end-to-end with a real node:
BRIDGE_TOKEN=mytoken \
NODE_DOMAIN=node.example.com \
NODE_TOKEN=<node-api-token> \
bash scripts/smoke.sh
```
