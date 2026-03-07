use brain::error::Result;
use async_trait::async_trait;
use crate::types::OAuthToken;

#[async_trait]
pub trait TokenStore: Send + Sync {
    /// Save a token for a provider
    async fn save_token(&self, provider: &str, token: OAuthToken) -> Result<()>;
    
    /// Get a token for a provider
    async fn get_token(&self, provider: &str) -> Result<Option<OAuthToken>>;
    
    /// Delete a token
    async fn delete_token(&self, provider: &str) -> Result<()>;
}

/// A simple file-based token store (JSON)
/// WARNING: Stores tokens in plain text if encryption is not added.
/// For production, use keyring or encrypted database.
pub struct FileTokenStore {
    path: std::path::PathBuf,
}

impl FileTokenStore {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
    
    async fn load(&self) -> Result<std::collections::HashMap<String, OAuthToken>> {
        if !self.path.exists() {
            return Ok(std::collections::HashMap::new());
        }
        let content = tokio::fs::read_to_string(&self.path).await?;
        let map = serde_json::from_str(&content).unwrap_or_default();
        Ok(map)
    }
    
    async fn save(&self, map: &std::collections::HashMap<String, OAuthToken>) -> Result<()> {
        let content = serde_json::to_string_pretty(map)?;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&self.path, content).await?;
        Ok(())
    }
}

#[async_trait]
impl TokenStore for FileTokenStore {
    async fn save_token(&self, provider: &str, token: OAuthToken) -> Result<()> {
        // Use a lock if concurrent access is expected, but for now purely async file IO
        // Simple read-modify-write (race condition possible but low risk for single user)
        let mut map = self.load().await?;
        map.insert(provider.to_string(), token);
        self.save(&map).await
    }
    
    async fn get_token(&self, provider: &str) -> Result<Option<OAuthToken>> {
        let map = self.load().await?;
        Ok(map.get(provider).cloned())
    }
    
    async fn delete_token(&self, provider: &str) -> Result<()> {
        let mut map = self.load().await?;
        map.remove(provider);
        self.save(&map).await
    }
}

/// A token store backed by the encrypted Vault
pub struct VaultTokenStore {
    vault: std::sync::Arc<crate::vault::Vault>,
}

impl VaultTokenStore {
    pub fn new(vault: std::sync::Arc<crate::vault::Vault>) -> Self {
        Self { vault }
    }
}

#[async_trait]
impl TokenStore for VaultTokenStore {
    async fn save_token(&self, provider: &str, token: OAuthToken) -> Result<()> {
        let key = format!("oauth_token_{}", provider);
        let val = serde_json::to_string(&token)?;
        self.vault.set(&key, &val)
    }

    async fn get_token(&self, provider: &str) -> Result<Option<OAuthToken>> {
        let key = format!("oauth_token_{}", provider);
        match self.vault.get(&key)? {
            Some(val) => {
                let token = serde_json::from_str(&val)?;
                Ok(Some(token))
            }
            None => Ok(None),
        }
    }

    async fn delete_token(&self, provider: &str) -> Result<()> {
        let key = format!("oauth_token_{}", provider);
        self.vault.delete(&key)
    }
}
