use axum::{
    extract::State,
    response::sse::{Event, Sse},
    Json,
};
use futures::stream::Stream;
// use std::convert::Infallible;
use serde::{Deserialize, Serialize};
use crate::api::server::AppState;
use aimaxxing_engram::HybridSearchResult;
use tokio_stream::StreamExt as _; 

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum SearchEvent {
    #[serde(rename = "fast_result")]
    FastResult(Vec<HybridSearchResult>),
    #[serde(rename = "slow_result")]
    SlowResult(Vec<HybridSearchResult>),
    #[serde(rename = "done")]
    Done,
}

pub async fn search_handler(
    State(state): State<AppState>,
    Json(payload): Json<SearchRequest>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let query = payload.query;
    
    // Create a channel for sending events
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    
    let state_clone = state.clone();
    let query_clone = query.clone();
    let tx_clone = tx.clone();
    
    tokio::spawn(async move {
        // Fast Track (FTS)
        let fast_task = async {
            if let Ok(results) = state_clone.knowledge.search(&query_clone, 5) {
                if !results.is_empty() {
                    let _ = tx_clone.send(SearchEvent::FastResult(results)).await;
                }
            }
        };

        // Slow Track (Recursive)
        let slow_task = async {
             if let Some(retriever) = &state_clone.retriever {
                 // Note: search_recursive returns Result<Vec<HybridSearchResult>>
                 if let Ok(results) = retriever.search_recursive(&query_clone, 5).await {
                     if !results.is_empty() {
                         let _ = tx_clone.send(SearchEvent::SlowResult(results)).await;
                     }
                 }
             }
        };
        
        // Execute in parallel
        tokio::join!(fast_task, slow_task);
        
        let _ = tx_clone.send(SearchEvent::Done).await;
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx)
        .map(|event| {
            Event::default()
                .json_data(event)
                .map_err(|e| axum::Error::new(e))
        });

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}
