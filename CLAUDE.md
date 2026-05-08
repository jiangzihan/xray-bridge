# xray-bridge — Rust HTTP→gRPC Bridge for Xray

本文件是项目宪法。所有实现必须严格遵守。子 agent 在写代码前先通读全文。

---

## 1. 项目目标

把 Python 版 `cf-xray-bridge`（FastAPI + grpcio）等价移植为 **Rust 单体可执行文件**，行为、HTTP 路由、请求/响应格式、HTTP 状态码、header 协议、错误语义与 Python 版**完全一致**——只是换实现语言。

### 1.1 为什么换 Rust

- **单二进制部署**：`cargo build --release --target x86_64-unknown-linux-musl` 产出 fully-static 可执行文件，scp 到任意 amd64 Debian 10/11/12 直接 `./xray-bridge` 跑，不要 Python 解释器、不要虚拟环境、不要 Docker、不要 protoc 运行时。
- **零运行时 protobuf 编译**：proto 在 `build.rs` 里编译进二进制，部署机不需要 xray-src，也不需要 grpc-tools。
- **更低延迟和内存占用**：bridge 是热路径上的协议翻译器，gRPC channel 复用 + tokio + tonic 比 Python grpcio 快好几倍。

### 1.2 不变量（Invariant）

下列条目改了一个就算破坏 API 兼容性：

| 不变量 | 说明 |
|---|---|
| HTTP 路由路径 | 与 Python 版逐字符一致（见 §6） |
| 请求 header 名 | `Authorization`, `X-Node-Domain`, `X-Node-Token`, `X-Node-Port`, `X-Node-Name` |
| 请求 body JSON 字段名 | `tag`, `email`, `uuid`, `proto`, `level`, `flow`, `vmess_security`, `password`, `ss_cipher`, `config`, `target_domain`, `network`, `inbound_tag`, `user`, `target` |
| 响应 body JSON 字段名 | `ok`, `code`, `error`, `action`, `persisted`, `detail`, `name`, `parts`, `scope`, `id`, `category`, `metric`, `value` 等 |
| 响应 envelope 结构 | 错误 `{"ok":false,"code":STR,"error":STR}`；操作成功 `{"ok":true,"action":STR,"persisted":bool,"detail":{...}}` |
| HTTP 状态码 | gRPC code → HTTP 映射逐项匹配（见 §5） |
| Stat name 解析 | `scope>>>id>>>category>>>metric` 四段拆分 |
| 无状态语义 | 节点连接信息只通过 header 传，bridge 进程内不持久化 |

任何不在上表的实现细节（路由器框架、JSON 库选择、错误类型设计等）由 Rust 实现自由发挥。

---

## 2. 技术栈（强制）

| 维度 | 选型 | 理由 |
|---|---|---|
| 异步运行时 | `tokio` (rt-multi-thread) | tonic/axum 默认 |
| HTTP server | `axum` 0.7+ | tower 生态、状态注入符合本项目需求 |
| gRPC client | `tonic` 0.12+ | 配套 prost、原生 rustls |
| Protobuf | `prost` 0.13+ + `prost-types` | tonic 自带 |
| TLS | `rustls` (`tonic` feature `tls-roots` + `tls-webpki-roots`) | **禁止 OpenSSL**——musl 静态链接更难 |
| 序列化 | `serde` + `serde_json` | 标准 |
| 日志 | `tracing` + `tracing-subscriber` (`fmt`) | 结构化输出 |
| 配置 | `dotenvy` + 手写 `Config` 结构 | 不必引入 `figment` 等重型库 |
| LRU 缓存 | `lru` crate（或 `parking_lot` + `IndexMap` 自实现） | 与 Python `OrderedDict` 行为一致 |
| 错误 | `thiserror`（库内） + `anyhow`（main 顶层） | 标准做法 |
| Build 时 proto 编译 | `tonic-build` 0.12+ | 在 `build.rs` 里跑 |

**禁止依赖**：openssl-sys、native-tls、reqwest 默认 features（如需 HTTP client 用 `hyper` 或 reqwest 的 rustls feature）。

---

## 3. 目录结构（强制）

