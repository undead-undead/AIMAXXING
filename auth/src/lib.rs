pub mod types;
pub mod store;
pub mod manager;

pub use types::{OAuthConfig, OAuthToken};
pub use store::{TokenStore, FileTokenStore};
pub use manager::OAuthManager;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_token_store() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store_path = temp_dir.path().join("tokens.json");
        let store = FileTokenStore::new(store_path);

        let token = OAuthToken {
            access_token: "access".to_string(),
            refresh_token: Some("refresh".to_string()),
            expires_at: None,
            scope: None,
        };

        store.save_token("test_provider", token.clone()).await.unwrap();
        
        let retrieved = store.get_token("test_provider").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().access_token, "access");
        
        store.delete_token("test_provider").await.unwrap();
        let retrieved = store.get_token("test_provider").await.unwrap();
        assert!(retrieved.is_none());
    }
}
