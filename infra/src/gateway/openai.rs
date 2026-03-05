//! OpenAI API Compatibility Layer
//!
//! Standardized endpoints for integrating AIMAXXING with 3rd party tools.

use axum::{
    extract::{State, Json},
    response::{IntoResponse, Response},
    http::{StatusCode, HeaderMap},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use tracing::info;

use super::state::GatewayState;
use brain::bus::message_bus::InboundMessage;

/// OpenAI Chat Completion Request
#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
}

/// OpenAI Chat Message
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// OpenAI Chat Completion Response
#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Serialize)]
pub struct Choice {
    pub index: usize,
    pub message: ChatMessage,
    pub finish_reason: String,
}

#[derive(Debug, Serialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<GatewayState>,
    headers: HeaderMap,
    Json(req): Json<ChatCompletionRequest>,
) -> Response {
    // 1. Authenticate manually from Authorization header
    if let Some(server_token) = state.get_auth_token() {
        let auth_header = headers.get("Authorization")
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default();
            
        let token = if auth_header.starts_with("Bearer ") {
            &auth_header[7..]
        } else {
            auth_header
        };

        if token != server_token {
            return (StatusCode::UNAUTHORIZED, "Invalid API token").into_response();
        }
    }

    // 2. Prepare request
    let request_id = Uuid::new_v4().to_string();
    let prompt = req.messages.last().map(|m| m.content.clone()).unwrap_or_default();
    
    // model name is used as the target agent name or channel
    let inbound = InboundMessage::new("openai", "api_user", &req.model, prompt)
        .with_request_id(&request_id);

    // 3. Register for response
    let rx = state.register_response(request_id.clone());

    // 4. Publish to bus
    if let Err(e) = state.bus.publish_inbound(inbound).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Bus error: {}", e)).into_response();
    }

    info!("OpenAI request {} forwarded to agent {}", request_id, req.model);

    // 5. Wait for response (with timeout)
    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(outbound)) => {
            let resp = ChatCompletionResponse {
                id: format!("chatcmpl-{}", request_id),
                object: "chat.completion".to_string(),
                created: Utc::now().timestamp(),
                model: req.model,
                choices: vec![Choice {
                    index: 0,
                    message: ChatMessage {
                        role: "assistant".to_string(),
                        content: outbound.content,
                    },
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 0, // Simplified
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            };
            Json(resp).into_response()
        }
        Ok(Err(_)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Response channel closed").into_response()
        }
        Err(_) => {
            // Cleanup pending response on timeout
            state.pending_responses.remove(&request_id);
            (StatusCode::GATEWAY_TIMEOUT, "Agent timed out").into_response()
        }
    }
}

/// GET /v1/models
pub async fn list_models() -> impl IntoResponse {
    #[derive(Serialize)]
    struct ModelList {
        object: String,
        data: Vec<Model>,
    }
    #[derive(Serialize)]
    struct Model {
        id: String,
        object: String,
        created: i64,
        owned_by: String,
    }

    let models = vec![
        Model {
            id: "aimaxxing-agent".to_string(),
            object: "model".to_string(),
            created: 1700000000,
            owned_by: "aimaxxing".to_string(),
        }
    ];

    Json(ModelList {
        object: "list".to_string(),
        data: models,
    })
}
