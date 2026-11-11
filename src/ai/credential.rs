// SPDX-License-Identifier: AGPL-3.0-or-later
//! API anahtarları için güvenli saklama ve maskeleme altyapısı.
//! Anahtarlar asla proje dosyalarına veya günlük dosyalarına düz metin yazılmaz.

use std::collections::HashMap;
use std::sync::RwLock;

/// Kimlik bilgisi saklama arayüzü.
pub trait CredentialStorage: Send + Sync {
    fn get(&self, target: &str) -> Option<String>;
    fn set(&self, target: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, target: &str) -> Result<(), String>;
    fn set_store_in_wincred(&self, _enabled: bool) {}
    fn is_store_in_wincred(&self) -> bool {
        false
    }
}

/// Yalnızca oturum boyunca bellekte saklayan (diske yazmayan) depo.
#[derive(Default)]
pub struct SessionCredentialStore {
    store: RwLock<HashMap<String, String>>,
}

impl SessionCredentialStore {
    pub fn new() -> Self {
        Self {
            store: RwLock::new(HashMap::new()),
        }
    }
}

impl CredentialStorage for SessionCredentialStore {
    fn get(&self, target: &str) -> Option<String> {
        self.store.read().ok()?.get(target).cloned()
    }

    fn set(&self, target: &str, secret: &str) -> Result<(), String> {
        let mut lock = self.store.write().map_err(|e| e.to_string())?;
        lock.insert(target.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, target: &str) -> Result<(), String> {
        let mut lock = self.store.write().map_err(|e| e.to_string())?;
        lock.remove(target);
        Ok(())
    }
}

/// Birim ve entegrasyon testlerinde kullanılan sahte (mock) kimlik bilgisi deposu.
/// Gerçek Windows Kimlik Bilgisi Yöneticisi'ne veya kullanıcı dizinlerine asla dokunmaz.
#[derive(Default)]
pub struct MockCredentialStore {
    inner: SessionCredentialStore,
    vault: SessionCredentialStore,
    store_in_wincred: std::sync::atomic::AtomicBool,
}

impl MockCredentialStore {
    pub fn new() -> Self {
        Self {
            inner: SessionCredentialStore::new(),
            vault: SessionCredentialStore::new(),
            store_in_wincred: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn new_with_persistence(store_in_wincred: bool) -> Self {
        Self {
            inner: SessionCredentialStore::new(),
            vault: SessionCredentialStore::new(),
            store_in_wincred: std::sync::atomic::AtomicBool::new(store_in_wincred),
        }
    }

    pub fn is_in_vault(&self, target: &str) -> bool {
        self.vault.get(target).is_some()
    }
}

impl CredentialStorage for MockCredentialStore {
    fn get(&self, target: &str) -> Option<String> {
        if let Some(val) = self.inner.get(target) {
            return Some(val);
        }
        if self
            .store_in_wincred
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return self.vault.get(target);
        }
        None
    }

    fn set(&self, target: &str, secret: &str) -> Result<(), String> {
        self.inner.set(target, secret)?;
        if self
            .store_in_wincred
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            self.vault.set(target, secret)?;
        }
        Ok(())
    }

    fn delete(&self, target: &str) -> Result<(), String> {
        self.inner.delete(target)?;
        self.vault.delete(target)?;
        Ok(())
    }

    fn set_store_in_wincred(&self, enabled: bool) {
        let prev = self
            .store_in_wincred
            .swap(enabled, std::sync::atomic::Ordering::SeqCst);
        if prev && !enabled {
            // true'dan false'a geçişte kayıtlı anahtarı sil
            let _ = self.vault.store.write().map(|mut map| map.clear());
        }
    }

