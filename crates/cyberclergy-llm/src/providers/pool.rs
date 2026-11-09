//! The custom network as a rotating pool of OpenAI-compatible gateways.
//!
//! A person may point the application at several endpoints at once — a model
//! server on this machine, a proxy, a colleague's box. They all stand behind the
//! single `custom` network, so the roster and the journal keep one identity for
//! them, while the work is spread and a dead endpoint does not silence the row.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use crate::errors::LlmError;
use crate::provider::Provider;
use crate::types::{ChatRequest, ChatResponse, ProviderId};

/// Several endpoints answering as one provider.
pub struct CustomPool {
    endpoints: Vec<Arc<dyn Provider>>,
    /// Grows on every call, so consecutive prayers start at different gateways.
    cursor: AtomicUsize,
}

impl CustomPool {
    pub fn new(endpoints: Vec<Arc<dyn Provider>>) -> Self {
        Self {
            endpoints,
            cursor: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl Provider for CustomPool {
    fn id(&self) -> ProviderId {
        ProviderId::Custom
    }

    /// The first endpoint names the pool, because that is the gateway the
    /// person configured first and would expect to see.
    fn model(&self) -> &str {
        self.endpoints
            .first()
            .map(|endpoint| endpoint.model())
            .unwrap_or_default()
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        if self.endpoints.is_empty() {
            return Err(LlmError::NotConfigured(ProviderId::Custom));
        }
        let start = self.cursor.fetch_add(1, Ordering::Relaxed);
        let mut last_error = None;
        for offset in 0..self.endpoints.len() {
            let endpoint = &self.endpoints[(start + offset) % self.endpoints.len()];
            match endpoint.chat(request).await {
                Ok(response) => return Ok(response),
                // The last failure is the one the person most likely has to
                // fix, so it is the one reported.
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.expect("a non-empty pool always yields either a response or an error"))
    }
}
