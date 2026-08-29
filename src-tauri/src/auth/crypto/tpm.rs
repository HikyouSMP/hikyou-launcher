//! Windows TPM バックエンド
//!
//! NCrypt API (MS_PLATFORM_CRYPTO_PROVIDER) + AES-256-GCM ハイブリッド暗号化。
//!
//! ┌ セキュリティ特性 ────────────────────────────────────────────────────────┐
//! │ RSA 秘密鍵はTPMハードウェア内にのみ存在し、外部に出ない。               │
//! │ ストレージファイルを盗んでも別マシンでは復号不可。                       │
//! │ AES 鍵は使用直後に Zeroizing<> で自動ゼロ化。                          │
//! └──────────────────────────────────────────────────────────────────────────┘
//!
//! ファイル形式:
//!   [4B]    マジック b"HTPM"
//!   [4B LE] RSA-OAEP ラップ済み AES 鍵の長さ (N)
//!   [N B]   RSA-OAEP(SHA-256) でラップされた AES-256 鍵
//!   [12B]   AES-GCM ノンス
//!   [残り]  AES-256-GCM 暗号文 (末尾16B = GCM 認証タグ)

use super::{
    SecureStorage, SecureStorageDiagnostics,
    diagnostics::{SecurityMeasurement, now_unix_ms},
};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use rand::RngCore;
use std::ptr;
use windows::Win32::Foundation::BOOL;
use windows::Win32::Security::Cryptography::{
    BCRYPT_OAEP_PADDING_INFO, CERT_KEY_SPEC, NCRYPT_ALLOW_ARCHIVING_FLAG, NCRYPT_ALLOW_EXPORT_FLAG,
    NCRYPT_ALLOW_PLAINTEXT_ARCHIVING_FLAG, NCRYPT_ALLOW_PLAINTEXT_EXPORT_FLAG,
    NCRYPT_EXPORT_POLICY_PROPERTY, NCRYPT_FLAGS, NCRYPT_HANDLE, NCRYPT_IMPL_HARDWARE_FLAG,
    NCRYPT_IMPL_TYPE_PROPERTY, NCRYPT_IMPL_VIRTUAL_ISOLATION_FLAG, NCRYPT_KEY_HANDLE,
    NCRYPT_PCP_PLATFORM_BINDING_PCRMASK_PROPERTY, NCRYPT_PROV_HANDLE,
    NCRYPT_SECURITY_DESCR_PROPERTY, NCRYPT_SECURITY_DESCR_SUPPORT_PROPERTY,
    NCRYPT_UI_FORCE_HIGH_PROTECTION_FLAG, NCRYPT_UI_POLICY_PROPERTY, NCRYPT_UI_PROTECT_KEY_FLAG,
    NCryptCreatePersistedKey, NCryptDecrypt, NCryptEncrypt, NCryptFinalizeKey, NCryptFreeObject,
    NCryptGetProperty, NCryptOpenKey, NCryptOpenStorageProvider, NCryptSetProperty,
};
use windows::Win32::Security::{
    ACL, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, IsValidSecurityDescriptor,
    PSECURITY_DESCRIPTOR, PSID,
};
use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, OBJECT_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
};
use windows::core::PCWSTR;
use zeroize::Zeroizing;

// ── 定数 ─────────────────────────────────────────────────────────────────────

const TPM_PROVIDER: PCWSTR = windows::core::w!("Microsoft Platform Crypto Provider");
const PROP_LENGTH: PCWSTR = windows::core::w!("Length");
const KEY_NAME: PCWSTR = windows::core::w!("hikyou-launcher-token-key");
const ALG_RSA: PCWSTR = windows::core::w!("RSA");
const HASH_SHA256: PCWSTR = windows::core::w!("SHA256");

/// UI プロンプトなし = 0x40
const SILENT: NCRYPT_FLAGS = NCRYPT_FLAGS(0x0000_0040);
/// OAEP パディング = 0x04
const PAD_OAEP: NCRYPT_FLAGS = NCRYPT_FLAGS(0x0000_0004);

const RSA_KEY_BITS: u32 = 2048;

