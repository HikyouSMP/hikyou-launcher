use super::{SecureStorage, Zeroizing};

// ── macOS Secure Enclave ──────────────────────────────────────────────────────
//
// T2 チップ（Intel 2018+）または Apple Silicon で利用可能なハードウェアバックドストレージ。
// 秘密鍵はチップ外に出ることがなく、ECIES (EC + AES-GCM) で認証データを暗号化する。
//
// ファイル形式: "se:<base64(ECIES暗号化バイト)>"
// 後方互換: "keychain:<label>" は旧 Keychain エントリへのフォールバックとして機能する。

#[cfg(target_os = "macos")]
mod se_impl {
    use super::Zeroizing;
    use base64::Engine;
    use std::ptr;

    // ── 型エイリアス ──────────────────────────────────────────────────────────
    type CFTypeRef = *const std::ffi::c_void;
    type CFStringRef = CFTypeRef;
    type CFDataRef = CFTypeRef;
    type CFDictionaryRef = CFTypeRef;
    type CFMutableDictionaryRef = CFTypeRef;
    type CFErrorRef = CFTypeRef;
    type CFNumberRef = CFTypeRef;
    type CFBooleanRef = CFTypeRef;
    type SecKeyRef = CFTypeRef;
    type SecAccessControlRef = CFTypeRef;
    type CFIndex = isize;
    type OSStatus = i32;

    // ── 外部シンボル宣言 ──────────────────────────────────────────────────────
    #[link(name = "Security", kind = "framework")]
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        // CFType
        fn CFRelease(cf: CFTypeRef);

        // CFBoolean
        static kCFBooleanTrue: CFBooleanRef;

        // CFString
        fn CFStringCreateWithBytes(
            alloc: CFTypeRef,
            bytes: *const u8,
            num_bytes: CFIndex,
            encoding: u32,
            is_external: bool,
        ) -> CFStringRef;

        // CFNumber
        fn CFNumberCreate(
            alloc: CFTypeRef,
            the_type: i32,
            value_ptr: *const std::ffi::c_void,
        ) -> CFNumberRef;

        // CFData
        fn CFDataCreate(alloc: CFTypeRef, bytes: *const u8, length: CFIndex) -> CFDataRef;
        fn CFDataGetBytePtr(the_data: CFDataRef) -> *const u8;
        fn CFDataGetLength(the_data: CFDataRef) -> CFIndex;

        // CFDictionary
        fn CFDictionaryCreateMutable(
            alloc: CFTypeRef,
            capacity: CFIndex,
            key_cbs: *const std::ffi::c_void,
            val_cbs: *const std::ffi::c_void,
        ) -> CFMutableDictionaryRef;
        fn CFDictionarySetValue(the_dict: CFMutableDictionaryRef, key: CFTypeRef, value: CFTypeRef);

        // CFDictionary callbacks (CoreFoundation)
        static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
        static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;

        // Keychain / Security item 定数
        static kSecClass: CFStringRef;
        static kSecClassKey: CFStringRef;
        static kSecAttrKeyType: CFStringRef;
        static kSecAttrKeyTypeEC: CFStringRef;
        static kSecAttrKeySizeInBits: CFStringRef;
        static kSecAttrTokenID: CFStringRef;
        static kSecAttrTokenIDSecureEnclave: CFStringRef;
        static kSecPrivateKeyAttrs: CFStringRef;
        static kSecAttrIsPermanent: CFStringRef;
        static kSecAttrApplicationTag: CFStringRef;
        static kSecAttrAccessGroup: CFStringRef;
        static kSecAttrKeyClass: CFStringRef;
        static kSecAttrKeyClassPrivate: CFStringRef;
        static kSecReturnRef: CFStringRef;
        static kSecMatchLimit: CFStringRef;
        static kSecMatchLimitOne: CFStringRef;

        // ECIES アルゴリズム
        static kSecKeyAlgorithmECIESEncryptionCofactorX963SHA256AESGCM: CFStringRef;

