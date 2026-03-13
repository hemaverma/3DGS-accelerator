//! Azure Blob Storage authentication handling
//!
//! Supports three authentication methods per PRD Section 6.3:
//! 1. Connection string (AZURE_STORAGE_CONNECTION_STRING)
//! 2. SAS token (AZURE_STORAGE_ACCOUNT + AZURE_STORAGE_SAS_TOKEN)
//! 3. Managed Identity (AZURE_STORAGE_ACCOUNT + AZURE_USE_MANAGED_IDENTITY)

use anyhow::{bail, Context, Result};
use std::env;
use tracing::{debug, info, instrument};

/// Azure authentication configuration
#[derive(Debug, Clone)]
pub struct AzureAuth {
    pub method: AzureAuthMethod,
    pub storage_account: Option<String>,
}

/// Supported Azure authentication methods
#[derive(Debug, Clone)]
pub enum AzureAuthMethod {
    /// Connection string authentication
    ConnectionString(String),
    /// SAS token authentication
    SasToken { account: String, token: String },
    /// Managed Identity authentication (Azure VM/ACI)
    ManagedIdentity { account: String },
}

impl AzureAuth {
    /// Detect and configure Azure authentication from environment variables
    ///
    /// This function auto-detects which authentication method to use based on
    /// the presence of specific environment variables. Priority order:
    /// 1. Connection string (AZURE_STORAGE_CONNECTION_STRING)
    /// 2. SAS token (AZURE_STORAGE_ACCOUNT + AZURE_STORAGE_SAS_TOKEN)
    /// 3. Managed Identity (AZURE_STORAGE_ACCOUNT + AZURE_USE_MANAGED_IDENTITY)
    ///
    /// # Security
    /// Connection strings and tokens are redacted in logs via #[instrument(skip(...))]
    #[instrument(skip_all)]
    pub fn from_env() -> Result<Self> {
        // Option 1: Connection string
        if let Ok(conn_str) = env::var("AZURE_STORAGE_CONNECTION_STRING") {
            if conn_str.is_empty() {
                bail!("AZURE_STORAGE_CONNECTION_STRING is set but empty");
            }

            // Extract account name from connection string for verification
            let storage_account = Self::extract_account_from_connection_string(&conn_str)?;

            info!(
                method = "connection_string",
                account = %storage_account,
                "Azure authentication configured"
            );

            return Ok(AzureAuth {
                method: AzureAuthMethod::ConnectionString(conn_str),
                storage_account: Some(storage_account),
            });
        }

        // For Options 2 and 3, we need AZURE_STORAGE_ACCOUNT
        let storage_account = env::var("AZURE_STORAGE_ACCOUNT").context(
            "AZURE_STORAGE_ACCOUNT not set (required for SAS token or Managed Identity)",
        )?;

        if storage_account.is_empty() {
            bail!("AZURE_STORAGE_ACCOUNT is set but empty");
        }

        // Option 2: SAS Token
        if let Ok(sas_token) = env::var("AZURE_STORAGE_SAS_TOKEN") {
            if sas_token.is_empty() {
                bail!("AZURE_STORAGE_SAS_TOKEN is set but empty");
            }

            info!(
                method = "sas_token",
                account = %storage_account,
                "Azure authentication configured"
            );

            return Ok(AzureAuth {
                method: AzureAuthMethod::SasToken {
                    account: storage_account.clone(),
                    token: sas_token,
                },
                storage_account: Some(storage_account),
            });
        }

        // Option 3: Managed Identity
        if let Ok(use_managed_identity) = env::var("AZURE_USE_MANAGED_IDENTITY") {
            if use_managed_identity.to_lowercase() == "true" {
                info!(
                    method = "managed_identity",
                    account = %storage_account,
                    "Azure authentication configured"
                );

                return Ok(AzureAuth {
                    method: AzureAuthMethod::ManagedIdentity {
                        account: storage_account.clone(),
                    },
                    storage_account: Some(storage_account),
                });
            }
        }

        bail!(
            "No valid Azure authentication method found. Set one of:\n\
             - AZURE_STORAGE_CONNECTION_STRING\n\
             - AZURE_STORAGE_ACCOUNT + AZURE_STORAGE_SAS_TOKEN\n\
             - AZURE_STORAGE_ACCOUNT + AZURE_USE_MANAGED_IDENTITY=true"
        );
    }

    /// Extract storage account name from connection string
    fn extract_account_from_connection_string(conn_str: &str) -> Result<String> {
        for part in conn_str.split(';') {
            if let Some(account) = part.strip_prefix("AccountName=") {
                return Ok(account.to_string());
            }
        }
        bail!("Could not extract AccountName from connection string");
    }

    /// Get storage account name
    pub fn account_name(&self) -> Option<&str> {
        self.storage_account.as_deref()
    }