// ── RAII ハンドルラッパー ─────────────────────────────────────────────────────
// NCRYPT_HANDLE(raw) を直接構築して NCryptFreeObject に渡す。
// これにより Param<NCRYPT_HANDLE, CopyType> 制約を明示的に満たす。

struct ProvHandle(NCRYPT_PROV_HANDLE);
impl Drop for ProvHandle {
    fn drop(&mut self) {
        if self.0.0 != 0 {
            unsafe {
                let _ = NCryptFreeObject(NCRYPT_HANDLE(self.0.0));
            }
        }
    }
}

struct KeyHandle(NCRYPT_KEY_HANDLE);
impl Drop for KeyHandle {
    fn drop(&mut self) {
        if self.0.0 != 0 {
            unsafe {
                let _ = NCryptFreeObject(NCRYPT_HANDLE(self.0.0));
            }
        }
    }
}

// ── プロバイダ / 鍵操作 ───────────────────────────────────────────────────────

fn open_provider() -> Result<ProvHandle, String> {
    unsafe {
        let mut prov = NCRYPT_PROV_HANDLE::default();
        NCryptOpenStorageProvider(&mut prov, TPM_PROVIDER, 0)
            .map_err(|e| format!("TPM provider open failed: {}", e))?;
        Ok(ProvHandle(prov))
    }
}

fn get_or_create_key(prov: &ProvHandle) -> Result<KeyHandle, String> {
    unsafe {
        let mut key = NCRYPT_KEY_HANDLE::default();

        // 既存の鍵を開く（CERT_KEY_SPEC(0) = AT_NONE, UI なし）
        if NCryptOpenKey(prov.0, &mut key, KEY_NAME, CERT_KEY_SPEC(0u32), SILENT).is_ok() {
            return Ok(KeyHandle(key));
        }

        // 新規作成（CERT_KEY_SPEC(0) = AT_NONE, dwFlags = 0）
        NCryptCreatePersistedKey(
            prov.0,
            &mut key,
            ALG_RSA,
            KEY_NAME,
            CERT_KEY_SPEC(0u32),
            NCRYPT_FLAGS(0),
        )
        .map_err(|e| format!("TPM key creation failed: {}", e))?;

        // 鍵サイズ設定 — windows-rs 0.58 は (ptr, len) の代わりに &[u8] を受け取る
        // NCryptSetProperty の第1引数は NCRYPT_HANDLE
        NCryptSetProperty(
            NCRYPT_HANDLE(key.0),
            PROP_LENGTH,
            &RSA_KEY_BITS.to_le_bytes(),
            NCRYPT_FLAGS(0),
        )
        .map_err(|e| format!("TPM key size update failed: {}", e))?;

        // 鍵を TPM に書き込んで確定させる
        NCryptFinalizeKey(key, SILENT).map_err(|e| format!("TPM key finalize failed: {}", e))?;

        Ok(KeyHandle(key))
    }
}

fn get_property_bytes(
    object: NCRYPT_HANDLE,
    property: PCWSTR,
    flags: OBJECT_SECURITY_INFORMATION,
) -> Result<Vec<u8>, String> {
    unsafe {
        let mut size = 0u32;
        NCryptGetProperty(object, property, None, &mut size, flags)
            .map_err(|error| error.to_string())?;

        if size == 0 {
            return Ok(Vec::new());
        }
        let mut bytes = vec![0u8; size as usize];
        let mut returned = 0u32;
        NCryptGetProperty(object, property, Some(&mut bytes), &mut returned, flags)
            .map_err(|error| error.to_string())?;
        bytes.truncate(returned as usize);
        Ok(bytes)
    }
}

fn get_dword_property(
    object: NCRYPT_HANDLE,
    property: PCWSTR,
    flags: OBJECT_SECURITY_INFORMATION,
) -> Result<u32, String> {
    let bytes = get_property_bytes(object, property, flags)?;
    let value = bytes
        .get(..std::mem::size_of::<u32>())
        .ok_or("property returned fewer than four bytes")?
        .try_into()
        .map_err(|_| "property has an invalid DWORD representation")?;
    Ok(u32::from_le_bytes(value))
}

