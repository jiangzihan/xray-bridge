fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 用 vendored protoc 二进制, 避免 cross-compile 容器 / 各种 CI 环境
    // 找不到 protoc 的问题. 不影响最终 release binary, 只是 build-time 工具.
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);

    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(
            &[
                "proto/app/stats/command/command.proto",
                "proto/app/proxyman/command/command.proto",
                "proto/app/log/command/config.proto",
                "proto/app/router/command/command.proto",
                "proto/common/protocol/user.proto",
                "proto/common/protocol/headers.proto",
                "proto/common/serial/typed_message.proto",
                "proto/common/net/network.proto",
                "proto/core/config.proto",
                "proto/proxy/vless/account.proto",
                "proto/proxy/vmess/account.proto",
                "proto/proxy/trojan/config.proto",
                "proto/proxy/shadowsocks/config.proto",
            ],
            &["proto/"],
        )?;
    println!("cargo:rerun-if-changed=proto");
    Ok(())
}
