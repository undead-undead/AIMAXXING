use std::sync::Arc;
use dashmap::DashMap;
use tokio::sync::oneshot;
use serde::{Serialize, Deserialize};
use async_trait::async_trait;
use brain::agent::core::ApprovalHandler;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApprovalInfo {
    pub id: String,
    pub tool_name: String,
    pub arguments: String,
}

pub struct PendingApproval {
    pub info: ApprovalInfo,
    pub responder: oneshot::Sender<bool>,
}

#[derive(Default)]
pub struct SecurityManager {
    pending: DashMap<String, PendingApproval>,
}

impl SecurityManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_request(&self, tool_name: &str, arguments: &str) -> oneshot::Receiver<bool> {
        let (tx, rx) = oneshot::channel();
        let id = Uuid::new_v4().to_string();
        
        let info = ApprovalInfo {
            id: id.clone(),
            tool_name: tool_name.to_string(),
            arguments: arguments.to_string(),
        };
        
        self.pending.insert(id, PendingApproval {
            info,
            responder: tx,
        });
        
        rx
    }

    pub fn list_pending(&self) -> Vec<ApprovalInfo> {
        self.pending.iter().map(|item| item.value().info.clone()).collect()
    }

    pub fn resolve(&self, id: &str, approved: bool) -> bool {
        if let Some((_, pending)) = self.pending.remove(id) {
            let _ = pending.responder.send(approved);
            true
        } else {
            false
        }
    }
}

pub struct GatewayApprovalHandler {
    manager: Arc<SecurityManager>,
}

impl GatewayApprovalHandler {
    pub fn new(manager: Arc<SecurityManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ApprovalHandler for GatewayApprovalHandler {
    async fn approve(&self, tool_name: &str, arguments: &str) -> anyhow::Result<bool> {
        let rx = self.manager.add_request(tool_name, arguments);
        // Wait for user to resolve from frontend
        match rx.await {
            Ok(approved) => Ok(approved),
            Err(_) => Ok(false), // Cancelled or dropped
        }
    }
}