```
xray-bridge/
├── Cargo.toml
├── Cargo.lock                  # 提交
├── build.rs                    # tonic-build 入口
├── rust-toolchain.toml         # 锁定 stable
├── .gitignore
├── .env.example
├── README.md
├── proto/                      # 从 xray-src 拷贝出来的 .proto（自包含，不依赖 XRAY_SRC env）
│   ├── app/stats/command/command.proto
│   ├── app/proxyman/command/command.proto
│   ├── app/log/command/config.proto
│   ├── app/router/command/command.proto
│   ├── common/protocol/user.proto
│   ├── common/protocol/headers.proto
│   ├── common/protocol/server_spec.proto
│   ├── common/serial/typed_message.proto
│   ├── common/net/{address,port,destination,network}.proto
│   ├── core/config.proto
│   ├── proxy/vless/account.proto
│   ├── proxy/vmess/account.proto
│   ├── proxy/trojan/config.proto
│   ├── proxy/shadowsocks/config.proto
│   └── transport/internet/...  # 仅 import 链拉到的
└── src/
    ├── main.rs                 # 进程入口：日志 init、Config::from_env、router 装载、监听 PORT
    ├── config.rs               # Settings { bridge_token, port }
    ├── auth.rs                 # bearer_auth tower middleware / extractor
    ├── error.rs                # AppError、IntoResponse、grpc_status_to_http
    ├── proto.rs                # tonic::include_proto! 各 package（xray.app.stats.command 等）
    ├── nodes.rs                # XrayClientCache + NodeContext extractor (从 X-Node-* header)
    ├── xray/                   # gRPC 客户端封装
    │   ├── mod.rs              # pub use 子模块；XrayClient struct
    │   ├── stats.rs            # query_stats / get_stat / sys_stats / users / users_stats / online_users / list_users
    │   ├── handler.rs          # add_user / remove_user / inbounds / outbounds / restart_logger
    │   ├── routing.rs          # test_route / list_rules / remove_rule / get_balancer / override_balancer
    │   └── account.rs          # build_account_typed_message (vless/vmess/trojan/ss)
    └── routes/
        ├── mod.rs              # build_router() -> Router<AppState>
        ├── stats.rs            # GET /v1/sys, /v1/users*, /v1/stats*
        ├── handler.rs          # POST/DELETE /v1/users, /v1/inbounds*, /v1/outbounds*, /v1/logger/restart
        └── routing.rs          # /v1/routing/*
```

> 文件名严格按上表，子 agent 不得自创层级（不要 `services/`、`domain/` 这些）。

---

## 4. proto 文件管理

### 4.1 复制策略

`build.rs` 启动时从 `proto/` 读取 `.proto`（**不**读 `$XRAY_SRC`）。proto 文件夹是 vendored 的，提交进 git。

首次填充 `proto/` 的方法（**只在初始化阶段做一次**，由实现 agent 执行）：

1. 源目录：`/home/fido/webdav/Collection/7-ServerInstall/2-proxy/xray/xray搭建/xray-src/`
2. 入口 proto（在 Python 版 `ROOT_PROTOS` 基础上**补足** Rust 静态编译需要显式列出的传递依赖）：

   ```
   app/stats/command/command.proto
   app/proxyman/command/command.proto
   app/log/command/config.proto
   app/router/command/command.proto
   common/protocol/user.proto
   common/protocol/headers.proto       # SecurityConfig (VMess)
   common/serial/typed_message.proto
   common/net/network.proto            # Network enum (test-route)
   core/config.proto                   # InboundHandlerConfig / OutboundHandlerConfig
   proxy/vless/account.proto
   proxy/vmess/account.proto
   proxy/trojan/config.proto
   proxy/shadowsocks/config.proto
   ```

   理由：Python 的 `discover_proto_deps` 是运行时递归 import；Rust `tonic-build` 也会自动跟随 import，但**显式列出**这几个 .proto 可以保证以下类型有 Rust 生成代码可用：
   - `xray.core.{InboundHandlerConfig, OutboundHandlerConfig}` — `add-inbound`/`add-outbound` 的请求体目标类型
   - `xray.common.protocol.SecurityConfig` — 构造 VMess `Account.security_settings`
   - `xray.common.net.Network` — `test-route` 的 enum 转换