    /// Generate blobfuse2 config file content for this auth method
    ///
    /// # Security
    /// This method handles sensitive credentials. Ensure the returned config
    /// is written to a file with restricted permissions (0600).
    #[instrument(skip(self))]
    pub fn to_blobfuse_config(&self, container_name: &str, mount_path: &str) -> String {
        debug!(container = %container_name, mount_path = %mount_path, "Generating blobfuse2 config");

        match &self.method {
            AzureAuthMethod::ConnectionString(conn_str) => {
                format!(
                    r#"# Blobfuse2 configuration for container: {}
# Mount path: {}

allow-other: true
file-cache-timeout-in-seconds: 120
log-level: log_warning

components:
  - libfuse
  - file_cache
  - attr_cache
  - azstorage

libfuse:
  attribute-expiration-sec: 120
  entry-expiration-sec: 120
  negative-entry-expiration-sec: 240

file_cache:
  path: /tmp/blobfuse-cache/{}
  timeout-sec: 120
  max-size-mb: 4096

attr_cache:
  timeout-sec: 7200

azstorage:
  type: block
  account-name: {}
  endpoint: https://{}.blob.core.windows.net
  mode: key
  account-key: {}
  container: {}
"#,
                    container_name,
                    mount_path,
                    container_name,
                    self.storage_account.as_ref().unwrap(),
                    self.storage_account.as_ref().unwrap(),
                    Self::extract_key_from_connection_string(conn_str)
                        .unwrap_or_else(|_| "INVALID_KEY".to_string()),
                    container_name
                )
            }
            AzureAuthMethod::SasToken { account, token } => {
                format!(
                    r#"# Blobfuse2 configuration for container: {}
# Mount path: {}

allow-other: true
file-cache-timeout-in-seconds: 120
log-level: log_warning

components:
  - libfuse
  - file_cache
  - attr_cache
  - azstorage

libfuse:
  attribute-expiration-sec: 120
  entry-expiration-sec: 120
  negative-entry-expiration-sec: 240

file_cache:
  path: /tmp/blobfuse-cache/{}
  timeout-sec: 120
  max-size-mb: 4096

attr_cache:
  timeout-sec: 7200

azstorage:
  type: block
  account-name: {}
  endpoint: https://{}.blob.core.windows.net
  mode: sas
  sas: {}
  container: {}
"#,
                    container_name,
                    mount_path,
                    container_name,
                    account,
                    account,
                    token,
                    container_name
                )
            }
            AzureAuthMethod::ManagedIdentity { account } => {
                format!(
                    r#"# Blobfuse2 configuration for container: {}
# Mount path: {}

allow-other: true
file-cache-timeout-in-seconds: 120
log-level: log_warning

components:
  - libfuse
  - file_cache
  - attr_cache
  - azstorage

libfuse:
  attribute-expiration-sec: 120
  entry-expiration-sec: 120
  negative-entry-expiration-sec: 240

file_cache:
  path: /tmp/blobfuse-cache/{}
  timeout-sec: 120
  max-size-mb: 4096

attr_cache:
  timeout-sec: 7200

azstorage:
  type: block
  account-name: {}
  endpoint: https://{}.blob.core.windows.net
  mode: msi
  container: {}
"#,
                    container_name, mount_path, container_name, account, account, container_name
                )
            }
        }
    }

    /// Extract account key from connection string
    fn extract_key_from_connection_string(conn_str: &str) -> Result<String> {
        for part in conn_str.split(';') {
            if let Some(key) = part.strip_prefix("AccountKey=") {
                return Ok(key.to_string());
            }
        }
        bail!("Could not extract AccountKey from connection string");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_account_from_connection_string() {
        let conn_str = "DefaultEndpointsProtocol=https;AccountName=myaccount;AccountKey=abc123==;EndpointSuffix=core.windows.net";
        let account = AzureAuth::extract_account_from_connection_string(conn_str).unwrap();
        assert_eq!(account, "myaccount");
    }

    #[test]
    fn test_extract_key_from_connection_string() {
        let conn_str = "DefaultEndpointsProtocol=https;AccountName=myaccount;AccountKey=abc123==;EndpointSuffix=core.windows.net";
        let key = AzureAuth::extract_key_from_connection_string(conn_str).unwrap();
        assert_eq!(key, "abc123==");
    }

    #[test]
    fn test_blobfuse_config_connection_string() {
        let auth = AzureAuth {
            method: AzureAuthMethod::ConnectionString(
                "DefaultEndpointsProtocol=https;AccountName=test;AccountKey=key123==;EndpointSuffix=core.windows.net".to_string()
            ),
            storage_account: Some("test".to_string()),
        };

        let config = auth.to_blobfuse_config("input", "/mnt/input");
        assert!(config.contains("container: input"));
        assert!(config.contains("account-name: test"));
        assert!(config.contains("mode: key"));
    }

    #[test]
    fn test_blobfuse_config_sas_token() {
        let auth = AzureAuth {
            method: AzureAuthMethod::SasToken {
                account: "test".to_string(),
                token: "sv=2021-06-08&ss=b&srt=sco&sp=rwdlac".to_string(),
            },
            storage_account: Some("test".to_string()),
        };

        let config = auth.to_blobfuse_config("output", "/mnt/output");
        assert!(config.contains("container: output"));
        assert!(config.contains("account-name: test"));
        assert!(config.contains("mode: sas"));
    }

    #[test]
    fn test_blobfuse_config_managed_identity() {
        let auth = AzureAuth {
            method: AzureAuthMethod::ManagedIdentity {
                account: "test".to_string(),
            },
            storage_account: Some("test".to_string()),
        };

        let config = auth.to_blobfuse_config("processed", "/mnt/processed");
        assert!(config.contains("container: processed"));
        assert!(config.contains("account-name: test"));
        assert!(config.contains("mode: msi"));
    }
}