fn inspect_security_descriptor(bytes: &mut [u8]) -> Result<String, String> {
    if bytes.is_empty() {
        return Err("security descriptor is empty".to_string());
    }

    unsafe {
        let descriptor = PSECURITY_DESCRIPTOR(bytes.as_mut_ptr().cast());
        if !IsValidSecurityDescriptor(descriptor).as_bool() {
            return Err("Windows rejected the security descriptor as invalid".to_string());
        }

        let mut owner = PSID::default();
        let mut owner_defaulted = BOOL::default();
        GetSecurityDescriptorOwner(descriptor, &mut owner, &mut owner_defaulted)
            .map_err(|error| format!("owner query failed: {error}"))?;
        if owner.0.is_null() {
            return Err("security descriptor has no owner".to_string());
        }

        let mut dacl_present = BOOL::default();
        let mut dacl_defaulted = BOOL::default();
        let mut dacl: *mut ACL = ptr::null_mut();
        GetSecurityDescriptorDacl(
            descriptor,
            &mut dacl_present,
            &mut dacl,
            &mut dacl_defaulted,
        )
        .map_err(|error| format!("DACL query failed: {error}"))?;
        if !dacl_present.as_bool() {
            return Err("security descriptor has no DACL".to_string());
        }
        if dacl.is_null() {
            return Err("security descriptor has a NULL DACL (unrestricted access)".to_string());
        }

        Ok(format!(
            "valid descriptor; owner present; non-NULL DACL with {} ACE(s); ownerDefaulted={}; daclDefaulted={}",
            (*dacl).AceCount,
            owner_defaulted.as_bool(),
            dacl_defaulted.as_bool()
        ))
    }
}

