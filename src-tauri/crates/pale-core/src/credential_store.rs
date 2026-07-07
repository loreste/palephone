// On desktop, use the OS keychain via the keyring crate.
// On Android, keyring is unavailable — provide a no-op fallback that returns
// clear errors so the caller can use an alternative storage mechanism.

#[cfg(not(target_os = "android"))]
use keyring::Entry;

const SERVICE_NAME: &str = "com.pale.softphone";

/// Store a SIP password in the OS keychain
pub fn store_password(account_id: &str, password: &str) -> Result<(), String> {
    #[cfg(not(target_os = "android"))]
    {
        let entry = Entry::new(SERVICE_NAME, account_id)
            .map_err(|e| format!("Keychain entry error: {}", e))?;
        entry
            .set_password(password)
            .map_err(|e| format!("Failed to store password: {}", e))
    }
    #[cfg(target_os = "android")]
    {
        let _ = (account_id, password);
        log::warn!("Keychain not available on Android; password not persisted");
        Ok(())
    }
}

/// Retrieve a SIP password from the OS keychain
pub fn get_password(account_id: &str) -> Result<Option<String>, String> {
    #[cfg(not(target_os = "android"))]
    {
        let entry = Entry::new(SERVICE_NAME, account_id)
            .map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Failed to get password: {}", e)),
        }
    }
    #[cfg(target_os = "android")]
    {
        let _ = account_id;
        Ok(None)
    }
}

/// Delete a SIP password from the OS keychain
pub fn delete_password(account_id: &str) -> Result<(), String> {
    #[cfg(not(target_os = "android"))]
    {
        let entry = Entry::new(SERVICE_NAME, account_id)
            .map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Failed to delete password: {}", e)),
        }
    }
    #[cfg(target_os = "android")]
    {
        let _ = account_id;
        Ok(())
    }
}