        // SecKey 関数
        fn SecKeyCreateRandomKey(parameters: CFDictionaryRef, error: *mut CFErrorRef) -> SecKeyRef;
        fn SecKeyCopyPublicKey(key: SecKeyRef) -> SecKeyRef;
        fn SecKeyCreateEncryptedData(
            key: SecKeyRef,
            algorithm: CFStringRef,
            plaintext: CFDataRef,
            error: *mut CFErrorRef,
        ) -> CFDataRef;
        fn SecKeyCreateDecryptedData(
            key: SecKeyRef,
            algorithm: CFStringRef,
            ciphertext: CFDataRef,
            error: *mut CFErrorRef,
        ) -> CFDataRef;

        // SecItem 関数
        fn SecItemCopyMatching(query: CFDictionaryRef, result: *mut CFTypeRef) -> OSStatus;

        // SecAccessControl — Level 1 (ハードウェア分離、生体認証なし)
        static kSecAttrAccessibleWhenUnlockedThisDeviceOnly: CFTypeRef;
        static kSecAttrAccessControl: CFStringRef;
        fn SecAccessControlCreateWithFlags(
            alloc: CFTypeRef,
            protection: CFTypeRef,
            flags: u64,
            error: *mut CFErrorRef,
        ) -> SecAccessControlRef;