3. 递归解析 `import "..."`，把所有传递依赖也拷过来，保持原相对路径。
4. **绝不修改 proto 内容**——包名、import 路径、字段都按上游原样。
5. 拷完后 `tree proto/ > /dev/null` 验证没有外部 import 找不到的情况。

### 4.2 build.rs 要求

```rust
// build.rs 关键约束
tonic_build::configure()
    .build_server(false)                            // 我们只做 client
    .build_client(true)
    .compile_protos(&[ALL_ENTRY_PROTOS], &["proto/"])?;
println!("cargo:rerun-if-changed=proto");
```

- **只**编译需要的入口（与 §4.1 ROOT_PROTOS 同），其他靠 import 自动拉。
- 不写额外 `field_attribute`/`type_attribute`（保持纯 prost 默认）。

### 4.3 proto.rs 模块汇总

```rust
// src/proto.rs
pub mod xray {
    pub mod common {
        pub mod net      { tonic::include_proto!("xray.common.net"); }
        pub mod protocol { tonic::include_proto!("xray.common.protocol"); }
        pub mod serial   { tonic::include_proto!("xray.common.serial"); }
    }
    pub mod core { tonic::include_proto!("xray.core"); }
    pub mod app {
        pub mod stats    { pub mod command { tonic::include_proto!("xray.app.stats.command"); } }
        pub mod proxyman { pub mod command { tonic::include_proto!("xray.app.proxyman.command"); } }
        pub mod log      { pub mod command { tonic::include_proto!("xray.app.log.command"); } }
        pub mod router   { pub mod command { tonic::include_proto!("xray.app.router.command"); } }
    }
    pub mod proxy {
        pub mod vless       { tonic::include_proto!("xray.proxy.vless"); }
        pub mod vmess       { tonic::include_proto!("xray.proxy.vmess"); }
        pub mod trojan      { tonic::include_proto!("xray.proxy.trojan"); }
        pub mod shadowsocks { tonic::include_proto!("xray.proxy.shadowsocks"); }
    }
}
```

> 实际 module 名按 `package` 定义里的来，编译报错就调整，**不要**改 proto 文件本身。

### 4.4 TypedMessage 序列化

Python 版用 `descriptor.full_name` 拿到类型名（如 `xray.proxy.vless.Account`）。Rust prost 没有运行时 descriptor。**约定**：在 `xray/account.rs` 里硬编码常量：

```rust
const VLESS_ACCOUNT_TYPE:  &str = "xray.proxy.vless.Account";
const VMESS_ACCOUNT_TYPE:  &str = "xray.proxy.vmess.Account";
const TROJAN_ACCOUNT_TYPE: &str = "xray.proxy.trojan.Account";
const SS_ACCOUNT_TYPE:     &str = "xray.proxy.shadowsocks.Account";
const ADD_USER_OP_TYPE:    &str = "xray.app.proxyman.command.AddUserOperation";
const REMOVE_USER_OP_TYPE: &str = "xray.app.proxyman.command.RemoveUserOperation";
```

构造 TypedMessage 用 `prost::Message::encode_to_vec()` 拿 bytes，type 字段填上述常量字符串。

注意：proto 里 `package` 行可能是 `xray.proxy.vless` 或 `v2ray.core.proxy.vless`（fork 不同）——以**实际 vendored 进 proto/ 的版本**为准。如果不是 `xray.*`，常量也对应改。

---

## 5. gRPC error → HTTP status 映射（强制，与 Python 一致）

| gRPC code | HTTP | 备注 |
|---|---|---|
| `NotFound` | 404 | |
| `AlreadyExists` | 409 | |
| `Unimplemented` | 501 | 节点版本不支持该 API |
| `Unavailable` | 502 | gRPC 通道不可达 |
| `Unknown` + details 含 `"not found"` (大小写不敏感) | 404 | 兼容老版 xray |
| `Unknown` + details 含 `"already exists"` | 409 | |
| `Unknown` 其他 | 502 | |
| 其余所有 code | 500 | |

错误响应 body：

```json
{ "ok": false, "code": "<gRPC_CODE_NAME>", "error": "<details>" }
```

