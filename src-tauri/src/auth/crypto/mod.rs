//! セキュアストレージのプラットフォーム抽象レイヤー
//!
//! バックエンドの優先順位:
//!   Windows : TPM (MS_PLATFORM_CRYPTO_PROVIDER + AES-256-GCM ハイブリッド)
//!             → DPAPI フォールバック（TPM 非搭載 PC 向け）
//!   macOS   : Secure Enclave (T2/Apple Silicon ハードウェアバックド EC 鍵 + ECIES)
//!             → Keychain フォールバック（SE 非搭載の旧 Intel Mac 向け）
//!   Linux   : machine-id 由来の AES-256-GCM（permission 600 併用）
//!
//! `SecureStorage` トレイトで実装を隠蔽しているため、
//! 将来のプラットフォーム対応時も呼び出し側コードを変えずに乗り換えられる。

use std::sync::OnceLock;
use zeroize::Zeroizing;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
pub mod dpapi;
#[cfg(target_os = "windows")]
pub mod tpm;

// ── トレイト ─────────────────────────────────────────────────────────────────

/// プラットフォーム固有の暗号化バックエンド
///
/// - `label` : エントリを識別する文字列。
///     * Windows / Linux : 無視（ファイルパスで区別）
///     * macOS Keychain  : Keychain エントリ名として使用
///       呼び出し規約: デフォルト認証 = `"default"`、アカウント別 = UUID 文字列
///
/// - `decrypt` の戻り値 `Zeroizing<Vec<u8>>` は Drop 時に自動ゼロ化される。
///   呼び出し元は必要以上に長くメモリに保持しないこと。
pub trait SecureStorage: Send + Sync {
    fn encrypt(&self, label: &str, plaintext: &[u8]) -> Result<Vec<u8>, String>;
    fn decrypt(&self, label: &str, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, String>;
    fn backend_name(&self) -> String;
}

// ── シングルトン ──────────────────────────────────────────────────────────────

static BACKEND: OnceLock<Box<dyn SecureStorage>> = OnceLock::new();

/// アプリ全体で使用するバックエンドを返す（初回呼び出し時に初期化）。
pub fn backend() -> &'static dyn SecureStorage {
    BACKEND.get_or_init(create_backend).as_ref()
}

fn create_backend() -> Box<dyn SecureStorage> {
    #[cfg(target_os = "windows")]
    {
        match tpm::TpmStorage::new() {
            Ok(b) => {
                log::info!("[SecureStorage] Backend: {}", b.backend_name());
                return Box::new(b);
            }
            Err(e) => {
                log::warn!(
                    "[SecureStorage] TPM init failed → falling back to DPAPI: {}",
                    e
                );
            }
        }
        log::info!(
            "[SecureStorage] Backend: {}",
            dpapi::DpapiStorage.backend_name()
        );
        return Box::new(dpapi::DpapiStorage);
    }

    #[cfg(target_os = "macos")]
    {
        match macos::SecureEnclaveStorage::new() {
            Ok(b) => {
                log::info!("[SecureStorage] Backend: {}", b.backend_name());
                return Box::new(b);
            }
            Err(e) => {
                // SE は keychain-access-groups entitlement + Developer ID 署名が必要。
                // ad-hoc 署名では -34018 になるため AES-256-GCM にフォールバック。
                if e.contains("-34018") || e.contains("errSecMissingEntitlement") {
                    log::info!(
                        "[SecureStorage] Secure Enclave entitlement is unavailable; using macOS AES-256-GCM fallback"
                    );
                } else {
                    log::warn!(
                        "[SecureStorage] Secure Enclave init failed; using AES-256-GCM fallback: {}",
                        e
                    );
                }
            }
        }
        log::info!("[SecureStorage] Backend: macOS AES-256-GCM (machine-bound)");
        return Box::new(macos::MacOsAesStorage);
    }

    #[cfg(target_os = "linux")]
    {
        log::info!("[SecureStorage] Backend: Linux AES-256-GCM (machine-id bound)");
        return Box::new(linux::LinuxAesStorage);
    }

    #[allow(unreachable_code)]
    {
        // ビルドターゲットが上記以外の場合（Android/iOS 等）
        // actualにはここには到達しないが、型推論のためにコンパイルされる
        panic!("unsupported platform")
    }
}

#[cfg(test)]
mod tests {
    use zeroize::ZeroizeOnDrop;

    #[test]
    fn aes_gcm_internal_key_state_zeroizes_on_drop() {
        fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}

        // The direct feature declaration in Cargo.toml makes AES round-key
        // clearing a compile-time contract instead of an assumption about a
        // transitive default. GHASH/Polyval use their own Drop implementations
        // in the fixed dependency versions and are locked by Cargo.lock.
        assert_zeroize_on_drop::<aes::Aes256>();
    }
}
