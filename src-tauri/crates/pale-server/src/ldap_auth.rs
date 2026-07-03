use ldap3::{LdapConnAsync, Scope, SearchEntry};
use serde::{Deserialize, Serialize};

/// Active Directory / LDAP configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapConfig {
    pub enabled: bool,
    pub server_url: String, // ldap://dc.company.com:389 or ldaps://dc.company.com:636
    pub bind_dn: String,    // CN=svc-pale,OU=Service Accounts,DC=company,DC=com
    pub bind_password: String,
    pub base_dn: String,                // DC=company,DC=com
    pub user_search_filter: String,     // (&(objectClass=user)(sAMAccountName={username}))
    pub user_dn_attribute: String,      // sAMAccountName
    pub display_name_attribute: String, // displayName
    pub email_attribute: String,        // mail
    pub group_attribute: String,        // memberOf
    pub admin_group: String,            // CN=PaleAdmins,OU=Groups,DC=company,DC=com
    pub sip_domain: String,             // company.com — used to construct SIP URIs
}

impl Default for LdapConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server_url: "ldap://localhost:389".to_string(),
            bind_dn: String::new(),
            bind_password: String::new(),
            base_dn: String::new(),
            user_search_filter: "(&(objectClass=user)(sAMAccountName={username}))".to_string(),
            user_dn_attribute: "sAMAccountName".to_string(),
            display_name_attribute: "displayName".to_string(),
            email_attribute: "mail".to_string(),
            group_attribute: "memberOf".to_string(),
            admin_group: String::new(),
            sip_domain: "company.com".to_string(),
        }
    }
}

/// Result from LDAP authentication
#[derive(Debug, Clone)]
pub struct LdapUser {
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub sip_uri: String,
    pub is_admin: bool,
}

/// Authenticate a user against Active Directory / LDAP
pub async fn ldap_authenticate(
    config: &LdapConfig,
    username: &str,
    password: &str,
) -> Result<LdapUser, String> {
    if !config.enabled {
        return Err("LDAP not enabled".to_string());
    }

    // Connect to LDAP server
    let (conn, mut ldap) = LdapConnAsync::new(&config.server_url)
        .await
        .map_err(|e| format!("LDAP connection failed: {}", e))?;

    ldap3::drive!(conn);

    // Bind with service account to search
    ldap.simple_bind(&config.bind_dn, &config.bind_password)
        .await
        .map_err(|e| format!("LDAP bind failed: {}", e))?
        .success()
        .map_err(|e| format!("LDAP bind rejected: {}", e))?;

    // Search for the user
    let filter = config.user_search_filter.replace("{username}", username);

    let (entries, _) = ldap
        .search(
            &config.base_dn,
            Scope::Subtree,
            &filter,
            vec![
                &config.user_dn_attribute,
                &config.display_name_attribute,
                &config.email_attribute,
                &config.group_attribute,
                "dn",
            ],
        )
        .await
        .map_err(|e| format!("LDAP search failed: {}", e))?
        .success()
        .map_err(|e| format!("LDAP search error: {}", e))?;

    if entries.is_empty() {
        return Err("User not found in directory".to_string());
    }

    let entry = SearchEntry::construct(entries[0].clone());
    let user_dn = &entry.dn;

    // Authenticate: bind as the user with their password
    let (conn2, mut ldap2) = LdapConnAsync::new(&config.server_url)
        .await
        .map_err(|e| format!("LDAP reconnect failed: {}", e))?;

    ldap3::drive!(conn2);

    ldap2
        .simple_bind(user_dn, password)
        .await
        .map_err(|e| format!("Authentication failed: {}", e))?
        .success()
        .map_err(|_| "Invalid credentials".to_string())?;

    // Extract user attributes
    let display_name = entry
        .attrs
        .get(&config.display_name_attribute)
        .and_then(|v| v.first())
        .cloned()
        .unwrap_or_else(|| username.to_string());

    let email = entry
        .attrs
        .get(&config.email_attribute)
        .and_then(|v| v.first())
        .cloned()
        .unwrap_or_default();

    let groups = entry
        .attrs
        .get(&config.group_attribute)
        .cloned()
        .unwrap_or_default();

    let is_admin = !config.admin_group.is_empty()
        && groups.iter().any(|g| {
            g.to_lowercase()
                .contains(&config.admin_group.to_lowercase())
        });

    let sip_uri = format!("sip:{}@{}", username, config.sip_domain);

    let _ = ldap.unbind().await;
    let _ = ldap2.unbind().await;

    Ok(LdapUser {
        username: username.to_string(),
        display_name,
        email,
        sip_uri,
        is_admin,
    })
}
