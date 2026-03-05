use crate::error::{Error, Result};
use std::env;

/// Trait for retrieving secrets from various backends.
pub trait SecretVault: Send + Sync {
    /// Retrieve a secret by key. Returns None if not found.
    fn get(&self, key: &str) -> Result<Option<String>>;

    /// Set a secret (if supported by the backend).
    fn set(&self, _key: &str, _value: &str) -> Result<()> {
        Err(Error::Internal(
            "Setting secrets not supported by this vault".to_string(),
        ))
    }

    /// Delete a secret (if supported by the backend).
    fn delete(&self, _key: &str) -> Result<()> {
        Err(Error::Internal(
            "Deleting secrets not supported by this vault".to_string(),
        ))
    }
}

/// A vault that reads from environment variables.
#[derive(Default)]
pub struct EnvVault;

impl SecretVault for EnvVault {
    fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(env::var(key).ok())
    }
}

/// A vault that uses the system keychain via the `keyring` crate.
#[cfg(not(target_arch = "wasm32"))]
pub struct KeyringVault {
    service: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl KeyringVault {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SecretVault for KeyringVault {
    fn get(&self, key: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(&self.service, key)
            .map_err(|e| Error::Internal(format!("Keyring error: {}", e)))?;

        match entry.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::Internal(format!("Keyring error: {}", e))),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(&self.service, key)
            .map_err(|e| Error::Internal(format!("Keyring error: {}", e)))?;

        entry
            .set_password(value)
            .map_err(|e| Error::Internal(format!("Keyring error: {}", e)))?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        let entry = keyring::Entry::new(&self.service, key)
            .map_err(|e| Error::Internal(format!("Keyring error: {}", e)))?;

        entry.delete_credential().map_err(|_e| {
            Error::Internal(format!(
                "Failed to delete from keyring (maybe it was already deleted?)"
            ))
        })?;
        Ok(())
    }
}

/// A vault that chains multiple vaults and returns the first result found.
pub struct CompositeVault {
    vaults: Vec<Box<dyn SecretVault>>,
}

impl CompositeVault {
    pub fn new() -> Self {
        Self { vaults: Vec::new() }
    }

    pub fn add(mut self, vault: Box<dyn SecretVault>) -> Self {
        self.vaults.push(vault);
        self
    }

    pub fn default_system() -> Self {
        let mut composite = Self::new();

        #[cfg(not(target_arch = "wasm32"))]
        composite
            .vaults
            .push(Box::new(KeyringVault::new("aimaxxing")));

        composite.vaults.push(Box::new(EnvVault::default()));
        composite
    }
}

impl SecretVault for CompositeVault {
    fn get(&self, key: &str) -> Result<Option<String>> {
        for vault in &self.vaults {
            if let Ok(Some(secret)) = vault.get(key) {
                return Ok(Some(secret));
            }
        }
        Ok(None)
    }
}
