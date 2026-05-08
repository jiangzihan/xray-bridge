# CI/CD 方案与快速使用

本项目用 **GitHub Actions** 做 CI/CD，工作流就两个：

| 文件 | 触发 | 用途 | 耗时 |
|---|---|---|---|
| `.github/workflows/ci.yml` | push 到 main / 任何 PR | 代码质量检查 (fmt + clippy + test + build) | ~3 分钟 |
| `.github/workflows/release.yml` | 推 tag `v*` | 交叉编译 4 平台静态二进制 → 发到 GitHub Releases | ~10 分钟 |

不构建 Docker 镜像（Railway 直接拉 GitHub repo 自己 build）。

---

## 1. CI 工作流（`ci.yml`）

每次 push 或 PR 自动跑，失败会 block 合并。

### 检查项

| 步骤 | 命令 | 失败影响 |
|---|---|---|
| 格式 | `cargo fmt --all -- --check` | 代码风格不统一 |
| Clippy | `cargo clippy --all-targets --all-features -- -D warnings` | 有 lint 警告 |
| 单元测试 | `cargo test --all-features` | 测试不过 |
| 编译 | `cargo build --all-features` | 不能编译 |

### 关键设计

- **`RUSTFLAGS=-D warnings` 不在 env 全局设**：否则 prost 生成代码里的 `dead_code` 会让 `cargo build/test` 失败。严格检查**只在 clippy 步骤**用 `-- -D warnings`，clippy 默认跳过 `OUT_DIR` 里的生成代码。
- **`Swatinem/rust-cache@v2`**：缓存 `~/.cargo` 和 `target/`，重跑快 5–10 倍。
- **不安装 `protoc`**：`build.rs` 用 `protoc-bin-vendored` 自带 protoc 二进制。这对 cross-compile 至关重要——`cross` 在 Docker 容器里跑，看不到 host 上装的 protoc。vendored 方案各环境（host / cross / 本地）行为一致。

---

## 2. Release 工作流（`release.yml`）

打 tag 触发，并行编译 4 个 Linux 目标平台并自动发 Release。

### 产物（每次 release 4 个 tar.gz + 4 个 .sha256）

| Target | 用途 |
|---|---|
| `x86_64-unknown-linux-gnu` | 通用 Linux 服务器（动态链接 glibc） |
| `x86_64-unknown-linux-musl` | 静态链接，**Alpine / 任意 Linux 都能跑** |
| `aarch64-unknown-linux-gnu` | ARM64 服务器（树莓派、Oracle ARM 实例） |
| `aarch64-unknown-linux-musl` | ARM64 静态版 |

`taiki-e/upload-rust-binary-action@v1` 一行解决交叉编译 + strip + 打包 + SHA256 + 上传。

---

## 3. 快速使用

### 一次性准备

```bash
# 1. 仓库 GitHub 设置开 Actions 写权限 (release.yml 创建 Release 必需)
#    Settings → Actions → General → Workflow permissions
#    选 "Read and write permissions"

# 2. (可选) 本地装 cargo 工具, 推送前先在本地过一遍 CI 检查
rustup component add rustfmt clippy
```

### 日常开发：写代码 + 推送

```bash
# 推送前本地校验 (跟 CI 检查项完全一致)
cargo fmt --all                                    # 自动修格式
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# 推送
git add .
git commit -m "feat: 新功能"
git push                  # ← 触发 ci.yml
```

GitHub Actions 页面 ~3 分钟跑完。失败时点进去看日志。

### 发布版本：打 tag

```bash
# 用语义化版本号
git tag -a v0.1.0 -m "first release"
git push origin v0.1.0    # ← 触发 release.yml
```

10 分钟后，仓库 **Releases** 页出现 v0.1.0，含 4 个二进制 tar.gz。

### 用户下载使用

```bash
# 选你环境的版本 (Alpine / Debian / Ubuntu / RHEL 任意一个推荐 musl 版)
curl -LO https://github.com/<owner>/xray-bridge/releases/latest/download/xray-bridge-v0.1.0-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/<owner>/xray-bridge/releases/latest/download/xray-bridge-v0.1.0-x86_64-unknown-linux-musl.tar.gz.sha256

# 校验 SHA256
sha256sum -c xray-bridge-v0.1.0-x86_64-unknown-linux-musl.tar.gz.sha256

# 解压运行
tar xzf xray-bridge-v0.1.0-x86_64-unknown-linux-musl.tar.gz
./xray-bridge --help
```

---

## 4. 版本号约定（语义化）

| Tag 格式 | 含义 | 何时用 |
|---|---|---|
| `v0.1.0` → `v0.1.1` | PATCH | 修 bug，无 API 变化 |
| `v0.1.x` → `v0.2.0` | MINOR | 新功能，向后兼容 |
| `v0.x.x` → `v1.0.0` | MAJOR | 不兼容改动 / 第一个稳定版 |

`v0.x.x` 视为 alpha/beta，可以随便破坏兼容。`v1.0.0` 后必须严格遵守 semver。

### 预发布版

```bash
git tag -a v0.2.0-rc.1 -m "release candidate"
git push origin v0.2.0-rc.1
```

`taiki-e/upload-rust-binary-action` 会识别 `-rc.X` / `-beta.X` 等后缀，标记为 prerelease。

---

## 5. 常见问题排查

### CI 红了

| 报错 | 解决 |
|---|---|
| `cargo fmt` 失败 | 本地跑 `cargo fmt --all` 后重 push |
| Clippy `dead_code` (你写的代码) | 修代码 / 加 `#[allow(dead_code)]` 显式标注意图 |
| Clippy `dead_code` (生成代码) | 不该出现；如果出现是 `proto.rs` 模块属性丢了，确认有 `#[allow(dead_code)]` |
| `cargo test` 失败 | 看具体 test 报错 |
| `protoc` 找不到 (host 上) | 不该出现；`build.rs` 已用 `protoc-bin-vendored` 自带，详见第 6 章 |

