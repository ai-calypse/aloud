//! The dispatcher — Phase 1 Task 6, with the robustness pieces from Phase 4.
//!
//! Routes an approved action to its connector, retries transient failures, and
//! turns an unsupported action into an honest message rather than a dead end.

use crate::connector::{Connector, ExecutionResult};
use crate::schema::ParsedAction;
use std::{collections::HashMap, sync::Arc, time::Duration};

/// Transient failures get this many attempts total (Task 16).
const MAX_ATTEMPTS: u32 = 3;
const BACKOFF_BASE: Duration = Duration::from_millis(400);

pub struct Dispatcher {
    connectors: HashMap<&'static str, Arc<dyn Connector>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            connectors: HashMap::new(),
        }
    }

    pub fn register(&mut self, connector: Arc<dyn Connector>) {
        self.connectors.insert(connector.app(), connector);
    }

    /// Executes an approved action, retrying transient failures.
    ///
    /// The action is assumed already parsed, validated, and user-approved —
    /// confirmation is the caller's job (Task 5), not the dispatcher's.
    pub async fn dispatch(&self, action: &ParsedAction) -> ExecutionResult {
        let Some(connector) = self.connectors.get(action.app.as_str()) else {
            // Task 17: a real gap, reported honestly and logged as product
            // feedback rather than surfaced as a generic failure.
            tracing::info!(action = %action.action, "no connector; unsupported action");
            return ExecutionResult::failed(
                format!("I don't have a {} connector for that yet.", action.app),
                false,
            );
        };

        if !connector.is_ready().await {
            return ExecutionResult::failed(
                format!(
                    "{} isn't connected yet — set it up in Settings.",
                    action.app
                ),
                false,
            );
        }

        self.execute_with_retry(connector.as_ref(), action).await
    }

    /// Retry-with-backoff for transient failures only (Task 16). A
    /// non-retryable failure (bad request, missing entity) returns at once.
    async fn execute_with_retry(
        &self,
        connector: &dyn Connector,
        action: &ParsedAction,
    ) -> ExecutionResult {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match connector.execute(action).await {
                Ok(result) if result.success => return result,
                Ok(result) if !result.retryable || attempt >= MAX_ATTEMPTS => return result,
                Err(e) if attempt >= MAX_ATTEMPTS => {
                    return ExecutionResult::failed(format!("{e:#}"), false);
                }
                outcome => {
                    tracing::warn!(?outcome, attempt, "transient failure; retrying");
                    // Exponential backoff: 400ms, 800ms, ...
                    let delay = BACKOFF_BASE * 2u32.pow(attempt - 1);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    pub fn is_supported(&self, action: &ParsedAction) -> bool {
        self.connectors.contains_key(action.app.as_str())
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs a chained action's steps in order, stopping at the first failure
/// (Phase 4, Task 14). No rollback: a sent message cannot be unsent, so the
/// partial state is reported instead.
pub async fn dispatch_chain(
    dispatcher: &Dispatcher,
    actions: &[ParsedAction],
) -> Vec<ExecutionResult> {
    let mut results = Vec::with_capacity(actions.len());
    for action in actions {
        let result = dispatcher.dispatch(action).await;
        let failed = !result.success;
        results.push(result);
        if failed {
            // Stop here: later steps may depend on this one, and continuing
            // could compound the mistake.
            break;
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connector::Connector;
    use anyhow::Result;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex;

    /// A connector whose result is scripted, counting how often it ran.
    struct MockConnector {
        app: &'static str,
        ready: bool,
        calls: AtomicU32,
        /// Result per call; the last is repeated once exhausted.
        results: Mutex<Vec<ExecutionResult>>,
    }

    impl MockConnector {
        fn new(app: &'static str, results: Vec<ExecutionResult>) -> Arc<Self> {
            Arc::new(Self {
                app,
                ready: true,
                calls: AtomicU32::new(0),
                results: Mutex::new(results),
            })
        }
    }

    #[async_trait]
    impl Connector for MockConnector {
        fn app(&self) -> &'static str {
            self.app
        }
        fn supported_actions(&self) -> Vec<&'static str> {
            vec![]
        }
        async fn is_ready(&self) -> bool {
            self.ready
        }
        async fn execute(&self, _action: &ParsedAction) -> Result<ExecutionResult> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) as usize;
            let results = self.results.lock().unwrap();
            Ok(results.get(n).or_else(|| results.last()).unwrap().clone())
        }
    }

    fn action(app: &str) -> ParsedAction {
        ParsedAction {
            action: format!("{app}.do"),
            app: app.into(),
            entities: Default::default(),
            confidence: 1.0,
            requires_confirmation: false,
        }
    }

    #[tokio::test(start_paused = true)]
    async fn retries_transient_failures_then_succeeds() {
        let connector = MockConnector::new(
            "spotify",
            vec![
                ExecutionResult::failed("timeout", true),
                ExecutionResult::ok("playing"),
            ],
        );
        let mut d = Dispatcher::new();
        d.register(connector.clone());

        let result = d.dispatch(&action("spotify")).await;
        assert!(result.success, "should succeed on the retry");
        assert_eq!(connector.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn does_not_retry_a_permanent_failure() {
        let connector =
            MockConnector::new("slack", vec![ExecutionResult::failed("bad request", false)]);
        let mut d = Dispatcher::new();
        d.register(connector.clone());

        let result = d.dispatch(&action("slack")).await;
        assert!(!result.success);
        assert_eq!(
            connector.calls.load(Ordering::SeqCst),
            1,
            "no retry on a 4xx"
        );
    }

    #[tokio::test]
    async fn unsupported_action_is_honest_not_generic() {
        let d = Dispatcher::new();
        let result = d.dispatch(&action("figma")).await;
        assert!(!result.success);
        assert!(
            result.message.contains("figma connector"),
            "got: {}",
            result.message
        );
    }

    #[tokio::test]
    async fn chain_stops_at_the_first_failure() {
        let mut d = Dispatcher::new();
        d.register(MockConnector::new(
            "github",
            vec![ExecutionResult::ok("issue #1")],
        ));
        d.register(MockConnector::new(
            "slack",
            vec![ExecutionResult::failed("nope", false)],
        ));

        let results =
            dispatch_chain(&d, &[action("github"), action("slack"), action("github")]).await;
        // github ok, slack fails, third step never runs.
        assert_eq!(results.len(), 2);
        assert!(results[0].success);
        assert!(!results[1].success);
    }
}
