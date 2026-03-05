use brain::error::{Error, Result};
use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl,
    TokenResponse, AuthorizationCode, CsrfToken, PkceCodeChallenge, PkceCodeVerifier,
    EndpointSet, EndpointNotSet,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::types::{OAuthConfig, OAuthToken};
use crate::store::TokenStore;

/// Manages OAuth flows for multiple providers
pub struct OAuthManager {
    clients: HashMap<String, BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>>,
    store: Arc<dyn TokenStore>,
    // Store CSRF tokens and PKCE verifiers temporarily (in-memory)
    // Key: csrf_token
    pending_requests: Arc<Mutex<HashMap<String, PendingRequest>>>,
}

struct PendingRequest {
    provider: String,
    verifier: PkceCodeVerifier,
    _redirect_url: String, // To verify callback matches
}

impl OAuthManager {
    pub fn new(store: Arc<dyn TokenStore>) -> Self {
        Self {
            clients: HashMap::new(),
            store,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Register a provider configuration
    pub fn register_provider(&mut self, provider_name: &str, config: OAuthConfig) -> Result<()> {
        let client_id = ClientId::new(config.client_id);
        let auth_url = AuthUrl::new(config.auth_url)
            .map_err(|e| Error::config(format!("Invalid auth URL: {}", e)))?;
        let token_url = TokenUrl::new(config.token_url)
            .map_err(|e| Error::config(format!("Invalid token URL: {}", e)))?;
            
        let client = BasicClient::new(client_id)
            .set_client_secret(ClientSecret::new(config.client_secret))
            .set_auth_uri(auth_url)
            .set_token_uri(token_url)
            .set_redirect_uri(RedirectUrl::new(config.redirect_url).map_err(|e| Error::config(format!("Invalid redirect URL: {}", e)))?);
        
        self.clients.insert(provider_name.to_string(), client);
        Ok(())
    }
    
    /// Generate authorization URL for a provider
    pub fn initiate_auth(&self, provider_name: &str) -> Result<(String, String)> {
        let client = self.clients.get(provider_name)
            .ok_or_else(|| Error::config(format!("Provider '{}' not configured", provider_name)))?;

        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, csrf_token) = client
            .clone()
            .authorize_url(CsrfToken::new_random)
            .set_pkce_challenge(pkce_challenge)
            .url();

        // Store verifier
        let mut pending = self.pending_requests.lock().map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
        pending.insert(csrf_token.secret().clone(), PendingRequest {
            provider: provider_name.to_string(),
            verifier: pkce_verifier,
            _redirect_url: client.redirect_uri().map(|u| u.to_string()).unwrap_or_default(),
        });

        Ok((auth_url.to_string(), csrf_token.secret().clone()))
    }
    
    /// Handle callback and exchange code for token
    pub async fn handle_callback(&self, code: String, state: String) -> Result<OAuthToken> {
        // Retrieve verifier
        let pending_req = {
            let mut pending = self.pending_requests.lock().map_err(|_| Error::Internal("Lock poisoned".to_string()))?;
            pending.remove(&state)
                .ok_or_else(|| Error::auth("Invalid or expired CSRF token"))?
        };
        
        let client = self.clients.get(&pending_req.provider)
            .ok_or_else(|| Error::Internal(format!("Provider '{}' lost from config", pending_req.provider)))?;
            
        // Exchange code
        let http_client = reqwest::Client::new();
        let token_result = client
            .exchange_code(AuthorizationCode::new(code))
            .set_pkce_verifier(pending_req.verifier)
            .request_async(&http_client)
            .await
            .map_err(|e| Error::auth(format!("Token exchange failed: {}", e)))?;
            
        // Convert to our token type
        let access_token = token_result.access_token().secret().clone();
        let refresh_token = token_result.refresh_token().map(|t| t.secret().clone());
        let expires_at = token_result.expires_in().map(|d| chrono::Utc::now() + d);
        
        let token = OAuthToken {
            access_token,
            refresh_token,
            expires_at,
            scope: None,
        };
        
        // Save token
        self.store.save_token(&pending_req.provider, token.clone()).await?;
        
        info!("Successfully authenticated with provider: {}", pending_req.provider);
        
        Ok(token)
    }
    
    /// Get a valid access token (refresh if needed)
    pub async fn get_access_token(&self, provider_name: &str) -> Result<String> {
        let mut token = self.store.get_token(provider_name).await?
            .ok_or_else(|| Error::auth(format!("No token found for '{}'. Please authenticate first.", provider_name)))?;
            
        // Check expiration
        if let Some(expires_at) = token.expires_at {
             // Add buffer of 60 seconds
             if chrono::Utc::now() + chrono::Duration::seconds(60) > expires_at {
                 info!("Token for '{}' expired or expiring soon. Refreshing...", provider_name);
                 
                 // Refresh
                 if let Some(refresh_token) = &token.refresh_token {
                     let client = self.clients.get(provider_name)
                        .ok_or_else(|| Error::config(format!("Provider '{}' not configured", provider_name)))?;
                        
                     let http_client = reqwest::Client::new();
                     let new_token = client
                        .exchange_refresh_token(&oauth2::RefreshToken::new(refresh_token.clone()))
                        .request_async(&http_client)
                        .await
                        .map_err(|e| Error::auth(format!("Token refresh failed: {}", e)))?;
                        
                     token.access_token = new_token.access_token().secret().clone();
                     if let Some(rt) = new_token.refresh_token() {
                         token.refresh_token = Some(rt.secret().clone());
                     }
                     if let Some(exp) = new_token.expires_in() {
                         token.expires_at = Some(chrono::Utc::now() + exp);
                     }
                     
                     // Save updated token
                     self.store.save_token(provider_name, token.clone()).await?;
                 } else {
                     return Err(Error::auth(format!("Token expired and no refresh token available for '{}'", provider_name)));
                 }
             }
        }
        
        Ok(token.access_token)
    }
}