    fn is_store_in_wincred(&self) -> bool {
        self.store_in_wincred
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

// --- Windows Kimlik Bilgisi Yöneticisi (WinCred) Dinamik Bağlantısı ---
#[cfg(windows)]
#[repr(C)]
pub struct CredentialW {
    pub flags: u32,
    pub cred_type: u32,
    pub target_name: *mut u16,
    pub comment: *mut u16,
    pub last_written: [u32; 2],
    pub credential_blob_size: u32,
    pub credential_blob: *mut u8,
    pub persist: u32,
    pub attribute_count: u32,
    pub attributes: *mut std::ffi::c_void,
    pub target_alias: *mut u16,
    pub user_name: *mut u16,
}

#[cfg(windows)]
pub const CRED_TYPE_GENERIC: u32 = 1;
#[cfg(windows)]
pub const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;

type CredWriteWFn = unsafe extern "system" fn(*const CredentialW, u32) -> i32;
type CredReadWFn = unsafe extern "system" fn(*const u16, u32, u32, *mut *mut CredentialW) -> i32;
type CredDeleteWFn = unsafe extern "system" fn(*const u16, u32, u32) -> i32;
type CredFreeFn = unsafe extern "system" fn(*mut std::ffi::c_void);

/// Windows Credential Manager kullanan depo (mevcut değilse oturum deposuna geri düşer).
pub struct WinCredStore {
    fallback: SessionCredentialStore,
    store_in_wincred: std::sync::atomic::AtomicBool,
}

impl Default for WinCredStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WinCredStore {
    pub fn new() -> Self {
        Self {
            fallback: SessionCredentialStore::new(),
            store_in_wincred: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn new_with_persistence(store_in_wincred: bool) -> Self {
        Self {
            fallback: SessionCredentialStore::new(),
            store_in_wincred: std::sync::atomic::AtomicBool::new(store_in_wincred),
        }
    }

    pub fn set_store_in_wincred(&self, enabled: bool) {
        let prev = self
            .store_in_wincred
            .swap(enabled, std::sync::atomic::Ordering::SeqCst);
        if prev && !enabled {
            crate::footprint::clear_credential_vault();
        }
    }

    pub fn is_store_in_wincred(&self) -> bool {
        self.store_in_wincred
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(windows)]
    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[cfg(windows)]
    fn get_advapi_fns() -> Option<(CredReadWFn, CredWriteWFn, CredDeleteWFn, CredFreeFn)> {
        use std::ffi::CString;
        unsafe {
            unsafe extern "system" {
                fn LoadLibraryA(lpLibFileName: *const i8) -> *mut std::ffi::c_void;
                fn GetProcAddress(
                    hModule: *mut std::ffi::c_void,
                    lpProcName: *const i8,
                ) -> *mut std::ffi::c_void;
            }
            let lib_name = CString::new("advapi32.dll").ok()?;
            let hmod = LoadLibraryA(lib_name.as_ptr());
            if hmod.is_null() {
                return None;
            }

            let read_name = CString::new("CredReadW").ok()?;
            let write_name = CString::new("CredWriteW").ok()?;
            let del_name = CString::new("CredDeleteW").ok()?;
            let free_name = CString::new("CredFree").ok()?;

            let p_read = GetProcAddress(hmod, read_name.as_ptr());
            let p_write = GetProcAddress(hmod, write_name.as_ptr());
            let p_del = GetProcAddress(hmod, del_name.as_ptr());
            let p_free = GetProcAddress(hmod, free_name.as_ptr());

            if p_read.is_null() || p_write.is_null() || p_del.is_null() || p_free.is_null() {
                return None;
            }

            Some((
                std::mem::transmute::<*mut std::ffi::c_void, CredReadWFn>(p_read),
                std::mem::transmute::<*mut std::ffi::c_void, CredWriteWFn>(p_write),
                std::mem::transmute::<*mut std::ffi::c_void, CredDeleteWFn>(p_del),
                std::mem::transmute::<*mut std::ffi::c_void, CredFreeFn>(p_free),
            ))
        }
    }
}

impl CredentialStorage for WinCredStore {
    fn set_store_in_wincred(&self, enabled: bool) {
        self.set_store_in_wincred(enabled);
    }

    fn is_store_in_wincred(&self) -> bool {
        self.is_store_in_wincred()
    }

    fn get(&self, target: &str) -> Option<String> {
        if let Some(val) = self.fallback.get(target) {
            return Some(val);
        }
        #[cfg(windows)]
        {
            if self
                .store_in_wincred
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                if let Some((cred_read, _, _, cred_free)) = Self::get_advapi_fns() {
                    let wide_target = Self::to_wide(target);
                    let mut cred_ptr: *mut CredentialW = std::ptr::null_mut();
                    unsafe {
                        let ret = cred_read(
                            wide_target.as_ptr(),
                            CRED_TYPE_GENERIC,
                            0,
                            &mut cred_ptr as *mut _,
                        );
                        if ret != 0 && !cred_ptr.is_null() {
                            let blob_size = (*cred_ptr).credential_blob_size as usize;
                            let blob_ptr = (*cred_ptr).credential_blob;
                            let val = if !blob_ptr.is_null() && blob_size > 0 {
                                let slice = std::slice::from_raw_parts(blob_ptr, blob_size);
                                String::from_utf8(slice.to_vec()).ok()
                            } else {
                                None
                            };
                            cred_free(cred_ptr as *mut _);
                            if val.is_some() {
                                return val;
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn set(&self, target: &str, secret: &str) -> Result<(), String> {
        self.fallback.set(target, secret)?;
        #[cfg(windows)]
        {
            if self
                .store_in_wincred
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                if let Some((_, cred_write, _, _)) = Self::get_advapi_fns() {
                    let mut wide_target = Self::to_wide(target);
                    let mut wide_user = Self::to_wide("WerSeuS_User");
                    let mut secret_bytes = secret.as_bytes().to_vec();

                    let cred = CredentialW {
                        flags: 0,
                        cred_type: CRED_TYPE_GENERIC,
                        target_name: wide_target.as_mut_ptr(),
                        comment: std::ptr::null_mut(),
                        last_written: [0, 0],
                        credential_blob_size: secret_bytes.len() as u32,
                        credential_blob: secret_bytes.as_mut_ptr(),
                        persist: CRED_PERSIST_LOCAL_MACHINE,
                        attribute_count: 0,
                        attributes: std::ptr::null_mut(),
                        target_alias: std::ptr::null_mut(),
                        user_name: wide_user.as_mut_ptr(),
                    };

                    unsafe {
                        let ret = cred_write(&cred, 0);
                        if ret == 0 {
                            return Err("CredWriteW failed".into());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn delete(&self, target: &str) -> Result<(), String> {
        self.fallback.delete(target)?;
        #[cfg(windows)]
        {
            if let Some((_, _, cred_delete, _)) = Self::get_advapi_fns() {
                let wide_target = Self::to_wide(target);
                unsafe {
                    let _ = cred_delete(wide_target.as_ptr(), CRED_TYPE_GENERIC, 0);
                }
            }
        }
        Ok(())
    }
}

/// Arayüzde göstermek üzere API anahtarını güvenli şekilde maskeler (●●●●●●).
pub fn mask_api_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let len = trimmed.chars().count();
    if len <= 6 {
        return "●".repeat(len);
    }
    let first: String = trimmed.chars().take(3).collect();
    let last: String = trimmed.chars().skip(len - 3).collect();
    format!("{}••••••••{}", first, last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_store_crud() {
        let store = MockCredentialStore::new();
        assert_eq!(store.get("test_key"), None);

        store.set("test_key", "secret_12345").unwrap();
        assert_eq!(store.get("test_key"), Some("secret_12345".into()));

        store.delete("test_key").unwrap();
        assert_eq!(store.get("test_key"), None);
    }

    #[test]
    fn test_mask_api_key() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("123"), "●●●");
        assert_eq!(mask_api_key("sk-1234567890abcdef"), "sk-••••••••def");
    }

    #[test]
    fn test_store_in_wincred_preference_and_transition() {
        let store = MockCredentialStore::new_with_persistence(false);
        assert!(!store.is_store_in_wincred());

        // store_in_wincred = false: Yalnızca oturum belleğinde tutulur, kasaya yazılmaz
        store
            .set("WerSeuS_AI_ApiKey_test", "session_secret_123")
            .unwrap();
        assert_eq!(
            store.get("WerSeuS_AI_ApiKey_test"),
            Some("session_secret_123".into())
        );
        assert!(!store.is_in_vault("WerSeuS_AI_ApiKey_test"));

        // store_in_wincred = true'ya geçiş
        store.set_store_in_wincred(true);
        assert!(store.is_store_in_wincred());
        store
            .set("WerSeuS_AI_ApiKey_test2", "vault_secret_456")
            .unwrap();
        assert!(store.is_in_vault("WerSeuS_AI_ApiKey_test2"));

        // true'dan false'a geçişte kasadaki kayıtlı anahtarlar silinmeli
        store.set_store_in_wincred(false);
        assert!(!store.is_store_in_wincred());
        assert!(!store.is_in_vault("WerSeuS_AI_ApiKey_test2"));
    }

    #[test]
    fn switching_wincred_off_purges_all_provider_keys_but_keeps_session_copy() {
        let store = MockCredentialStore::new_with_persistence(true);
        let targets = [
            "WerSeuS_AI_ApiKey_openai",
            "WerSeuS_AI_ApiKey_anthropic",
            "WerSeuS_AI_ApiKey_gemini",
        ];
        for t in targets {
            store.set(t, "secret-value").unwrap();
            assert!(store.is_in_vault(t));
        }
        // Arayüz yalnızca soyut arayüz üzerinden çağırır.
        let dynstore: &dyn CredentialStorage = &store;
        dynstore.set_store_in_wincred(false);
        for t in targets {
            assert!(!store.is_in_vault(t), "{t} kasadan silinmeli");
            assert_eq!(
                store.get(t),
                Some("secret-value".into()),
                "oturum kopyası kalmalı"
            );
        }
        // Zaten kapalıyken tekrar kapatmak zararsız
        dynstore.set_store_in_wincred(false);
    }

    #[test]
    fn wincred_store_transition_flag_in_tests_never_touches_real_vault() {
        // cfg(test) altında clear_credential_vault gerçek Credential Manager'a dokunmaz.
        let store = WinCredStore::new_with_persistence(true);
        let dynstore: &dyn CredentialStorage = &store;
        assert!(dynstore.is_store_in_wincred());
        dynstore.set_store_in_wincred(false);
        assert!(!dynstore.is_store_in_wincred());
    }
}
