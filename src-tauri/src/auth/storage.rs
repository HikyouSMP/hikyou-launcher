//! 認証情報のセキュアなストレージ管理
//!
//! ┌ バックエンド優先順位 ────────────────────────────────────────────────────┐
//! │ Windows : TPM (NCrypt + AES-256-GCM)  →  DPAPI フォールバック          │
//! │ macOS   : Keychain (keyring crate)                                      │
//! │ Linux   : machine-id 由来 AES-256-GCM + パーミッション 600               │
//! └──────────────────────────────────────────────────────────────────────────┘
//!
//! 保存モデル:
//!   秘密情報の正本はアカウント UUID ごとの `auth_<uuid>.bin` 一つだけにする。
//!   選択中アカウントは、秘密を含まない `active_account.json` の UUID で参照する。
//!   旧 `auth.bin` は読み込み時に正本へ移行してから削除する。

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::crypto;

/// Outer marker for the current secret-file envelope. Its presence means the
/// decrypted payload must carry an authenticated storage context.
const SECRET_FILE_V2_MAGIC: &[u8] = b"HSA2";
const SECRET_CONTEXT_MAGIC: &[u8] = b"hikyou-auth-context-v1\0";

// ── StoredAuth ────────────────────────────────────────────────────────────────

/// Persisted authentication data.
///
/// Token-bearing strings are zeroized when the owning value is dropped.
#[derive(Debug, Serialize, Deserialize, Clone, Zeroize, ZeroizeOnDrop)]
pub struct StoredAuth {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
    pub username: Option<String>,
    pub uuid: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PublicAuth {
    pub expires_at: u64,
    pub username: Option<String>,
    pub uuid: Option<String>,
}

/// Current account selection. This file deliberately contains no credentials.
#[derive(Debug, Serialize, Deserialize)]
struct ActiveAccountRef {
    uuid: String,
}

impl StoredAuth {
    pub fn is_valid(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.expires_at > now + 60
    }

    pub fn to_public(&self) -> PublicAuth {
        PublicAuth {
            expires_at: self.expires_at,
            username: self.username.clone(),
            uuid: self.uuid.clone(),
        }
    }
}

// ── 公開 async API ────────────────────────────────────────────────────────────

pub async fn save_auth(auth: &StoredAuth) -> Result<(), String> {
    let uuid = auth_account_uuid(auth)?;
    let serialized = serialize_auth(auth)?;
    tokio::task::spawn_blocking(move || save_active_account_blocking(&uuid, &serialized))
        .await
        .map_err(|e| format!("save task failed: {}", e))?
}

pub async fn load_auth() -> Result<StoredAuth, String> {
    tokio::task::spawn_blocking(load_active_account_blocking)
        .await
        .map_err(|e| format!("load task failed: {}", e))?
}

pub async fn delete_auth() -> Result<(), String> {
    tokio::task::spawn_blocking(delete_active_account_blocking)
        .await
        .map_err(|e| format!("delete task failed: {}", e))?
}

// ── アカウント別 async API ────────────────────────────────────────────────────

pub async fn load_account_auth(uuid: String) -> Result<StoredAuth, String> {
    tokio::task::spawn_blocking(move || {
        let normalized = normalize_account_uuid(&uuid)?;
        let path = per_account_path(&normalized)?;
        let legacy_path = app_dir()?.join(format!("auth_{}.bin", uuid.trim()));
        if path.exists() {
            return load_blocking(&normalized, &path);
        }
        if legacy_path != path && legacy_path.exists() {
            let auth = load_blocking(&normalized, &legacy_path)?;
            let serialized = serialize_auth(&auth)?;
            save_account_blocking(&normalized, &serialized)?;
            if let Err(error) = fs::remove_file(&legacy_path) {
                log::warn!(
                    "[storage] Legacy account credential cleanup failed: {}",
                    error
                );
            }
            return Ok(auth);
        }
        load_blocking(&normalized, &path)
    })
    .await
    .map_err(|e| format!("load task failed: {}", e))?
}

pub async fn delete_account_auth(uuid: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let normalized = normalize_account_uuid(&uuid)?;
        let path = per_account_path(&normalized)?;
        if path.exists() {
            fs::remove_file(&path).map_err(|e| format!("delete failed: {}", e))?;
        }
        let legacy_path = app_dir()?.join(format!("auth_{}.bin", uuid.trim()));
        if legacy_path != path && legacy_path.exists() {
            fs::remove_file(&legacy_path).map_err(|e| format!("delete failed: {}", e))?;
        }
        clear_active_account_if_matches(&normalized)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("delete task failed: {}", e))?
}