        // CFError → 文字列
        fn CFErrorCopyDescription(err: CFErrorRef) -> CFStringRef;
        fn CFStringGetCString(
            the_string: CFStringRef,
            buffer: *mut std::os::raw::c_char,
            buffer_size: CFIndex,
            encoding: u32,
        ) -> bool;
    }

    // ── 定数 ─────────────────────────────────────────────────────────────────
    const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const CF_NUMBER_SINT32_TYPE: i32 = 3;
    /// Level 1: ハードウェア分離のみ（生体認証・パスコード不要）
    const SEC_ACCESS_CONTROL_PRIVATE_KEY_USAGE: u64 = 1 << 30;

    /// Keychain に保存する SE 鍵の識別タグ（アプリ固有）
    const SE_TAG: &[u8] = b"net.hikyou.launcher.se-key.v1";
    const KEYCHAIN_ACCESS_GROUP: &str = "io.github.hikyou-smp.hikyou-launcher";

    // ── RAII ラッパー ─────────────────────────────────────────────────────────

    /// Drop 時に CFRelease を呼ぶ自動解放ラッパー
    struct CfOwned(CFTypeRef);
    impl Drop for CfOwned {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CFRelease(self.0) };
            }
        }
    }

    // ── ヘルパー ──────────────────────────────────────────────────────────────

    fn cf_str(s: &str) -> CfOwned {
        unsafe {
            CfOwned(CFStringCreateWithBytes(
                ptr::null(),
                s.as_ptr(),
                s.len() as CFIndex,
                CF_STRING_ENCODING_UTF8,
                false,
            ))
        }
    }

    fn cf_num_i32(n: i32) -> CfOwned {
        // CFNumberCreate はポインタが指す値をコピーするため、ローカル変数のアドレスで安全。
        unsafe {
            CfOwned(CFNumberCreate(
                ptr::null(),
                CF_NUMBER_SINT32_TYPE,
                &n as *const i32 as *const _,
            ))
        }
    }

    fn cf_data(bytes: &[u8]) -> CfOwned {
        unsafe {
            CfOwned(CFDataCreate(
                ptr::null(),
                bytes.as_ptr(),
                bytes.len() as CFIndex,
            ))
        }
    }

    fn new_dict() -> CfOwned {
        unsafe {
            CfOwned(CFDictionaryCreateMutable(
                ptr::null(),
                0,
                &kCFTypeDictionaryKeyCallBacks as *const _ as *const _,
                &kCFTypeDictionaryValueCallBacks as *const _ as *const _,
            ))
        }
    }

    // ── ヘルパー続き ─────────────────────────────────────────────────────────

    /// CFError の説明文を Rust String に変換する。
    unsafe fn cf_error_to_string(err: CFErrorRef) -> String {
        if err.is_null() {
            return "(null error)".to_string();
        }
        let desc = unsafe { CFErrorCopyDescription(err) };
        if desc.is_null() {
            return "(description unavailable)".to_string();
        }
        let _desc_guard = CfOwned(desc);
        let mut buf = vec![0i8; 512];
        let ok = unsafe {
            CFStringGetCString(
                desc,
                buf.as_mut_ptr(),
                buf.len() as CFIndex,
                CF_STRING_ENCODING_UTF8,
            )
        };
        if ok {
            let cstr = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) };
            cstr.to_string_lossy().into_owned()
        } else {
            "(CFStringGetCString failed)".to_string()
        }
    }

    // ── SE 鍵の取得/生成 ─────────────────────────────────────────────────────

    /// Keychain から SE 秘密鍵を取得する（存在しない場合は None）。
    unsafe fn load_se_key() -> Option<CfOwned> {
        unsafe {
            let tag = cf_data(SE_TAG);
            let ag = cf_str(KEYCHAIN_ACCESS_GROUP);
            let query = new_dict();
            CFDictionarySetValue(query.0, kSecClass, kSecClassKey);
            CFDictionarySetValue(query.0, kSecAttrKeyClass, kSecAttrKeyClassPrivate);
            CFDictionarySetValue(query.0, kSecAttrApplicationTag, tag.0);
            CFDictionarySetValue(query.0, kSecAttrTokenID, kSecAttrTokenIDSecureEnclave);
            CFDictionarySetValue(query.0, kSecAttrAccessGroup, ag.0);
            CFDictionarySetValue(query.0, kSecReturnRef, kCFBooleanTrue);
            CFDictionarySetValue(query.0, kSecMatchLimit, kSecMatchLimitOne);

            let mut result: CFTypeRef = ptr::null();
            let status = SecItemCopyMatching(query.0, &mut result);
            if status == 0 && !result.is_null() {
                Some(CfOwned(result))
            } else {
                None
            }
        }
    }

    /// Secure Enclave に新しい EC 鍵ペアを生成し Keychain に永続保存する。
    ///
    /// Level 1 (`kSecAccessControlPrivateKeyUsage`): ハードウェア分離のみ。
    /// 生体認証・パスコードは不要で、SE チップ外に秘密鍵が出ない保証だけを持つ。
    unsafe fn generate_se_key() -> Result<CfOwned, String> {
        unsafe {
            let tag = cf_data(SE_TAG);

            // Level 1 アクセス制御: ハードウェア分離のみ（生体認証不要）
            let mut ac_error: CFErrorRef = ptr::null();
            let access_control = SecAccessControlCreateWithFlags(
                ptr::null(),
                kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
                SEC_ACCESS_CONTROL_PRIVATE_KEY_USAGE,
                &mut ac_error,
            );
            // RAII で ac_error を管理（成功・失敗どちらのパスでも確実に解放）
            let _ac_error_guard = if !ac_error.is_null() {
                Some(CfOwned(ac_error))
            } else {
                None
            };
            if access_control.is_null() {
                return Err(format!(
                    "SecAccessControl creation failed: {}",
                    cf_error_to_string(ac_error)
                ));
            }
            let _ac_guard = CfOwned(access_control);

            // 秘密鍵の属性: 永続保存 + アクセスグループ + アプリタグ + Level 1 アクセス制御
            // kSecAttrAccessGroup: Data Protection Keychain のどのグループに保存するかを指定。
            // Entitlements の keychain-access-groups と一致させる必要がある。
            let ag = cf_str(KEYCHAIN_ACCESS_GROUP);
            let priv_attrs = new_dict();
            CFDictionarySetValue(priv_attrs.0, kSecAttrIsPermanent, kCFBooleanTrue);
            CFDictionarySetValue(priv_attrs.0, kSecAttrAccessGroup, ag.0);
            CFDictionarySetValue(priv_attrs.0, kSecAttrApplicationTag, tag.0);
            CFDictionarySetValue(priv_attrs.0, kSecAttrAccessControl, access_control);

            // 鍵生成パラメータ
            let size_num = cf_num_i32(256);
            let params = new_dict();
            CFDictionarySetValue(params.0, kSecAttrKeyType, kSecAttrKeyTypeEC);
            CFDictionarySetValue(params.0, kSecAttrKeySizeInBits, size_num.0);
            CFDictionarySetValue(params.0, kSecAttrTokenID, kSecAttrTokenIDSecureEnclave);
            CFDictionarySetValue(params.0, kSecPrivateKeyAttrs, priv_attrs.0);

            let mut error: CFErrorRef = ptr::null();
            let key = SecKeyCreateRandomKey(params.0, &mut error);
            // RAII で error を管理（成功・失敗どちらのパスでも確実に解放）
            let _error_guard = if !error.is_null() {
                Some(CfOwned(error))
            } else {
                None
            };

            if key.is_null() {
                return Err(format!(
                    "Secure Enclave key generation failed: {}",
                    cf_error_to_string(error)
                ));
            }
            Ok(CfOwned(key))
        }
    }

    /// SE 秘密鍵を取得または生成する。
    unsafe fn get_or_create_se_key() -> Result<CfOwned, String> {
        unsafe {
            if let Some(k) = load_se_key() {
                return Ok(k);
            }
            generate_se_key()
        }
    }

    // ── 公開 API ──────────────────────────────────────────────────────────────

    /// SE の利用可能性を確認する（鍵の取得/生成を試みる）。
    pub fn check_availability() -> Result<(), String> {
        unsafe { get_or_create_se_key().map(|_| ()) }
    }

    /// ECIES で平文を暗号化し base64 文字列を返す。
    pub fn encrypt(plaintext: &[u8]) -> Result<String, String> {
        unsafe {
            let priv_key = get_or_create_se_key()?;

            // 公開鍵を取得（暗号化に使用）
            let pub_key = CfOwned(SecKeyCopyPublicKey(priv_key.0));
            if pub_key.0.is_null() {
                return Err("Secure Enclave public key copy failed".to_string());
            }

            let pt_data = cf_data(plaintext);
            let mut error: CFErrorRef = ptr::null();
            let encrypted = CfOwned(SecKeyCreateEncryptedData(
                pub_key.0,
                kSecKeyAlgorithmECIESEncryptionCofactorX963SHA256AESGCM,
                pt_data.0,
                &mut error,
            ));
            let _err_guard = if !error.is_null() {
                Some(CfOwned(error))
            } else {
                None
            };

            if encrypted.0.is_null() {
                return Err("Secure Enclave encryption failed".to_string());
            }

            let len = CFDataGetLength(encrypted.0) as usize;
            let bytes_ptr = CFDataGetBytePtr(encrypted.0);
            let bytes = std::slice::from_raw_parts(bytes_ptr, len);
            Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
    }

    /// ECIES で base64 暗号文を復号する。
    pub fn decrypt(b64: &str) -> Result<Zeroizing<Vec<u8>>, String> {
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("base64 decode failed: {}", e))?;

        unsafe {
            let priv_key = get_or_create_se_key()?;

            let ct_data = cf_data(&ciphertext);
            let mut error: CFErrorRef = ptr::null();
            let decrypted = CfOwned(SecKeyCreateDecryptedData(
                priv_key.0,
                kSecKeyAlgorithmECIESEncryptionCofactorX963SHA256AESGCM,
                ct_data.0,
                &mut error,
            ));
            let _err_guard = if !error.is_null() {
                Some(CfOwned(error))
            } else {
                None
            };

            if decrypted.0.is_null() {
                return Err("Secure Enclave decryption failed".to_string());
            }

            let len = CFDataGetLength(decrypted.0) as usize;
            let bytes_ptr = CFDataGetBytePtr(decrypted.0);
            let bytes = Zeroizing::new(std::slice::from_raw_parts(bytes_ptr, len).to_vec());
            Ok(bytes)
        }
    }
}

