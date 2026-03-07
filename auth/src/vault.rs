use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use brain::error::{Error, Result};
use keyring::Entry;
use rand::{rngs::OsRng, RngCore};
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

const VAULT_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("secrets");
const KEYRING_SERVICE: &str = "aimaxxing-vault";
const KEYRING_USER: &str = "local-user";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretBundle {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

pub struct Vault {
    db: Database,
    key: [u8; 32],
}

impl Vault {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path)
            .map_err(|e| Error::Internal(format!("Failed to open Vault database: {}", e)))?;

        // Ensure table exists
        {
            let write_txn = db
                .begin_write()
                .map_err(|e| Error::Internal(format!("Failed to begin write txn: {}", e)))?;
            {
                let _ = write_txn
                    .open_table(VAULT_TABLE)
                    .map_err(|e| Error::Internal(format!("Failed to open/create table: {}", e)))?;
            }
            write_txn
                .commit()
                .map_err(|e| Error::Internal(format!("Failed to commit write txn: {}", e)))?;
        }

        let key = Self::get_or_create_key()?;

        Ok(Self { db, key })
    }

    /// Retrieve or generate a 256-bit encryption key from OS keyring.
    fn get_or_create_key() -> Result<[u8; 32]> {
        let entry = Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| Error::Internal(format!("Keyring initialization failed: {}", e)))?;

        match entry.get_password() {
            Ok(hex_key) => {
                let mut key = [0u8; 32];
                hex::decode_to_slice(&hex_key, &mut key)
                    .map_err(|e| Error::Internal(format!("Keyring data corruption: {}", e)))?;
                debug!("Master key retrieved from OS keyring.");
                Ok(key)
            }
            Err(_) => {
                info!("Generating new master key for Vault and storing in OS keyring...");
                let mut key = [0u8; 32];
                OsRng.fill_bytes(&mut key);
                let hex_key = hex::encode(key);
                entry.set_password(&hex_key).map_err(|e| {
                    Error::Internal(format!("Failed to save master key to OS keyring: {}", e))
                })?;
                Ok(key)
            }
        }
    }

    /// Store a secret with a given key.
    pub fn set(&self, name: &str, value: &str) -> Result<()> {
        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|_| Error::Internal("Invalid encryption key".to_string()))?;

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, value.as_bytes())
            .map_err(|e| Error::Internal(format!("Encryption failed: {}", e)))?;

        let bundle = SecretBundle {
            nonce: nonce_bytes,
            ciphertext,
        };

        let encoded = bincode::serialize(&bundle)
            .map_err(|e| Error::Internal(format!("Serialization failed: {}", e)))?;

        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Internal(format!("Write txn failed: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(VAULT_TABLE)
                .map_err(|e| Error::Internal(format!("Table open failed: {}", e)))?;
            table
                .insert(name, encoded.as_slice())
                .map_err(|e| Error::Internal(format!("Insert failed: {}", e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| Error::Internal(format!("Commit failed: {}", e)))?;

        Ok(())
    }

    /// Retrieve a secret by name.
    pub fn get(&self, name: &str) -> Result<Option<String>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Internal(format!("Read txn failed: {}", e)))?;
        let table = read_txn
            .open_table(VAULT_TABLE)
            .map_err(|e| Error::Internal(format!("Table open failed: {}", e)))?;

        let bundle_data = match table
            .get(name)
            .map_err(|e| Error::Internal(format!("Get failed: {}", e)))?
        {
            Some(v) => v.value().to_vec(),
            None => return Ok(None),
        };

        let bundle: SecretBundle = bincode::deserialize(&bundle_data)
            .map_err(|e| Error::Internal(format!("Deserialization failed: {}", e)))?;

        let cipher = Aes256Gcm::new_from_slice(&self.key)
            .map_err(|_| Error::Internal("Invalid encryption key".to_string()))?;

        let nonce = Nonce::from_slice(&bundle.nonce);
        let decrypted_bytes = cipher
            .decrypt(nonce, bundle.ciphertext.as_slice())
            .map_err(|e| Error::Internal(format!("Decryption failed: {}", e)))?;

        String::from_utf8(decrypted_bytes)
            .map(|s| Some(s))
            .map_err(|e| Error::Internal(format!("UTF-8 decoding failed: {}", e)))
    }

    /// Delete a secret.
    pub fn delete(&self, name: &str) -> Result<()> {
        let write_txn = self
            .db
            .begin_write()
            .map_err(|e| Error::Internal(format!("Write txn failed: {}", e)))?;
        {
            let mut table = write_txn
                .open_table(VAULT_TABLE)
                .map_err(|e| Error::Internal(format!("Table open failed: {}", e)))?;
            let _ = table
                .remove(name)
                .map_err(|e| Error::Internal(format!("Delete failed: {}", e)))?;
        }
        write_txn
            .commit()
            .map_err(|e| Error::Internal(format!("Commit failed: {}", e)))?;
        Ok(())
    }

    /// List all secret names.
    pub fn list_keys(&self) -> Result<Vec<String>> {
        let read_txn = self
            .db
            .begin_read()
            .map_err(|e| Error::Internal(format!("Read txn failed: {}", e)))?;
        let table = read_txn
            .open_table(VAULT_TABLE)
            .map_err(|e| Error::Internal(format!("Table open failed: {}", e)))?;

        let mut keys = Vec::new();
        for res in table
            .iter()
            .map_err(|e| Error::Internal(format!("Iter failed: {}", e)))?
        {
            let (key, _) = res.map_err(|e| Error::Internal(format!("Iter item failed: {}", e)))?;
            keys.push(key.value().to_string());
        }
        Ok(keys)
    }
}