fn collect_diagnostics(prov: &ProvHandle, key: &KeyHandle) -> SecureStorageDiagnostics {
    let no_flags = OBJECT_SECURITY_INFORMATION(0);
    let provider_handle = NCRYPT_HANDLE(prov.0.0);
    let key_handle = NCRYPT_HANDLE(key.0.0);

    let hardware_backing =
        match get_dword_property(provider_handle, NCRYPT_IMPL_TYPE_PROPERTY, no_flags) {
            Ok(flags) if flags & NCRYPT_IMPL_HARDWARE_FLAG != 0 => {
                let isolation = if flags & NCRYPT_IMPL_VIRTUAL_ISOLATION_FLAG != 0 {
                    " Hardware and virtual-isolation flags are both present."
                } else {
                    ""
                };
                SecurityMeasurement::new(
                    "verified",
                    format!("The active key storage provider reports hardware backing.{isolation}"),
                    format!("NCRYPT_IMPL_TYPE_PROPERTY on the provider returned 0x{flags:08x}."),
                )
            }
            Ok(flags) => SecurityMeasurement::new(
                "warning",
                "The active provider did not report hardware backing.",
                format!("NCRYPT_IMPL_TYPE_PROPERTY on the provider returned 0x{flags:08x}."),
            ),
            Err(error) => SecurityMeasurement::new(
                "unavailable",
                "Windows did not return the provider implementation flags.",
                format!("NCRYPT_IMPL_TYPE_PROPERTY failed: {error}"),
            ),
        };

    let export_policy =
        match get_dword_property(key_handle, NCRYPT_EXPORT_POLICY_PROPERTY, no_flags) {
            Ok(0) => SecurityMeasurement::new(
                "verified",
                "Private-key export and archiving are disabled.",
                "NCRYPT_EXPORT_POLICY_PROPERTY returned 0.",
            ),
            Ok(flags) => {
                let mut permissions = Vec::new();
                if flags & NCRYPT_ALLOW_EXPORT_FLAG != 0 {
                    permissions.push("export");
                }
                if flags & NCRYPT_ALLOW_PLAINTEXT_EXPORT_FLAG != 0 {
                    permissions.push("plaintext export");
                }
                if flags & NCRYPT_ALLOW_ARCHIVING_FLAG != 0 {
                    permissions.push("one-time archival export");
                }
                if flags & NCRYPT_ALLOW_PLAINTEXT_ARCHIVING_FLAG != 0 {
                    permissions.push("one-time plaintext archival export");
                }
                SecurityMeasurement::new(
                    "warning",
                    format!("The private-key policy allows {}.", permissions.join(", ")),
                    format!("NCRYPT_EXPORT_POLICY_PROPERTY returned 0x{flags:08x}."),
                )
            }
            Err(error) => SecurityMeasurement::new(
                "unavailable",
                "Windows did not expose the private-key export policy.",
                format!("NCRYPT_EXPORT_POLICY_PROPERTY failed: {error}"),
            ),
        };

    let user_presence = match get_property_bytes(key_handle, NCRYPT_UI_POLICY_PROPERTY, no_flags) {
        Ok(bytes) if bytes.len() >= 8 => {
            let flags = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or_default());
            if flags & NCRYPT_UI_FORCE_HIGH_PROTECTION_FLAG != 0 {
                SecurityMeasurement::new(
                    "enabled",
                    "High-protection user presence is required when the key is used.",
                    format!("NCRYPT_UI_POLICY_PROPERTY returned flags 0x{flags:08x}."),
                )
            } else if flags & NCRYPT_UI_PROTECT_KEY_FLAG != 0 {
                SecurityMeasurement::new(
                    "enabled",
                    "Windows strong-key UI may request user consent when the key is used.",
                    format!("NCRYPT_UI_POLICY_PROPERTY returned flags 0x{flags:08x}."),
                )
            } else {
                SecurityMeasurement::new(
                    "disabled",
                    "No user-presence requirement is configured.",
                    format!("NCRYPT_UI_POLICY_PROPERTY returned flags 0x{flags:08x}."),
                )
            }
        }
        Ok(_) => SecurityMeasurement::new(
            "unavailable",
            "Windows returned an incomplete user-interface policy.",
            "NCRYPT_UI_POLICY_PROPERTY returned fewer than eight bytes.",
        ),
        Err(error) => SecurityMeasurement::new(
            "unavailable",
            "Windows did not return a user-presence policy for this key.",
            format!(
                "NCRYPT_UI_POLICY_PROPERTY was unavailable ({error}). Hikyou does not set this property, but the runtime result cannot be inferred from that fact."
            ),
        ),
    };

    let pcr_binding = match get_dword_property(
        key_handle,
        NCRYPT_PCP_PLATFORM_BINDING_PCRMASK_PROPERTY,
        no_flags,
    ) {
        Ok(0) => SecurityMeasurement::new(
            "disabled",
            "The key is not bound to Platform Configuration Registers.",
            "NCRYPT_PCP_PLATFORM_BINDING_PCRMASK_PROPERTY returned 0.",
        ),
        Ok(mask) => SecurityMeasurement::new(
            "enabled",
            format!("The key is bound to PCR mask 0x{mask:08x}."),
            "NCRYPT_PCP_PLATFORM_BINDING_PCRMASK_PROPERTY returned a non-zero mask.",
        ),
        Err(error) => SecurityMeasurement::new(
            "unavailable",
            "Windows did not return a PCR-binding mask for this key.",
            format!(
                "The PCR-mask property was unavailable ({error}). Hikyou does not set a PCR policy during key creation, but the runtime result cannot be inferred from that fact."
            ),
        ),
    };

    let access_control = match get_dword_property(
        provider_handle,
        NCRYPT_SECURITY_DESCR_SUPPORT_PROPERTY,
        no_flags,
    ) {
        Ok(1) => match get_property_bytes(
            key_handle,
            NCRYPT_SECURITY_DESCR_PROPERTY,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
        ) {
            Ok(mut bytes) if !bytes.is_empty() => match inspect_security_descriptor(&mut bytes) {
                Ok(summary) => SecurityMeasurement::new(
                    "warning",
                    "The key has a valid owner and non-NULL discretionary access-control list; ACE identities and rights are not recorded.",
                    format!(
                        "Windows returned a {}-byte security descriptor: {summary}.",
                        bytes.len()
                    ),
                ),
                Err(error) => SecurityMeasurement::new(
                    "warning",
                    "The persisted key security descriptor failed structural validation.",
                    format!("NCRYPT_SECURITY_DESCR_PROPERTY validation failed: {error}"),
                ),
            },
            Ok(_) => SecurityMeasurement::new(
                "warning",
                "Windows returned an empty key security descriptor.",
                "NCRYPT_SECURITY_DESCR_PROPERTY returned zero bytes.",
            ),
            Err(error) => SecurityMeasurement::new(
                "unavailable",
                "The provider supports key ACLs, but this key's descriptor could not be read.",
                format!("NCRYPT_SECURITY_DESCR_PROPERTY failed: {error}"),
            ),
        },
        Ok(value) => SecurityMeasurement::new(
            "unavailable",
            "The provider did not report support for persisted-key security descriptors.",
            format!("NCRYPT_SECURITY_DESCR_SUPPORT_PROPERTY returned {value}."),
        ),
        Err(error) => SecurityMeasurement::new(
            "unavailable",
            "Windows did not expose key security-descriptor support.",
            format!("NCRYPT_SECURITY_DESCR_SUPPORT_PROPERTY failed: {error}"),
        ),
    };

    SecureStorageDiagnostics {
        backend: "Windows Platform Crypto Provider (RSA-2048-OAEP + AES-256-GCM)".to_string(),
        measured_at_unix_ms: now_unix_ms(),
        provider: Some("Microsoft Platform Crypto Provider".to_string()),
        key_scope: SecurityMeasurement::new(
            "verified",
            "The persisted RSA key is scoped to the current Windows user.",
            "The key was opened in the current-user KSP namespace; NCRYPT_MACHINE_KEY_FLAG is not used.",
        ),
        hardware_backing,
        export_policy,
        user_presence,
        pcr_binding,
        access_control,
    }
}