/// Secure Enclave バックエンド（T2/Apple Silicon）
#[cfg(target_os = "macos")]
pub(super) struct SecureEnclaveStorage;

#[cfg(target_os = "macos")]
impl SecureEnclaveStorage {
    pub(super) fn new() -> Result<Self, String> {
        se_impl::check_availability()?;
        Ok(Self)
    }
}

#[cfg(target_os = "macos")]
impl SecureStorage for SecureEnclaveStorage {
    fn encrypt(&self, _label: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        let b64 = se_impl::encrypt(plaintext)?;
        Ok(format!("se:{}", b64).into_bytes())
    }

    fn decrypt(&self, _label: &str, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        let marker = String::from_utf8_lossy(ciphertext);
        if let Some(b64) = marker.strip_prefix("se:") {
            // SE 暗号化エントリ
            se_impl::decrypt(b64)
        } else {
            // 旧 keyring 製エントリは古いACLで保護されており触れるとパスワードプロンプトが出る。
            // 再ログインを促すため、見つからないエラーを返す。
            Err("legacy keychain entry — re-login required".to_string())
        }
    }

    fn backend_name(&self) -> String {
        "macOS Secure Enclave".to_string()
    }
}

// ── macOS AES-256-GCM (machine-bound) ─────────────────────────────────────────
//
// Keychain（DPK・legacy 問わず）は ad-hoc 署名では kSecAttrAccessGroup に
// 有効な Team ID プレフィックスが必須（-34018）。Developer ID 配布前の回避策として、
// IOPlatformUUID をキーマテリアルとした AES-256-GCM でファイルに保存する。
// SE が成功した場合（Developer ID 署名時）はそちらが優先される。