// ── ブロッキング内部実装 ───────────────────────────────────────────────────────

/// Saves an authenticated, account-scoped secret envelope.
///
/// The context is encrypted together with the payload. Consequently, a current
/// credential file copied to a different account UUID cannot be accepted after
/// decryption. The outer marker lets us distinguish the old unscoped format
/// during a targeted, one-time migration.
fn save_blocking(label: &str, path: &Path, plaintext: &[u8]) -> Result<(), String> {
    let contextual_plaintext = encode_secret_context(label, plaintext)?;
    let encrypted = crypto::backend().encrypt(label, &contextual_plaintext)?;
    let mut envelope = Vec::with_capacity(SECRET_FILE_V2_MAGIC.len() + encrypted.len());
    envelope.extend_from_slice(SECRET_FILE_V2_MAGIC);
    envelope.extend_from_slice(&encrypted);
    write_file_atomic(path, &envelope, true)?;
    log::info!(
        "[storage] Auth data encrypted and saved ({}) via {}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        crypto::backend().backend_name()
    );
    Ok(())
}

fn save_account_blocking(uuid: &str, serialized: &[u8]) -> Result<(), String> {
    let path = per_account_path(uuid)?;
    save_blocking(uuid, &path, serialized)
}

fn save_active_account_blocking(uuid: &str, serialized: &[u8]) -> Result<(), String> {
    save_account_blocking(uuid, serialized)?;
    write_active_account(uuid)
}

fn load_active_account_blocking() -> Result<StoredAuth, String> {
    let active_path = active_account_path()?;
    if active_path.exists() {
        let active = read_active_account(&active_path)?;
        return load_blocking(&active.uuid, &per_account_path(&active.uuid)?);
    }

    migrate_legacy_default_auth()
}

/// Migrate the legacy default credential only when no active-account reference
/// exists. This is a compatibility path, not a repeated startup scan.
fn migrate_legacy_default_auth() -> Result<StoredAuth, String> {
    let legacy_path = auth_bin_path()?;
    let auth = load_blocking("default", &legacy_path)?;
    let uuid = auth_account_uuid(&auth)?;
    let serialized = serialize_auth(&auth)?;
    save_active_account_blocking(&uuid, &serialized)?;
    if let Err(error) = fs::remove_file(&legacy_path) {
        log::warn!(
            "[storage] Legacy default credential cleanup failed: {}",
            error
        );
    }
    log::info!("[storage] Migrated legacy default authentication to account credential");
    Ok(auth)
}

fn delete_active_account_blocking() -> Result<(), String> {
    let active_path = active_account_path()?;
    if !active_path.exists() {
        return Ok(());
    }

    let active = read_active_account(&active_path)?;
    let credential_path = per_account_path(&active.uuid)?;
    if credential_path.exists() {
        fs::remove_file(&credential_path).map_err(|e| format!("delete failed: {}", e))?;
    }
    fs::remove_file(&active_path).map_err(|e| format!("active account cleanup failed: {}", e))?;
    log::info!("[storage] Active authentication data deleted");
    Ok(())
}

fn clear_active_account_if_matches(uuid: &str) -> Result<(), String> {
    let active_path = active_account_path()?;
    if !active_path.exists() {
        return Ok(());
    }
    let active = read_active_account(&active_path)?;
    if active.uuid == uuid {
        fs::remove_file(active_path)
            .map_err(|e| format!("active account cleanup failed: {}", e))?;
    }
    Ok(())
}

fn load_blocking(label: &str, path: &PathBuf) -> Result<StoredAuth, String> {
    if !path.exists() {
        return Err("No saved authentication data. Please sign in again.".to_string());
    }

    let data = fs::read(path).map_err(|e| format!("read failed: {}", e))?;
    let (ciphertext, requires_context) = match data.strip_prefix(SECRET_FILE_V2_MAGIC) {
        Some(ciphertext) => (ciphertext, true),
        None => (&data[..], false),
    };

    let plain = crypto::backend().decrypt(label, ciphertext).map_err(|e| {
        format!(
            "Failed to decrypt authentication data. Please sign in again.\nDetails: {}",
            e
        )
    })?;
    let plaintext = decode_secret_context(label, &plain, requires_context)?;
    let auth = parse_auth(plaintext)?;
    drop(plain);

    // Only the exact credential being read is migrated. We do not scan other
    // accounts or re-encrypt healthy current-format credentials during startup.
    if !requires_context {
        let serialized = serialize_auth(&auth)?;
        if let Err(error) = save_blocking(label, path, &serialized) {
            log::warn!(
                "[storage] Could not upgrade legacy credential envelope: {}",
                error
            );
        } else {
            log::info!("[storage] Upgraded legacy credential envelope");
        }
    }
    Ok(auth)
}

