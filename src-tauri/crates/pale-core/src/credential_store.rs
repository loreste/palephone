use keyring::Entry;

const SERVICE_NAME: &str = "com.pale.softphone";

/// Store a SIP password in the OS keychain
pub fn store_password(account_id: &str, password: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, account_id)
        .map_err(|e| format!("Keychain entry error: {}", e))?;
    entry
        .set_password(password)
        .map_err(|e| format!("Failed to store password: {}", e))
}

/// Retrieve a SIP password from the OS keychain
pub fn get_password(account_id: &str) -> Result<Option<String>, String> {
    let entry = Entry::new(SERVICE_NAME, account_id)
        .map_err(|e| format!("Keychain entry error: {}", e))?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to get password: {}", e)),
    }
}

/// Delete a SIP password from the OS keychain
pub fn delete_password(account_id: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, account_id)
        .map_err(|e| format!("Keychain entry error: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
        Err(e) => Err(format!("Failed to delete password: {}", e)),
    }
}
