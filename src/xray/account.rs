// Build protocol-specific account TypedMessage objects for add_user.
// Type name constants match the package declarations in vendored .proto files.
// See CLAUDE.md §4.4 for the rationale behind hardcoded type name strings.

use prost::Message;

use crate::error::AppError;
use crate::proto::xray::{
    common::{
        protocol::{SecurityConfig, SecurityType},
        serial::TypedMessage,
    },
    proxy::{
        shadowsocks::{self, CipherType},
        trojan, vless, vmess,
    },
};

// Full protobuf type names as declared in the vendored proto files.
// These must match the `package` + message name exactly.
const VLESS_ACCOUNT_TYPE: &str = "xray.proxy.vless.Account";
const VMESS_ACCOUNT_TYPE: &str = "xray.proxy.vmess.Account";
const TROJAN_ACCOUNT_TYPE: &str = "xray.proxy.trojan.Account";
const SS_ACCOUNT_TYPE: &str = "xray.proxy.shadowsocks.Account";
pub const ADD_USER_OP_TYPE: &str = "xray.app.proxyman.command.AddUserOperation";
pub const REMOVE_USER_OP_TYPE: &str = "xray.app.proxyman.command.RemoveUserOperation";

/// Build a `TypedMessage` wrapping the protocol-specific account protobuf.
/// Called by `xray/handler.rs` when constructing the `AddUserOperation`.
pub fn build_account_typed_message(
    proto: &str,
    uuid: &str,
    flow: &str,
    vmess_security: &str,
    password: &str,
    ss_cipher: &str,
) -> Result<TypedMessage, AppError> {
    match proto {
        "vless" => {
            let account = vless::Account {
                id: uuid.to_owned(),
                flow: flow.to_owned(),
                encryption: "none".to_owned(),
                ..Default::default()
            };
            Ok(TypedMessage {
                r#type: VLESS_ACCOUNT_TYPE.to_owned(),
                value: account.encode_to_vec(),
            })
        }
        "vmess" => {
            let sec_int = vmess_security_to_int(vmess_security);
            let account = vmess::Account {
                id: uuid.to_owned(),
                security_settings: Some(SecurityConfig { r#type: sec_int }),
                ..Default::default()
            };
            Ok(TypedMessage {
                r#type: VMESS_ACCOUNT_TYPE.to_owned(),
                value: account.encode_to_vec(),
            })
        }
        "trojan" => {
            if password.is_empty() {
                return Err(AppError::UnprocessableEntity(
                    "Trojan protocol requires password".to_owned(),
                ));
            }
            let account = trojan::Account {
                password: password.to_owned(),
            };
            Ok(TypedMessage {
                r#type: TROJAN_ACCOUNT_TYPE.to_owned(),
                value: account.encode_to_vec(),
            })
        }
        "shadowsocks" => {
            if password.is_empty() {
                return Err(AppError::UnprocessableEntity(
                    "Shadowsocks protocol requires password".to_owned(),
                ));
            }
            let cipher_int = ss_cipher_to_int(ss_cipher);
            let account = shadowsocks::Account {
                password: password.to_owned(),
                cipher_type: cipher_int,
                ..Default::default()
            };
            Ok(TypedMessage {
                r#type: SS_ACCOUNT_TYPE.to_owned(),
                value: account.encode_to_vec(),
            })
        }
        other => Err(AppError::UnprocessableEntity(format!(
            "Unknown protocol '{other}'. Supported: vless, vmess, trojan, shadowsocks"
        ))),
    }
}

/// Map VMess security name string to SecurityType enum int.
/// Matches CLAUDE.md §7.2 and Python's _VMESS_SECURITY_MAP.
fn vmess_security_to_int(name: &str) -> i32 {
    match name.to_uppercase().as_str() {
        "AUTO" => SecurityType::Auto as i32,
        "AES128_GCM" => SecurityType::Aes128Gcm as i32,
        "CHACHA20_POLY1305" => SecurityType::Chacha20Poly1305 as i32,
        "NONE" => SecurityType::None as i32,
        "ZERO" => SecurityType::Zero as i32,
        _ => SecurityType::Auto as i32, // default to AUTO per Python behavior
    }
}

/// Map Shadowsocks cipher name string to CipherType enum int.
/// Matches CLAUDE.md §7.1 and Python's _SS_CIPHER_MAP.
fn ss_cipher_to_int(name: &str) -> i32 {
    match name.to_uppercase().as_str() {
        "AES_128_GCM" => CipherType::Aes128Gcm as i32,
        "AES_256_GCM" => CipherType::Aes256Gcm as i32,
        "CHACHA20_POLY1305" => CipherType::Chacha20Poly1305 as i32,
        "XCHACHA20_POLY1305" => CipherType::Xchacha20Poly1305 as i32,
        "NONE" => CipherType::None as i32,
        _ => CipherType::Chacha20Poly1305 as i32, // default per Python
    }
}
