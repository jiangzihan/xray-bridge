// Prost-generated protobuf types for all Xray gRPC services.
// All include_proto! blocks are wrapped in allow(clippy::all) to suppress
// warnings in generated code that we cannot modify.

#[allow(clippy::all)]
pub mod xray {
    pub mod common {
        pub mod net {
            tonic::include_proto!("xray.common.net");
        }
        pub mod protocol {
            tonic::include_proto!("xray.common.protocol");
        }
        pub mod serial {
            tonic::include_proto!("xray.common.serial");
        }
        pub mod geodata {
            tonic::include_proto!("xray.common.geodata");
        }
    }
    pub mod core {
        tonic::include_proto!("xray.core");
    }
    pub mod app {
        pub mod stats {
            pub mod command {
                tonic::include_proto!("xray.app.stats.command");
            }
        }
        pub mod proxyman {
            tonic::include_proto!("xray.app.proxyman");
            pub mod command {
                tonic::include_proto!("xray.app.proxyman.command");
            }
        }
        pub mod log {
            pub mod command {
                tonic::include_proto!("xray.app.log.command");
            }
        }
        pub mod router {
            pub mod command {
                tonic::include_proto!("xray.app.router.command");
            }
        }
    }
    pub mod proxy {
        pub mod vless {
            tonic::include_proto!("xray.proxy.vless");
        }
        pub mod vmess {
            tonic::include_proto!("xray.proxy.vmess");
        }
        pub mod trojan {
            tonic::include_proto!("xray.proxy.trojan");
        }
        pub mod shadowsocks {
            tonic::include_proto!("xray.proxy.shadowsocks");
        }
    }
    pub mod transport {
        pub mod internet {
            tonic::include_proto!("xray.transport.internet");
        }
    }
}