// ── ヘルパー ──────────────────────────────────────────────────────────────────

fn parse_auth(plain: &[u8]) -> Result<StoredAuth, String> {
    // 復号バッファから直接デシリアライズする。完全な JSON String の中間コピーは作らない。
    serde_json::from_slice(plain)
        .map_err(|_| "Authentication data is corrupted. Please sign in again.".to_string())
}

fn serialize_auth(auth: &StoredAuth) -> Result<Zeroizing<Vec<u8>>, String> {
    serde_json::to_vec(auth)
        .map(Zeroizing::new)
        .map_err(|e| format!("serialization failed: {}", e))
}

fn encode_secret_context(label: &str, plaintext: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
    let label = secret_context_label(label)?;
    let label_len =
        u16::try_from(label.len()).map_err(|_| "secret storage context is too long".to_string())?;
    let mut encoded = Zeroizing::new(Vec::with_capacity(
        SECRET_CONTEXT_MAGIC.len() + std::mem::size_of::<u16>() + label.len() + plaintext.len(),
    ));
    encoded.extend_from_slice(SECRET_CONTEXT_MAGIC);
    encoded.extend_from_slice(&label_len.to_le_bytes());
    encoded.extend_from_slice(label.as_bytes());
    encoded.extend_from_slice(plaintext);
    Ok(encoded)
}

fn decode_secret_context<'a>(
    label: &str,
    plaintext: &'a [u8],
    requires_context: bool,
) -> Result<&'a [u8], String> {
    if !requires_context {
        return Ok(plaintext);
    }

    let remaining = plaintext
        .strip_prefix(SECRET_CONTEXT_MAGIC)
        .ok_or("authentication storage context is missing or invalid")?;
    let raw_length: [u8; 2] = remaining
        .get(..2)
        .ok_or("authentication storage context is truncated")?
        .try_into()
        .map_err(|_| "authentication storage context is invalid")?;
    let label_len = u16::from_le_bytes(raw_length) as usize;
    let remaining = &remaining[2..];
    let stored_label = remaining
        .get(..label_len)
        .ok_or("authentication storage context is truncated")?;
    let expected = secret_context_label(label)?;
    if stored_label != expected.as_bytes() {
        return Err("authentication storage context does not match this account".to_string());
    }
    Ok(&remaining[label_len..])
}

fn secret_context_label(label: &str) -> Result<String, String> {
    if label == "default" {
        return Ok("hikyou-auth:legacy-default".to_string());
    }
    Ok(format!(
        "hikyou-auth:account:{}",
        normalize_account_uuid(label)?
    ))
}

fn auth_account_uuid(auth: &StoredAuth) -> Result<String, String> {
    credential_uuid(auth.uuid.as_deref())
}

fn credential_uuid(uuid: Option<&str>) -> Result<String, String> {
    normalize_account_uuid(
        uuid.ok_or("Authentication data does not include an account UUID. Please sign in again.")?,
    )
}

fn read_active_account(path: &PathBuf) -> Result<ActiveAccountRef, String> {
    let bytes = fs::read(path).map_err(|e| format!("active account read failed: {}", e))?;
    let active: ActiveAccountRef = serde_json::from_slice(&bytes)
        .map_err(|_| "Active account state is corrupted. Please sign in again.".to_string())?;
    Ok(ActiveAccountRef {
        uuid: normalize_account_uuid(&active.uuid)?,
    })
}

fn write_active_account(uuid: &str) -> Result<(), String> {
    let path = active_account_path()?;
    write_active_account_to(&path, uuid)
}

fn write_active_account_to(path: &std::path::Path, uuid: &str) -> Result<(), String> {
    let content = serde_json::to_vec(&ActiveAccountRef {
        uuid: normalize_account_uuid(uuid)?,
    })
    .map_err(|e| format!("active account serialization failed: {}", e))?;
    write_file_atomic(path, &content, false)
}

