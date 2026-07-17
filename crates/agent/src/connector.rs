//! The connector interface — Phase 0, Task 2.
//!
//! Every app connector implements this, so the dispatcher and the Phase 4
//! robustness logic are written once and apply to all of them.

use crate::schema::ParsedAction;
use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;

/// The outcome of executing an action.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ExecutionResult {
    pub success: bool,
    /// Plain-language result for the user ("Sent to #general").
    pub message: String,
    /// Whether a retry might succeed — a timeout is retryable, a 400 is not.
    pub retryable: bool,
}

impl ExecutionResult {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            retryable: false,
        }
    }

    pub fn failed(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            success: false,
            message: message.into(),
            retryable,
        }
    }
}

/// One app's integration. Every connector looks identical to the dispatcher,
/// whatever tier it belongs to (cloud API, AppleScript, companion extension).
#[async_trait]
pub trait Connector: Send + Sync {
    /// The app this connector serves, matching `ParsedAction::app`.
    fn app(&self) -> &'static str;

    /// The actions it can execute, for routing and for telling the parser what
    /// is available.
    fn supported_actions(&self) -> Vec<&'static str>;

    /// Whether it is ready to execute (authenticated, reachable).
    async fn is_ready(&self) -> bool;

    /// Runs the pre-checked, user-approved action.
    async fn execute(&self, action: &ParsedAction) -> Result<ExecutionResult>;
}

/// Where OAuth tokens and API keys live. A stub interface for now — the real
/// store lands with the first connector that needs it (Phase 1, Spotify).
pub trait TokenStore: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn delete(&self, key: &str) -> Result<()>;
}
