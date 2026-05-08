#!/usr/bin/env bash
# smoke.sh — quick smoke tests for xray-bridge.
# Tests that can run without a real xray node: healthz + 401/400 paths.
# Tests requiring a real node are shown but skipped unless NODE_DOMAIN is set.

set -euo pipefail

BRIDGE="${BRIDGE_URL:-http://localhost:8080}"
BRIDGE_TOKEN="${BRIDGE_TOKEN:-}"
NODE_DOMAIN="${NODE_DOMAIN:-}"
NODE_TOKEN="${NODE_TOKEN:-}"
EMAIL="smoke-test-$(date +%s)@example.com"
NEW_UUID=$(python3 -c "import uuid; print(uuid.uuid4())" 2>/dev/null || uuidgen)

if [ -z "$BRIDGE_TOKEN" ]; then
    echo "ERROR: BRIDGE_TOKEN environment variable is required"
    echo "Usage: BRIDGE_TOKEN=<token> [NODE_DOMAIN=<fqdn>] [NODE_TOKEN=<tok>] bash scripts/smoke.sh"
    exit 1
fi

pass() { echo "[PASS] $1"; }
fail() { echo "[FAIL] $1"; exit 1; }

echo "=== Smoke tests for xray-bridge at $BRIDGE ==="
echo ""

# 1. Health check (no auth needed)
echo "--- Test 1: healthz ---"
resp=$(curl -sf "$BRIDGE/healthz")
echo "$resp" | grep -q '"ok":true' && pass "healthz returns {\"ok\":true}" || fail "healthz failed: $resp"

# 2. Missing auth → 401
echo ""
echo "--- Test 2: missing Authorization → 401 ---"
http_code=$(curl -s -o /dev/null -w "%{http_code}" "$BRIDGE/v1/sys" \
    -H "X-Node-Domain: node.example.com" \
    -H "X-Node-Token: test-token")
[ "$http_code" = "401" ] && pass "missing auth → 401" || fail "expected 401, got $http_code"

# 3. Wrong token → 401
echo ""
echo "--- Test 3: wrong auth token → 401 ---"
http_code=$(curl -s -o /dev/null -w "%{http_code}" "$BRIDGE/v1/sys" \
    -H "Authorization: Bearer WRONG_TOKEN" \
    -H "X-Node-Domain: node.example.com" \
    -H "X-Node-Token: test-token")
[ "$http_code" = "401" ] && pass "wrong token → 401" || fail "expected 401, got $http_code"

# 4. Missing X-Node-Domain → 400
echo ""
echo "--- Test 4: missing X-Node-Domain → 400 ---"
http_code=$(curl -s -o /dev/null -w "%{http_code}" "$BRIDGE/v1/sys" \
    -H "Authorization: Bearer $BRIDGE_TOKEN" \
    -H "X-Node-Token: test-token")
[ "$http_code" = "400" ] && pass "missing X-Node-Domain → 400" || fail "expected 400, got $http_code"

# 5. Missing X-Node-Token → 400
echo ""
echo "--- Test 5: missing X-Node-Token → 400 ---"
http_code=$(curl -s -o /dev/null -w "%{http_code}" "$BRIDGE/v1/sys" \
    -H "Authorization: Bearer $BRIDGE_TOKEN" \
    -H "X-Node-Domain: node.example.com")
[ "$http_code" = "400" ] && pass "missing X-Node-Token → 400" || fail "expected 400, got $http_code"

# --- Node-required tests ---
if [ -z "$NODE_DOMAIN" ] || [ -z "$NODE_TOKEN" ]; then
    echo ""
    echo "NOTE: Skipping node-required tests (set NODE_DOMAIN and NODE_TOKEN to run)."
    echo ""
    echo "=== 5/5 smoke tests passed (node tests skipped) ==="
    exit 0
fi

H_BRIDGE="Authorization: Bearer $BRIDGE_TOKEN"
H_NODE_D="X-Node-Domain: $NODE_DOMAIN"
H_NODE_T="X-Node-Token: $NODE_TOKEN"

# 6. Add VLESS user
echo ""
echo "--- Test 6: add VLESS user ---"
http_code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BRIDGE/v1/users" \
    -H "$H_BRIDGE" -H "$H_NODE_D" -H "$H_NODE_T" \
    -H "Content-Type: application/json" \
    -d "{\"tag\":\"vless-ws\",\"email\":\"$EMAIL\",\"uuid\":\"$NEW_UUID\",\"proto\":\"vless\"}")
[ "$http_code" = "201" ] && pass "add user → 201" || fail "add user failed, got $http_code"

# 7. Query stats (empty pattern)
echo ""
echo "--- Test 7: query stats ---"
resp=$(curl -s -G "$BRIDGE/v1/stats" \
    -H "$H_BRIDGE" -H "$H_NODE_D" -H "$H_NODE_T" \
    --data-urlencode "pattern=")
echo "$resp" | python3 -c "import json,sys; d=json.load(sys.stdin); assert isinstance(d,list)" \
    && pass "query stats returns list" || fail "query stats failed: $resp"

# 8. Delete user (ignore_missing=true by default)
echo ""
echo "--- Test 8: remove user ---"
http_code=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE \
    "$BRIDGE/v1/users/$EMAIL?tag=vless-ws" \
    -H "$H_BRIDGE" -H "$H_NODE_D" -H "$H_NODE_T")
[ "$http_code" = "200" ] && pass "remove user → 200" || fail "remove user failed, got $http_code"

# 9. Idempotent delete (user gone, ignore_missing=true)
echo ""
echo "--- Test 9: idempotent remove (ignore_missing=true) ---"
http_code=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE \
    "$BRIDGE/v1/users/$EMAIL?tag=vless-ws&ignore_missing=true" \
    -H "$H_BRIDGE" -H "$H_NODE_D" -H "$H_NODE_T")
[ "$http_code" = "200" ] && pass "idempotent remove → 200" || fail "expected 200, got $http_code"

echo ""
echo "=== All smoke tests passed ==="