`code` 字段是 gRPC 大写名（`NOT_FOUND` / `ALREADY_EXISTS` / `UNIMPLEMENTED` / `UNAVAILABLE` / `UNKNOWN` 等）。Rust `tonic::Code::Unauthenticated` 序列化时要转成 `UNAUTHENTICATED` 这种 SCREAMING_SNAKE_CASE。

非 gRPC 错误：
- `bridge_token` 不匹配 / 缺 `Authorization` → 401, body `{"ok":false,"code":"UNAUTHENTICATED","error":"..."}`
- 缺 `X-Node-Domain` 或 `X-Node-Token` → 400, body `{"ok":false,"code":"INVALID_ARGUMENT","error":"..."}`
- POST body 校验失败（`add-inbound` 缺 `tag`） → 422, body `{"ok":false,"code":"INVALID_ARGUMENT","error":"..."}`
- 其余 panic / 未知错误 → 500

---

## 6. HTTP API（与 Python 版逐字符一致）

### 6.1 通用 header

所有 `/v1/*` 路由：

| Header | 必填 | 默认 | 用途 |
|---|---|---|---|
| `Authorization: Bearer <BRIDGE_TOKEN>` | ✓ | — | bridge 自身鉴权（恒定时间比较） |
| `X-Node-Domain` | ✓ | — | 目标 xray 节点 FQDN |
| `X-Node-Token` | ✓ | — | 节点 API token（作为 gRPC metadata `x-api-token`） |
| `X-Node-Port` | ✗ | 443 | gRPC 端口 |
| `X-Node-Name` | ✗ | — | 仅 trace log 用 |

### 6.2 路由表

```
GET  /healthz                            → 200 {"ok":true}    (无认证, 不进 trace)

# stats（GET, response 带 result，不走 OperationResult envelope）
GET    /v1/sys                           → SysStatsResponse
GET    /v1/users                         → string[]            (有流量记录的)
GET    /v1/users/online                  → string[]            (在线)
GET    /v1/users/stats?reset=<bool>      → [{email,uplink,downlink}]
GET    /v1/stats?pattern=<s>&reset=<b>   → StatRecord[]
GET    /v1/stats/{name}                  → StatRecord          (name 含 ">>>" 用 path 参数, axum :path)

# handler
POST   /v1/users                         body=AddUserRequest        → 201 OperationResult
DELETE /v1/users/{email}?tag=<s>&ignore_missing=<bool>             → 200 OperationResult
GET    /v1/inbounds                      → [{tag}]
POST   /v1/inbounds                      body={config:{tag,...}}    → 201 OperationResult
DELETE /v1/inbounds/{tag}                → 200 OperationResult
GET    /v1/outbounds                     → [{tag}]
POST   /v1/outbounds                     body={config:{tag,...}}    → 201 OperationResult
DELETE /v1/outbounds/{tag}               → 200 OperationResult
POST   /v1/logger/restart                → 200 OperationResult

# routing
POST   /v1/routing/test                  body=TestRouteRequest      → 200 dict
GET    /v1/routing/rules                 → [{tag,ruleTag}]
DELETE /v1/routing/rules/{tag}           → 200 OperationResult
GET    /v1/routing/balancers/{tag}       → {override_target, principle_targets}
PUT    /v1/routing/balancers/{tag}       body={target}              → 200 OperationResult
```

### 6.3 OperationResult schema

```json
{
  "ok": true,
  "action": "<verb-noun>",
  "persisted": false,
  "detail": { /* 各 action 自定义 */ }
}
```

`action` 字符串一律和 Python 版一致：`add-user` / `remove-user` / `add-inbound` / `remove-inbound` / `add-outbound` / `remove-outbound` / `restart-logger` / `remove-routing-rule` / `override-balancer-target`.

### 6.4 StatRecord schema

```json
{
  "name":     "user>>>alice@example.com>>>traffic>>>uplink",
  "parts":    ["user","alice@example.com","traffic","uplink"],
  "scope":    "user",            // null 当 parts 不足
  "id":       "alice@example.com",
  "category": "traffic",
  "metric":   "uplink",
  "value":    1234567
}
```