// ── RSA-OAEP 暗号化 / 復号 ───────────────────────────────────────────────────

/// 32バイト AES 鍵を TPM の RSA-2048 鍵で OAEP(SHA-256) 暗号化する。
fn tpm_rsa_encrypt(key: NCRYPT_KEY_HANDLE, plaintext: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let padding = BCRYPT_OAEP_PADDING_INFO {
            pszAlgId: HASH_SHA256,
            pbLabel: ptr::null_mut(),
            cbLabel: 0,
        };
        let padding_ptr = &padding as *const BCRYPT_OAEP_PADDING_INFO as *const std::ffi::c_void;

        // ① 出力サイズを取得（pbOutput = None）
        // windows-rs 0.58: pbInput=&[u8], pbOutput=Option<&mut [u8]>
        let mut out_size: u32 = 0;
        NCryptEncrypt(
            key,
            Some(plaintext),
            Some(padding_ptr),
            None,
            &mut out_size,
            PAD_OAEP,
        )
        .map_err(|e| format!("TPM RSA encryption size query failed: {}", e))?;

        // ② actualに暗号化
        let mut out = vec![0u8; out_size as usize];
        NCryptEncrypt(
            key,
            Some(plaintext),
            Some(padding_ptr),
            Some(&mut out),
            &mut out_size,
            PAD_OAEP,
        )
        .map_err(|e| format!("TPM RSA encryption failed: {}", e))?;

        out.truncate(out_size as usize);
        Ok(out)
    }
}

/// 暗号化された AES 鍵を TPM の RSA-2048 秘密鍵で OAEP(SHA-256) 復号する。
fn tpm_rsa_decrypt(
    key: NCRYPT_KEY_HANDLE,
    ciphertext: &[u8],
) -> Result<Zeroizing<Vec<u8>>, String> {
    unsafe {
        let padding = BCRYPT_OAEP_PADDING_INFO {
            pszAlgId: HASH_SHA256,
            pbLabel: ptr::null_mut(),
            cbLabel: 0,
        };
        let padding_ptr = &padding as *const BCRYPT_OAEP_PADDING_INFO as *const std::ffi::c_void;

        let mut out_size: u32 = 0;
        NCryptDecrypt(
            key,
            Some(ciphertext),
            Some(padding_ptr),
            None,
            &mut out_size,
            PAD_OAEP,
        )
        .map_err(|e| format!("TPM RSA decryption size query failed: {}", e))?;

        let mut out = Zeroizing::new(vec![0u8; out_size as usize]);
        NCryptDecrypt(
            key,
            Some(ciphertext),
            Some(padding_ptr),
            Some(&mut out),
            &mut out_size,
            PAD_OAEP,
        )
        .map_err(|e| {
            format!(
                "TPM RSA decryption failed (different TPM or corrupt key): {}",
                e
            )
        })?;

        out.truncate(out_size as usize);
        Ok(out)
    }
}