/// Writes a complete replacement in the destination directory, flushes it, and
/// atomically replaces the previous file where the platform supports it.
fn write_file_atomic(path: &Path, contents: &[u8], private: bool) -> Result<(), String> {
    #[cfg(not(target_os = "linux"))]
    let _ = private;
    let parent = path
        .parent()
        .ok_or("storage path does not have a parent directory")?;
    fs::create_dir_all(parent).map_err(|e| format!("storage directory creation failed: {}", e))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("storage path has an invalid file name")?;
    let temp_path = parent.join(format!(".{}.{}.tmp", name, Uuid::new_v4()));

    let write_result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|e| format!("temporary storage file creation failed: {}", e))?;
        file.write_all(contents)
            .map_err(|e| format!("temporary storage write failed: {}", e))?;
        file.sync_all()
            .map_err(|e| format!("temporary storage flush failed: {}", e))?;
        drop(file);

        #[cfg(target_os = "linux")]
        if private {
            set_permissions_600(&temp_path)?;
        }

        replace_file_atomically(&temp_path, path)
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(target_os = "windows")]
fn replace_file_atomically(temp_path: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, REPLACE_FILE_FLAGS,
        ReplaceFileW,
    };
    use windows::core::PCWSTR;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    let source = wide(temp_path);
    let target = wide(destination);
    unsafe {
        if destination.exists() {
            ReplaceFileW(
                PCWSTR(target.as_ptr()),
                PCWSTR(source.as_ptr()),
                PCWSTR::null(),
                REPLACE_FILE_FLAGS(0),
                None,
                None,
            )
            .or_else(|_| {
                MoveFileExW(
                    PCWSTR(source.as_ptr()),
                    PCWSTR(target.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            })
            .map_err(|e| format!("atomic storage replacement failed: {}", e))
        } else {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(target.as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )
            .map_err(|e| format!("atomic storage commit failed: {}", e))
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn replace_file_atomically(temp_path: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(temp_path, destination)
        .map_err(|e| format!("atomic storage replacement failed: {}", e))
}

// ── パス ────────────────────────────────────────────────────────────────────

fn app_dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or("Could not resolve the config directory")?
        .join("hikyou-launcher");
    fs::create_dir_all(&dir).map_err(|e| format!("directory creation failed: {}", e))?;
    Ok(dir)
}

fn auth_bin_path() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("auth.bin"))
}

fn active_account_path() -> Result<PathBuf, String> {
    Ok(app_dir()?.join("active_account.json"))
}

fn per_account_path(uuid: &str) -> Result<PathBuf, String> {
    let normalized = normalize_account_uuid(uuid)?;
    Ok(app_dir()?.join(format!("auth_{}.bin", normalized)))
}

fn normalize_account_uuid(uuid: &str) -> Result<String, String> {
    let compact = uuid.trim().replace('-', "");
    if compact.len() != 32 || !compact.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid account UUID.".to_string());
    }
    let parsed = Uuid::parse_str(&compact).map_err(|_| "Invalid account UUID.".to_string())?;
    Ok(parsed.simple().to_string())
}

#[cfg(target_os = "linux")]
fn set_permissions_600(path: &PathBuf) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("permission update failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::{
        SECRET_CONTEXT_MAGIC, decode_secret_context, encode_secret_context, read_active_account,
        write_active_account_to, write_file_atomic,
    };

    #[test]
    fn active_account_reference_replaces_previous_selection() {
        let directory =
            std::env::temp_dir().join(format!("hikyou-auth-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("active_account.json");
        let first = "00000000-0000-0000-0000-000000000001";
        let second = "00000000-0000-0000-0000-000000000002";

        write_active_account_to(&path, first).unwrap();
        write_active_account_to(&path, second).unwrap();

        assert_eq!(
            read_active_account(&path).unwrap().uuid,
            "00000000000000000000000000000002"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn contextual_secret_rejects_a_different_account() {
        let account = "00000000-0000-0000-0000-000000000001";
        let other_account = "00000000-0000-0000-0000-000000000002";
        let plaintext = b"credential payload";
        let encoded = encode_secret_context(account, plaintext).unwrap();

        assert_eq!(
            decode_secret_context(account, &encoded, true).unwrap(),
            plaintext
        );
        assert!(decode_secret_context(other_account, &encoded, true).is_err());
        assert!(encoded.starts_with(SECRET_CONTEXT_MAGIC));
    }

    #[test]
    fn atomic_write_replaces_an_existing_file() {
        let directory =
            std::env::temp_dir().join(format!("hikyou-auth-atomic-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("auth.bin");

        write_file_atomic(&path, b"first", true).unwrap();
        write_file_atomic(&path, b"second", true).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
