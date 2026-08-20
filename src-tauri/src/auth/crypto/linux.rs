use super::{SecureStorage, Zeroizing};

// ── Linux AES-256-GCM (machine-id bound) ─────────────────────────────────────

#[cfg(target_os = "linux")]
mod linux_aes {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use hkdf::Hkdf;
    use sha2::Sha256;
    use zeroize::Zeroizing;

    fn machine_id() -> Result<Zeroizing<String>, String> {
        for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(s) = std::fs::read_to_string(path) {
                let id = s.trim();
                if !id.is_empty() {
                    return Ok(Zeroizing::new(id.to_string()));
                }
            }
        }
        Err("Linux machine-id was not found".to_string())
    }

    fn derive_key() -> Result<Zeroizing<[u8; 32]>, String> {
        let id = machine_id()?;
        let hk = Hkdf::<Sha256>::new(Some(b"hikyou-launcher-linux-v1"), id.as_bytes());
        let mut key = Zeroizing::new([0u8; 32]);
        hk.expand(b"aes-256-gcm-key", &mut *key)
            .map_err(|e| format!("HKDF expand failed: {}", e))?;
        Ok(key)
    }

    pub fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, String> {
        use rand::RngCore;
        let key_bytes = derive_key()?;
        let cipher = Aes256Gcm::new_from_slice(&*key_bytes)
            .map_err(|_| "AES key initialization failed".to_string())?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);
        let ct = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| format!("AES-GCM encryption failed: {}", e))?;
        let mut out = Vec::with_capacity(12 + ct.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub fn decrypt(data: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        if data.len() < 13 {
            return Err("ciphertext is too short".to_string());
        }
        let (nonce_bytes, ct) = data.split_at(12);
        let key_bytes = derive_key()?;
        let cipher = Aes256Gcm::new_from_slice(&*key_bytes)
            .map_err(|_| "AES key initialization failed".to_string())?;
        let nonce_arr =
            <[u8; 12]>::try_from(nonce_bytes).map_err(|_| "invalid nonce size".to_string())?;
        let nonce = Nonce::from(nonce_arr);
        cipher.decrypt(&nonce, ct).map(Zeroizing::new).map_err(|_| {
            "AES-GCM decryption failed (corrupt data or machine-id mismatch)".to_string()
        })
    }
}

#[cfg(target_os = "linux")]
pub(super) struct LinuxAesStorage;

#[cfg(target_os = "linux")]
impl SecureStorage for LinuxAesStorage {
    fn encrypt(&self, _label: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        linux_aes::encrypt(plaintext)
    }

    fn decrypt(&self, _label: &str, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        linux_aes::decrypt(ciphertext)
    }

    fn backend_name(&self) -> String {
        "Linux AES-256-GCM (machine-id bound)".to_string()
    }
}