---

## 7. AddUserRequest 协议字段映射

`POST /v1/users` body：

```json
{
  "tag":            "vless-ws",      // required, inbound tag
  "email":          "u@x.com",       // required
  "proto":          "vless",         // vless|vmess|trojan|shadowsocks, default vless
  "level":          0,
  "uuid":           "...",           // vless/vmess required
  "flow":           "",              // vless: e.g. "xtls-rprx-vision"
  "vmess_security": "AUTO",          // AUTO|AES128_GCM|CHACHA20_POLY1305|NONE|ZERO
  "password":       "",              // trojan/shadowsocks required
  "ss_cipher":      "CHACHA20_POLY1305"  // AES_128_GCM|AES_256_GCM|CHACHA20_POLY1305|XCHACHA20_POLY1305|NONE
}
```

### 7.1 ss_cipher → enum int

| 字符串 | int |
|---|---|
| UNKNOWN | 0 |
| AES_128_GCM | 5 |
| AES_256_GCM | 6 |
| CHACHA20_POLY1305 | 7 |
| XCHACHA20_POLY1305 | 8 |
| NONE | 9 |

### 7.2 vmess_security → enum int

| 字符串 | int |
|---|---|
| UNKNOWN | 0 |
| AUTO | 2 |
| AES128_GCM | 3 |
| CHACHA20_POLY1305 | 4 |
| NONE | 5 |
| ZERO | 6 |

prost 生成的 enum 值会有同名常量（如 `SecurityType::Auto = 2`），优先用 enum 而不是 raw int，但 wire 上是 int，最终会被 tonic 序列化成同样的 byte。

### 7.3 校验规则（与 Python 一致）

- `proto = trojan|shadowsocks` 时 `password` 为空 → 422 `INVALID_ARGUMENT`。
- `proto` 不在白名单 → 422 `INVALID_ARGUMENT`。
- `add-inbound` / `add-outbound` body `config` 缺 `tag` → 422 `INVALID_ARGUMENT`。
- `proto = vless|vmess` 但 `uuid` 空：**与 Python 版行为一致**——不前置校验，让 xray 服务端报错（observation: Python 版没主动 reject 空 UUID，由 server 决定）。

---

## 8. XrayClientCache（节点 client 池）

### 8.1 行为

- key = `(domain, port, token)` 三元组（**token 入 key**——不同 token 视为不同 client）
- 容量上限 `DEFAULT_MAX_ENTRIES = 256`
- 命中时移到 LRU 末尾
- 容量满时 evict 最老条目并 log
- 移除 client 时 channel 自动 drop（tonic Channel 是 Arc 包装，最后一个 ref drop 即关）

### 8.2 实现要点

- 用 `tokio::sync::Mutex<lru::LruCache<...>>` 或 `parking_lot::Mutex<...>`
- **不要**在持有锁时 `.await`——构造 Channel 是同步的（lazy connect），所以 `get_or_create` 整个函数同步即可
- gRPC channel：`tonic::transport::Channel::from_static(...)` 不行（要 owned String），用 `Channel::builder(uri).tls_config(rustls::ClientConfig::default())?.connect_lazy()`，**lazy** 关键——首次 RPC 才建连

### 8.3 NodeContext extractor (axum)

写成 axum 的 `FromRequestParts`：从 header 解析出 `(domain, port, token, name?)`，调 cache 拿 `Arc<XrayClient>`，挂到 request extension 或直接构造 handler 参数。

---

## 9. AppState

```rust
#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub cache:    Arc<XrayClientCache>,
}
```

`Settings` 用 `serde` + `dotenvy` 在 main 里 `Settings::from_env()` 加载，失败立即 exit。

---

