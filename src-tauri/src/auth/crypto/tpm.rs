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

use super::SecureStorage;
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use rand::RngCore;
use std::ptr;
use windows::Win32::Security::Cryptography::{
    BCRYPT_OAEP_PADDING_INFO, CERT_KEY_SPEC, NCRYPT_FLAGS, NCRYPT_HANDLE,
    NCRYPT_IMPL_HARDWARE_FLAG, NCRYPT_IMPL_TYPE_PROPERTY, NCRYPT_KEY_HANDLE, NCRYPT_PROV_HANDLE,
    NCryptCreatePersistedKey, NCryptDecrypt, NCryptEncrypt, NCryptFinalizeKey, NCryptFreeObject,
    NCryptGetProperty, NCryptOpenKey, NCryptOpenStorageProvider, NCryptSetProperty,
};
use windows::Win32::Security::OBJECT_SECURITY_INFORMATION;
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

/// Reads the CNG implementation flag from the actual persisted key.  The
/// provider name alone is not evidence that a key is hardware-backed.
fn key_implementation_type(key: &KeyHandle) -> Result<u32, String> {
    unsafe {
        let mut bytes = [0u8; std::mem::size_of::<u32>()];
        let mut returned = 0u32;
        NCryptGetProperty(
            NCRYPT_HANDLE(key.0.0),
            NCRYPT_IMPL_TYPE_PROPERTY,
            Some(&mut bytes),
            &mut returned,
            OBJECT_SECURITY_INFORMATION(0),
        )
        .map_err(|error| format!("TPM implementation property read failed: {}", error))?;

        if returned != bytes.len() as u32 {
            return Err("TPM implementation property has an invalid size".to_string());
        }
        Ok(u32::from_le_bytes(bytes))
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
    implementation_type: Option<u32>,
}

impl TpmStorage {
    /// TPM プロバイダが利用可能か確認し、鍵を初期化する。
    pub fn new() -> Result<Self, String> {
        let prov = open_provider()?;
        let key = get_or_create_key(&prov)?;
        let implementation_type = match key_implementation_type(&key) {
            Ok(value) => Some(value),
            Err(error) => {
                log::warn!(
                    "[SecureStorage] TPM key implementation could not be verified: {}",
                    error
                );
                None
            }
        };
        Ok(TpmStorage {
            implementation_type,
        })
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
        let implementation = match self.implementation_type {
            Some(flags) if flags & NCRYPT_IMPL_HARDWARE_FLAG != 0 => "verified hardware-backed key",
            Some(_) => "provider key not reported as hardware-backed",
            None => "key implementation unverified",
        };
        format!(
            "Windows Platform Crypto Provider ({}, RSA-2048-OAEP + AES-256-GCM)",
            implementation
        )
    }
}