#[cfg(target_os = "macos")]
mod macos_aes {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    use hkdf::Hkdf;
    use sha2::Sha256;
    use zeroize::Zeroizing;

    fn hardware_uuid() -> Zeroizing<String> {
        let Ok(out) = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
        else {
            return Zeroizing::new("fallback-uuid-hikyou-launcher-000".to_string());
        };
        Zeroizing::new(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .find(|l| l.contains("IOPlatformUUID"))
                .and_then(|l| l.split('"').nth(3))
                .unwrap_or("fallback-uuid-hikyou-launcher-000")
                .to_string(),
        )
    }

    pub fn derive_key() -> Zeroizing<[u8; 32]> {
        let uuid = hardware_uuid();
        let hk = Hkdf::<Sha256>::new(Some(b"hikyou-launcher-macos-v1"), uuid.as_bytes());
        let mut key = Zeroizing::new([0u8; 32]);
        hk.expand(b"aes-256-gcm-key", &mut *key)
            .expect("HKDF expand: 32 bytes is a valid output length");
        key
    }

    pub fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>, String> {
        use rand::RngCore;
        let key_bytes = derive_key();
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
        let key_bytes = derive_key();
        let cipher = Aes256Gcm::new_from_slice(&*key_bytes)
            .map_err(|_| "AES key initialization failed".to_string())?;
        let nonce_arr =
            <[u8; 12]>::try_from(nonce_bytes).map_err(|_| "invalid nonce size".to_string())?;
        let nonce = Nonce::from(nonce_arr);
        cipher
            .decrypt(&nonce, ct)
            .map(Zeroizing::new)
            .map_err(|_| "AES-GCM decryption failed (corrupt data or device mismatch)".to_string())
    }
}

#[cfg(target_os = "macos")]
pub(super) struct MacOsAesStorage;

#[cfg(target_os = "macos")]
impl SecureStorage for MacOsAesStorage {
    fn encrypt(&self, _label: &str, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        macos_aes::encrypt(plaintext)
    }

    fn decrypt(&self, _label: &str, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        let s = String::from_utf8_lossy(ciphertext);
        if s.starts_with("se:") || s.starts_with("keychain") {
            return Err("legacy entry — re-login required".to_string());
        }
        macos_aes::decrypt(ciphertext)
    }

    fn backend_name(&self) -> String {
        "macOS AES-256-GCM (machine-bound)".to_string()
    }
}
