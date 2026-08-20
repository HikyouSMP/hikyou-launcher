//! Windows DPAPI フォールバックバックエンド
//!
//! TPM が利用できない PC 向け。
//! DPAPI はユーザーアカウントに紐づいた暗号化で、
//! 同じユーザーのセッション上でのみ復号できる。
//!
//! windows-rs を使用（旧 winapi crate は廃止）。
//!
//! ファイル形式:
//!   [4B]   マジック b"HDPA"
//!   [残り] DPAPI 暗号化ブロブ（生バイナリ）
//!
//! レガシー形式（旧 winapi 版 auth.bin）:
//!   マジックなし、DPAPI 生ブロブのみ
//!   → storage.rs の migrate_legacy_dpapi() で自動移行

use super::SecureStorage;
use std::ptr;
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData,
};
use windows::core::PCWSTR;
use zeroize::{Zeroize, Zeroizing};

pub struct DpapiStorage;

impl SecureStorage for DpapiStorage {
    fn encrypt(&self, _label: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let raw = dpapi_protect(plaintext)?;
        let mut out = Vec::with_capacity(4 + raw.len());
        out.extend_from_slice(b"HDPA");
        out.extend_from_slice(&raw);
        Ok(out)
    }

    fn decrypt(&self, _label: &str, data: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        if data.len() >= 4 && &data[..4] == b"HDPA" {
            dpapi_unprotect(&data[4..])
        } else {
            // レガシー形式（マジックなし）も復号できるようにする
            dpapi_unprotect(data)
        }
    }

    fn backend_name(&self) -> String {
        "Windows DPAPI (fallback)".to_string()
    }
}

// ── DPAPI ラッパー ────────────────────────────────────────────────────────────

/// CryptProtectData で暗号化して生ブロブを返す。
pub(crate) fn dpapi_protect(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: plaintext.len() as u32,
            pbData: plaintext.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };

        // windows-rs 0.58: returns Result<()>, optional params use Option<*const T>
        // 第2引数は PCWSTR (説明文) — 不要なので null を渡す
        CryptProtectData(
            &input,
            PCWSTR::null(), // 説明文なし
            None,           // 追加エントロピーなし
            None,           // 予約済み
            None,           // UI プロンプトなし
            0,              // 現在のユーザーのみ
            &mut output,
        )
        .map_err(|_| "DPAPI encryption failed (CryptProtectData)".to_string())?;

        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        // Windows が確保したバッファを解放
        free_local(output.pbData as *mut std::ffi::c_void);
        Ok(result)
    }
}

/// CryptUnprotectData で復号して Zeroizing バッファを返す。
pub(crate) fn dpapi_unprotect(blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: blob.len() as u32,
            pbData: blob.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };

        // windows-rs 0.58: returns Result<()>, optional params use Option<*const T>
        CryptUnprotectData(
            &input,
            None, // 説明文出力 (不要)
            None, // 追加エントロピーなし
            None, // 予約済み
            None, // UI プロンプトなし
            0,
            &mut output,
        )
        .map_err(|_| {
            "DPAPI decryption failed (CryptUnprotectData) - please sign in again".to_string()
        })?;

        let result = Zeroizing::new(
            std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec(),
        );
        // DPAPI が LocalAlloc した平文も解放前にゼロ化する。
        std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize).zeroize();
        free_local(output.pbData as *mut std::ffi::c_void);
        Ok(result)
    }
}

/// LocalFree で Windows 確保のメモリを解放する。
///
/// windows-rs 0.58 は LocalFree を Win32_System_Memory に含まないため、
/// extern "system" で直接リンクする。
unsafe fn free_local(ptr: *mut std::ffi::c_void) {
    if !ptr.is_null() {
        unsafe extern "system" {
            fn LocalFree(hMem: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        }
        unsafe { LocalFree(ptr) };
    }
}
