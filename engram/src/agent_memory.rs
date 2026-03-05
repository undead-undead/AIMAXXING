use crate::hybrid_search::HybridSearchEngine;
#[cfg(feature = "vector")]
use crate::quant::QuantLevel;
use aimaxxing_core::agent::memory::Memory;
use aimaxxing_core::agent::message::Message;
use aimaxxing_core::agent::session::AgentSession;
use aimaxxing_core::knowledge::rag::Document;
use async_trait::async_trait;
use std::sync::Arc;

/// Adapter to use HybridSearchEngine as an AIMAXXING Memory backend
pub struct EngramMemory {
    engine: Arc<HybridSearchEngine>,
}

impl EngramMemory {
    pub fn new(engine: Arc<HybridSearchEngine>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl Memory for EngramMemory {
    async fn store(&self, _user_id: &str, _agent_id: Option<&str>, _message: Message) -> aimaxxing_core::error::Result<()> {
        // Conversation history stored via sessions, not individual messages
        Ok(())
    }

    async fn store_knowledge(&self, _user_id: &str, _agent_id: Option<&str>, title: &str, content: &str, collection: &str, unverified: bool) -> aimaxxing_core::error::Result<()> {
        let path = format!("manual/{}", uuid::Uuid::new_v4());
        
        #[cfg(feature = "vector")]
        {
            // Differentiated Quantization: Use Full (FP32) for Soul/Persona, Warm for others
            let level = match collection.to_lowercase().as_str() {
                "soul" | "persona" | "core" | "identity" => QuantLevel::Full,
                _ => QuantLevel::Warm,
            };
            
            self.engine.index_at_level(collection, &path, title, content, level, unverified)
                .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        }
        #[cfg(not(feature = "vector"))]
        {
            self.engine.index_document(collection, &path, title, content, unverified)
                .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        }
        
        Ok(())
    }

    async fn retrieve(&self, _user_id: &str, _agent_id: Option<&str>, _limit: usize) -> Vec<Message> {
        Vec::new()
    }

    async fn search(&self, _user_id: &str, _agent_id: Option<&str>, query: &str, limit: usize) -> aimaxxing_core::error::Result<Vec<Document>> {
        let results = self.engine.search(query, limit)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;

        let docs = results.into_iter().map(|r| {
            let mut doc = Document {
                id: r.document.docid.clone(),
                title: r.document.title.clone(),
                content: r.document.body.clone().unwrap_or_default(),
                summary: r.document.summary.clone(),
                collection: Some(r.document.collection.clone()),
                path: Some(r.document.path.clone()),
                metadata: std::collections::HashMap::new(),
                score: r.rrf_score as f32, // Use RRF score for hybrid search
            };
            
            // We attach the source metadata to the document so it can be extracted if used in a message
            doc.metadata.insert("source_collection".to_string(), r.document.collection);
            doc.metadata.insert("source_path".to_string(), r.document.path);
            
            doc
        }).collect();

        Ok(docs)
    }

    async fn store_session(&self, session: AgentSession) -> aimaxxing_core::error::Result<()> {
        let data = serde_json::to_string(&session)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        self.engine.engram_store().store_session(&session.id, &data)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        Ok(())
    }

    async fn retrieve_session(&self, session_id: &str) -> aimaxxing_core::error::Result<Option<AgentSession>> {
        let data = self.engine.engram_store().get_session(session_id)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        if let Some(json) = data {
            let session = serde_json::from_str(&json)
                .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
            Ok(Some(session))
        } else {
            Ok(None)
        }
    }

    async fn fetch_document(&self, collection: &str, path: &str) -> aimaxxing_core::error::Result<Option<Document>> {
        let doc = self.engine.engram_store().get_by_path(collection, path)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        Ok(doc.map(|d| Document {
            id: d.docid,
            title: d.title,
            content: d.body.unwrap_or_default(),
            summary: d.summary,
            collection: Some(d.collection),
            path: Some(d.path),
            metadata: std::collections::HashMap::new(),
            score: 1.0,
        }))
    }

    async fn clear(&self, _user_id: &str, _agent_id: Option<&str>) -> aimaxxing_core::error::Result<()> {
        Ok(())
    }

    async fn undo(&self, _user_id: &str, _agent_id: Option<&str>) -> aimaxxing_core::error::Result<Option<Message>> {
        Ok(None)
    }

    async fn list_unverified(&self, limit: usize) -> aimaxxing_core::error::Result<Vec<Message>> {
        let docs = self.engine.list_unverified(limit)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        
        let messages = docs.into_iter().map(|d| {
            let mut msg = Message::assistant(format!("[{}]: {}", d.title, d.body.unwrap_or_default()));
            msg.unverified = true;
            msg
        }).collect();
        
        Ok(messages)
    }

    async fn mark_verified(&self, entry_content: &str) -> aimaxxing_core::error::Result<()> {
        // Broad search to find the document by content to mark it verified
        // Note: This is heuristic-based because the Memory trait uses content string as ID
        let docs = self.engine.list_unverified(100)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        
        for doc in docs {
            let doc_text = format!("[{}]: {}", doc.title, doc.body.as_ref().map(|s| s.as_str()).unwrap_or_default());
            if doc_text == entry_content {
                self.engine.mark_verified(&doc.collection, &doc.path)
                    .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
                
                // Phase 14: Verification reward
                let _ = self.engine.engram_store().update_utility(&doc.docid, 0.2);
                
                return Ok(());
            }
        }
        Ok(())
    }

    async fn mark_pruned(&self, entry_content: &str) -> aimaxxing_core::error::Result<()> {
        let docs = self.engine.list_unverified(100)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        
        for doc in docs {
            let doc_text = format!("[{}]: {}", doc.title, doc.body.as_ref().map(|s| s.as_str()).unwrap_or_default());
            if doc_text == entry_content {
                self.engine.delete_document(&doc.collection, &doc.path)
                    .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
                return Ok(());
            }
        }
        Ok(())
    }

    async fn maintenance(&self) -> aimaxxing_core::error::Result<()> {
        self.engine.engram_store().delete_stale_sessions(7)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        Ok(())
    }

    async fn update_utility(&self, collection: &str, path: &str, increment: f32) -> aimaxxing_core::error::Result<()> {
        let doc = self.engine.engram_store().get_by_path(collection, path)
            .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        
        if let Some(d) = doc {
            self.engine.engram_store().update_utility(&d.docid, increment)
                .map_err(|e| aimaxxing_core::error::Error::Internal(e.to_string()))?;
        }
        Ok(())
    }
}