## 10. main.rs 入口

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    init_tracing();                      // env_filter=info,xray_bridge=debug
    let settings = Settings::from_env()?;
    let state = AppState::new(settings.clone());
    let app = routes::build_router(state);
    let addr = SocketAddr::new(IpAddr::from([0,0,0,0]), settings.port);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("xray-bridge listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
```

环境变量：
- `BRIDGE_TOKEN` (必填)
- `PORT` (默认 8080)
- `RUST_LOG` (默认 `info,xray_bridge=debug`)

---

## 11. 编码规范

- **Edition 2021**, MSRV = 当前 stable rustc
- **Rustfmt 默认**配置（不写自定义 rustfmt.toml，除非必要）
- **Clippy** `cargo clippy --all-targets -- -D warnings` 必须 pass
- 错误：库内部 `thiserror::Error`，handler 返回 `Result<Json<T>, AppError>`，`AppError: IntoResponse`
- 日志：每个 RPC 至少一行 `tracing::debug!("xray rpc {method} target={target}")`，错误用 `warn!`
- 注释：写 *为什么*，不写 *做什么*。函数名能说清楚的就不写文档。
- **禁止 `unwrap()` / `expect()`**：除了启动期 `Settings::from_env` 等明确 fail-fast 场景
- **禁止 panic 路径**：handler 返回 `Result`，永远不 panic

---

## 12. 构建与发布

### 12.1 本地开发

```bash
cargo build
cargo run
# 或
RUST_LOG=debug cargo watch -x run
```

### 12.2 Release 静态二进制（部署用）

```bash
# 一次性安装 musl target 和 工具
rustup target add x86_64-unknown-linux-musl
sudo apt install musl-tools                   # Debian/Ubuntu

cargo build --release --target x86_64-unknown-linux-musl
file target/x86_64-unknown-linux-musl/release/xray-bridge
# → ELF 64-bit LSB executable, x86-64, ..., statically linked
```

部署：直接 `scp target/.../release/xray-bridge` 到 Debian 10/11/12 服务器，`./xray-bridge` 即可。

### 12.3 Cargo.toml 关键 features

```toml
[dependencies]
tonic           = { version = "0.12", default-features = false, features = ["transport","prost","tls","tls-roots","tls-webpki-roots","codegen"] }
prost           = "0.13"
prost-types     = "0.13"
base64          = "0.22"     # Inbound/Outbound config 里的 TypedMessage.value
tokio           = { version = "1", features = ["macros","rt-multi-thread","net","signal"] }
axum            = { version = "0.7", default-features = false, features = ["json","tokio","http1","matched-path","query","tracing"] }
tower           = "0.5"
tower-http      = { version = "0.6", features = ["trace"] }
serde           = { version = "1", features = ["derive"] }
serde_json      = "1"
tracing         = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter","fmt"] }
dotenvy         = "0.15"
anyhow          = "1"
thiserror       = "1"
lru             = "0.12"
parking_lot     = "0.12"
http            = "1"
bytes           = "1"
async-trait     = "0.1"

[build-dependencies]
tonic-build     = { version = "0.12", default-features = false, features = ["prost","transport"] }

[profile.release]
lto             = true
codegen-units   = 1
strip           = "symbols"
opt-level       = 3
```

`Cargo.lock` 入库。

---

## 13. 测试要求

实现 agent 写完后，至少要：

1. `cargo build --release` 通过
2. `cargo build --release --target x86_64-unknown-linux-musl` 通过
3. `cargo clippy --all-targets -- -D warnings` 通过
4. `cargo fmt --check` 通过
5. `cargo test`（含至少这些单元测试，**用 `#[cfg(test)]` 写在源文件里**或 `tests/` 目录）：
   - `error::grpc_to_http`：每个映射条目至少 1 个测试用例
   - `xray::stats::parse_stat_name`：完整 4 段、3 段、2 段、1 段、空串
   - `auth::bearer_auth`：missing / wrong scheme / wrong token / right token
   - `nodes::XrayClientCache`：put/get/lru-evict
   - `routes::*`：用 `axum::http::Request` + `tower::ServiceExt::oneshot` 测试 router 在缺 header 时返回 400、缺 token 返回 401

6. **冒烟测试脚本**：`scripts/smoke.sh`（bash）——参照 `cf-xray-bridge/快速使用手册.md` 的 curl 链路，把环境变量化，文档说明需要真实 xray 节点才能跑全部、否则跑 healthz + 401 路径。

校验 agent 必须把上面这些命令真跑一遍并贴输出。

---

## 14. 不要做的事

- ❌ 不要引入 OpenSSL / native-tls
- ❌ 不要在 build.rs 里读 `XRAY_SRC` 环境变量——proto 已 vendored
- ❌ 不要写 Dockerfile（除非 §1.1 之外用户明确要求）——本项目主打裸二进制部署
- ❌ 不要写 README 之外的额外 .md 文档（除非用户明确要求）
- ❌ 不要 panic！handler 永远 `Result`
- ❌ 不要 `unwrap()` 在 RPC 路径上
- ❌ 不要换 HTTP 路由路径、字段名、错误码——见 §1.2
- ❌ 不要把节点 token 写日志（log domain/port/name 可以，token 绝不）
- ❌ 不要在 cache key 里只用 (domain, port)——必须含 token
- ❌ 不要给 axum router 加 `/api` 之类的前缀——`/v1` 直接挂根

---

## 14a. JSON → Protobuf 反序列化策略

`POST /v1/inbounds` / `POST /v1/outbounds` 请求体里的 `config` 字段是任意 JSON 对象，需要装到 `xray.core.InboundHandlerConfig` / `OutboundHandlerConfig`。Python 用 `google.protobuf.json_format.ParseDict` 自动转。Rust prost **没有运行时 JSON↔proto**，且不要为此引入大依赖（`prost-reflect` / `pbjson` 都偏重）。

**约定**（与 Python 行为一致，但更显式）：

- Handler 把 `config` 接收为 `serde_json::Value`。
- 必须含 `"tag"` 字符串字段（缺则 422 `INVALID_ARGUMENT`）。
- 可选字段 `"receiver_settings"` 与 `"proxy_settings"`：每个是一个 object，必须形如：

  ```json
  {
    "type":  "xray.app.proxyman.ReceiverConfig",
    "value": "<base64-encoded protobuf bytes>"
  }
  ```

  对应到 `xray.common.serial.TypedMessage { type, value }`。`value` 用 `base64::engine::general_purpose::STANDARD.decode()` 还原；解码失败 → 422 `INVALID_ARGUMENT`。

- 实现位置：`src/routes/handler.rs` 内私有辅助函数 `json_to_inbound_config(v: serde_json::Value) -> Result<InboundHandlerConfig, AppError>` / `json_to_outbound_config(...)`. 不要散到别处。
- 调用方（CF Worker）若要传纯 JSON 内嵌 proto，就由调用方先 base64 编码——这是 Python 版本就要求的同一契约（json_format 对 `bytes` 字段也是 base64）。

> 这条契约**就是** Python 版的隐式行为，文档化它而已；不构成行为变更。

---

## 15. 参考实现（Python）

见 `/home/fido/work/2026/cf-xray-bridge/cf_xray_bridge/`。

具体到字段映射、JSON 字段名时**以 Python 版为单一事实源**。如果 CLAUDE.md 与 Python 版冲突，先 stop，把冲突报给用户。

---

## 16. 子 agent 工作纪律

实现 agent：
1. 先读这份 CLAUDE.md 全文 + Python 版源码（`cf_xray_bridge/` 整个目录）。
2. 拷贝 proto 文件，写 `Cargo.toml` / `build.rs`，确保 `cargo check` 通过。
3. 按 §3 目录结构补全代码。
4. **每完成一个模块跑一次 `cargo build`** ——不要写完所有代码再编译。
5. 写单元测试（§13）。
6. 在 README 里写：项目目标、本地启动命令、release 静态构建命令、curl 例子（参照 Python 版 `快速使用手册.md`）。

校验 agent：
1. 不写实现代码，只读、跑命令、报告。
2. 跑 §13 的 1–5 项，把每条命令的真实输出贴进报告。
3. 抽查路由表（§6.2）每一条是否在 `routes/` 里实现，缺哪条直接列出来。
4. 抽查错误映射（§5）：搜代码看是否覆盖每个 case。
5. 抽查 unwrap/expect/panic：`grep -rn 'unwrap\|expect\|panic!' src/`，列出所有命中并判断是否合规（启动期 OK，handler 路径不行）。
6. 报告格式：
   - ✅ 通过项
   - ❌ 失败项（带具体证据：文件:行号 / 命令输出）
   - ⚠️ 警告（不阻塞但建议改的）