// ── TpmStorage 実装 ───────────────────────────────────────────────────────────

pub struct TpmStorage {
    diagnostics: SecureStorageDiagnostics,
}

impl TpmStorage {
    /// TPM プロバイダが利用可能か確認し、鍵を初期化する。
    pub fn new() -> Result<Self, String> {
        let prov = open_provider()?;
        let key = get_or_create_key(&prov)?;
        let diagnostics = collect_diagnostics(&prov, &key);
        Ok(TpmStorage { diagnostics })
    }
}

impl SecureStorage for TpmStorage {
    /// AES-256-GCM でデータを暗号化し、AES 鍵を TPM RSA 鍵でラップする。
    fn encrypt(&self, _label: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let prov = open_provider()?;
        let key = get_or_create_key(&prov)?;

        // 鍵は最初からゼロ化対象のバッファへ生成する。非ゼロ化の一時コピーは作らない。
        let mut aes_key = Zeroizing::new([0u8; 32]);
        OsRng.fill_bytes(&mut *aes_key);

        let cipher = Aes256Gcm::new_from_slice(&*aes_key)
            .map_err(|_| "AES key initialization failed".to_string())?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|_| "AES-GCM encryption failed".to_string())?;

        // AES 鍵を TPM RSA 鍵でラップ（aes_key は終了後 Zeroizing でゼロ化）
        let wrapped_key = tpm_rsa_encrypt(key.0, &*aes_key)?;

        let mut out = Vec::with_capacity(4 + 4 + wrapped_key.len() + 12 + ciphertext.len());
        out.extend_from_slice(b"HTPM");
        out.extend_from_slice(&(wrapped_key.len() as u32).to_le_bytes());
        out.extend_from_slice(&wrapped_key);
        // as_slice() の代わりに Deref を使用
        out.extend_from_slice(&nonce[..]);
        out.extend_from_slice(&ciphertext);
        Ok(out)
    }

    fn decrypt(&self, _label: &str, data: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        if data.len() < 8 || &data[..4] != b"HTPM" {
            return Err("invalid TPM file format".to_string());
        }
        let wrapped_len = u32::from_le_bytes(
            data[4..8]
                .try_into()
                .map_err(|_| "TPM file is corrupted".to_string())?,
        ) as usize;
        if data.len() < 8 + wrapped_len + 12 {
            return Err("TPM auth file is corrupted".to_string());
        }

        let wrapped_key = &data[8..8 + wrapped_len];
        let nonce_bytes = &data[8 + wrapped_len..8 + wrapped_len + 12];
        let encrypted = &data[8 + wrapped_len + 12..];

        let prov = open_provider()?;
        let key = get_or_create_key(&prov)?;

        // AES 鍵を TPM で復号（Zeroizing で自動ゼロ化）
        let aes_key_bytes = tpm_rsa_decrypt(key.0, wrapped_key)?;
        if aes_key_bytes.len() != 32 {
            return Err("decrypted AES key has an invalid size".to_string());
        }

        let cipher = Aes256Gcm::new_from_slice(&aes_key_bytes)
            .map_err(|_| "AES key initialization failed".to_string())?;
        // &[u8] → [u8; 12] → From (長さは上の bounds check で保証済み)
        let nonce_arr =
            <[u8; 12]>::try_from(nonce_bytes).map_err(|_| "invalid nonce size".to_string())?;
        let nonce = Nonce::from(nonce_arr);
        let plaintext = cipher.decrypt(&nonce, encrypted).map_err(|_| {
            "AES-GCM decryption failed (data was modified or TPM key differs)".to_string()
        })?;

        Ok(Zeroizing::new(plaintext))
    }

    fn backend_name(&self) -> String {
        let implementation = match self.diagnostics.hardware_backing.status {
            "verified" => "verified hardware-backed provider",
            "warning" => "provider not reported as hardware-backed",
            _ => "provider implementation unverified",
        };
        format!(
            "Windows Platform Crypto Provider ({}, RSA-2048-OAEP + AES-256-GCM)",
            implementation
        )
    }

    fn diagnostics(&self) -> SecureStorageDiagnostics {
        let mut diagnostics = self.diagnostics.clone();
        diagnostics.backend = self.backend_name();
        diagnostics
    }
}