### Release 红了

| 报错 | 解决 |
|---|---|
| `403 Forbidden` 创建 Release | 仓库 Settings → Actions → General → 选 "Read and write permissions" |
| 某 target 编译失败 | 看具体哪个 target 在 matrix 里红的；通常是依赖不支持该平台，删掉对应行 |
| `tag already exists` | tag 同名重新发：先 `git tag -d vX.Y.Z && git push origin :refs/tags/vX.Y.Z`，再重新打 |

### 想强制触发某次 release 重跑

GitHub Actions 页面 → 选 release run → 右上 "Re-run jobs"。

---

## 6. 关键技术：vendored protoc 解决跨平台编译

**这是本项目踩过最大的坑，单独立一节防止再忘。**

### 现象

`release.yml` 跑 `aarch64-unknown-linux-musl` 等 musl/ARM target 时炸：

```
error: failed to run custom build command for `xray-bridge`
--- stderr
Error: Could not find `protoc`. If `protoc` is installed, try setting
the `PROTOC` environment variable to the path of the `protoc` binary.
```

但 `ci.yml`（普通 x86_64 native build）一切正常，host 上明明装了 `protobuf-compiler`。

### 根因

`taiki-e/upload-rust-binary-action` 对**非原生 target**（musl / aarch64）默认走 [`cross`](https://github.com/cross-rs/cross)，**`cross` 是用 Docker 容器做交叉编译**：

```
GitHub Runner (Ubuntu, 装了 protoc)
    │
    └── docker run cross-rs/aarch64-unknown-linux-musl ...
                  │
                  └── 容器里 cargo build → build.rs 调 protoc → 找不到!
```

容器是个最小 musl 编译环境，**没装 protoc**，host 上的 protoc 进不去容器。`apt install protobuf-compiler` 只装到 host，对容器无效。

### 解决：让 protoc 跟着 cargo 走

**用 [`protoc-bin-vendored`](https://crates.io/crates/protoc-bin-vendored) crate**：把多平台的 protoc 二进制打包进 Rust crate，cargo 自动按 host 平台拿对应的 binary。Build.rs 启动时把 `PROTOC` 环境变量指向 vendored 二进制——容器、native、cross-rs、本地都一致。

#### 配置

**`Cargo.toml`**:
```toml
[build-dependencies]
tonic-build = { version = "0.12", default-features = false, features = ["prost", "transport"] }
protoc-bin-vendored = "3"   # 关键: 让 cargo 自动获取 protoc
```

**`build.rs`**:
```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 关键一行: 用 vendored protoc, 容器/host/cross 行为一致
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(/* ... */)?;
    Ok(())
}
```

#### 配套：CI workflow 不再需要装 protoc

```yaml
# ❌ 旧 (依赖 host protoc, 对 cross 容器无效)
- name: Install protoc
  run: sudo apt-get install -y protobuf-compiler

# ✅ 新 (vendored 自带, 删掉这步)
- uses: actions-rust-lang/setup-rust-toolchain@v1
  ...
```

### 为什么不选其他方案

| 方案 | 评价 | 否决理由 |
|---|---|---|
| **vendored protoc**（采用） | ✅ 容器/host 一致、零 CI 配置、各 OS 都跑通 | — |
| host 装 + cross volume mount | ⚠️ 可行 | 要写 cross 配置 + 维护成本 |
| 自定义 cross Dockerfile（每 target 一个） | ❌ | 维护噩梦 |
| 切到 `cargo-zigbuild` | ⚠️ 可行 | Zig 跨编对某些 C 依赖支持差，且 cross-rs 更主流 |
| **预生成 .rs 文件提交 git** | ❌ | 失去 build.rs 自动重生成的好处 |

### 副作用 / 注意事项

- ✅ **不影响最终 release binary**：`protoc-bin-vendored` 是 `[build-dependencies]`，编译完丢弃。
- ✅ 跨平台都能跑：`protoc-bin-vendored` 内置 Linux/macOS/Windows 二进制
- ⚠️ 第一次本地 `cargo build` 会下载几 MB 的 protoc 二进制（cargo 自己 cache，之后秒过）
- ⚠️ 用 vendored 后**不要再** `apt install protobuf-compiler`——两个 protoc 同时存在虽不会冲突，但容易让 PATH 分歧产生奇怪 bug。

### 一句话总结

> **build.rs 用 `std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?)` 取代依赖 system protoc，cross-compile 永不再痛。**

---

## 7. 设计权衡（FYI）

### 为什么不要 Docker 镜像 workflow？

- Railway 直接 deploy from GitHub repo + Dockerfile：自动 build，一次 push 一次部署
- 没有"分发镜像给其他人用"的需求
- 加 docker.yml 反而多一份维护

如果以后真要发 ghcr.io 镜像（让别人 `docker run ghcr.io/<you>/xray-bridge`），单独加个 `docker.yml` 即可。

### 为什么 4 个 target 都是 Linux？

- 项目是部署在 Linux VPS 上的 bridge
- Windows / macOS 跑这服务的可能性 ~ 0
- 加这俩平台只会拖慢 release，不带价值

### 为什么不集成测试节点？

- 集成测试要真实 xray 节点 + token，CI 里跑不了
- 集成测试在本地 / staging 环境手动跑
- CI 只验证编译 + 单元测试

---

## 8. 文件清单

```
.github/
└── workflows/
    ├── ci.yml          # PR/push 检查
    └── release.yml     # tag 发布
```

只有这两个文件。删除任一个都会失去对应功能；改写时注意 YAML 缩进。
