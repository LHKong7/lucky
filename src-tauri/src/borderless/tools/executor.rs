//! Tool execution engine with parallel + serialized batching.

use std::time::{Duration, Instant};

use crate::borderless::agent_core::{ToolCall, ToolError};

use super::registry::ToolRegistry;

/// Maximum per-tool timeout (10 minutes).
const MAX_TIMEOUT: Duration = Duration::from_secs(600);
/// Default per-tool timeout (60 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// A request to execute a tool.
#[derive(Debug, Clone)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

impl From<ToolCall> for ToolCallRequest {
    fn from(tc: ToolCall) -> Self {
        Self {
            id: tc.id,
            name: tc.name,
            arguments: tc.arguments,
        }
    }
}

/// Result of a tool call execution.
#[derive(Debug, Clone)]
pub struct ToolCallResult {
    pub id: String,
    pub name: String,
    pub output: String,
    pub success: bool,
    pub duration_ms: u64,
    pub error: Option<String>,
}

/// Approval callback type.
pub type ApprovalCallback =
    Box<dyn Fn(&str, &serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>> + Send + Sync>;

/// Tool executor that handles parallel and serialized tool execution.
pub struct ToolExecutor {
    approval_callback: Option<ApprovalCallback>,
}

impl ToolExecutor {
    pub fn new(approval_callback: Option<ApprovalCallback>) -> Self {
        Self { approval_callback }
    }

    /// Execute all tool calls, respecting parallelism and serialization rules.
    pub async fn execute_all(
        &self,
        calls: Vec<ToolCallRequest>,
        registry: &ToolRegistry,
    ) -> Vec<ToolCallResult> {
        if calls.is_empty() {
            return Vec::new();
        }

        // Classify into parallel and serialized groups
        let mut parallel = Vec::new();
        let mut serialized = Vec::new();

        for call in &calls {
            if let Some(tool) = registry.get(&call.name) {
                if tool.requires_approval || !tool.concurrency_safe {
                    serialized.push(call);
                } else {
                    parallel.push(call);
                }
            } else {
                serialized.push(call); // Unknown tools are serialized for safety
            }
        }

        let mut results: Vec<Option<ToolCallResult>> = vec![None; calls.len()];
        let call_index: std::collections::HashMap<String, usize> = calls
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.clone(), i))
            .collect();

        // Execute parallel group using JoinSet
        if !parallel.is_empty() {
            let mut join_set: tokio::task::JoinSet<ToolCallResult> = tokio::task::JoinSet::new();

            for call in parallel {
                let call = call.clone();
                let tool_opt = registry.get(&call.name);
                let timeout = tool_opt
                    .and_then(|t| t.timeout)
                    .unwrap_or(DEFAULT_TIMEOUT)
                    .min(MAX_TIMEOUT);

                // We need to actually call the execute function
                if let Some(tool) = tool_opt {
                    let execute = &tool.execute;
                    let args = call.arguments.clone();
                    let id = call.id.clone();
                    let name = call.name.clone();

                    // Since ToolExecuteFn is not Clone, we need to handle this differently.
                    // Execute in the current task context instead.
                    let start = Instant::now();
                    let result = tokio::time::timeout(timeout, (execute)(args)).await;
                    let duration = start.elapsed();

                    let tool_result = match result {
                        Ok(Ok(output)) => ToolCallResult {
                            id: id.clone(),
                            name: name.clone(),
                            output,
                            success: true,
                            duration_ms: duration.as_millis() as u64,
                            error: None,
                        },
                        Ok(Err(e)) => ToolCallResult {
                            id: id.clone(),
                            name: name.clone(),
                            output: format!("Error: {}", e),
                            success: false,
                            duration_ms: duration.as_millis() as u64,
                            error: Some(e.to_string()),
                        },
                        Err(_) => ToolCallResult {
                            id: id.clone(),
                            name: name.clone(),
                            output: format!("Tool '{}' timed out after {:?}", name, timeout),
                            success: false,
                            duration_ms: duration.as_millis() as u64,
                            error: Some(format!("Timeout after {:?}", timeout)),
                        },
                    };

                    if let Some(&idx) = call_index.get(&tool_result.id) {
                        results[idx] = Some(tool_result);
                    }
                }

                // For spawned tasks (when tool closures are Send + Sync + 'static):
                // join_set.spawn(async move { ... });
            }

            // Collect spawned task results
            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok(tool_result) => {
                        if let Some(&idx) = call_index.get(&tool_result.id) {
                            results[idx] = Some(tool_result);
                        }
                    }
                    Err(e) => {
                        eprintln!("Tool task panicked: {}", e);
                    }
                }
            }
        }

        // Execute serialized group sequentially
        for call in serialized {
            let tool_result = self.execute_one(call, registry).await;
            if let Some(&idx) = call_index.get(&tool_result.id) {
                results[idx] = Some(tool_result);
            }
        }

        // Fill any missing results
        results
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                r.unwrap_or_else(|| ToolCallResult {
                    id: calls[i].id.clone(),
                    name: calls[i].name.clone(),
                    output: "Tool execution failed: internal error".into(),
                    success: false,
                    duration_ms: 0,
                    error: Some("Missing result".into()),
                })
            })
            .collect()
    }

    /// Execute a single tool call.
    async fn execute_one(&self, call: &ToolCallRequest, registry: &ToolRegistry) -> ToolCallResult {
        let tool = match registry.get(&call.name) {
            Some(t) => t,
            None => {
                return ToolCallResult {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    output: format!("Tool '{}' not found", call.name),
                    success: false,
                    duration_ms: 0,
                    error: Some(ToolError::NotFound { name: call.name.clone() }.to_string()),
                };
            }
        };

        // Check approval if needed
        if tool.requires_approval {
            if let Some(ref callback) = self.approval_callback {
                let approved = callback(&call.name, &call.arguments).await;
                if !approved {
                    return ToolCallResult {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        output: format!("Tool '{}' was denied by user", call.name),
                        success: false,
                        duration_ms: 0,
                        error: Some(ToolError::UserDenied { name: call.name.clone() }.to_string()),
                    };
                }
            }
        }

        let timeout = tool
            .timeout
            .unwrap_or(DEFAULT_TIMEOUT)
            .min(MAX_TIMEOUT);

        let start = Instant::now();
        let result = tokio::time::timeout(timeout, (tool.execute)(call.arguments.clone())).await;
        let duration = start.elapsed();

        match result {
            Ok(Ok(output)) => ToolCallResult {
                id: call.id.clone(),
                name: call.name.clone(),
                output,
                success: true,
                duration_ms: duration.as_millis() as u64,
                error: None,
            },
            Ok(Err(e)) => ToolCallResult {
                id: call.id.clone(),
                name: call.name.clone(),
                output: format!("Error: {}", e),
                success: false,
                duration_ms: duration.as_millis() as u64,
                error: Some(e.to_string()),
            },
            Err(_) => ToolCallResult {
                id: call.id.clone(),
                name: call.name.clone(),
                output: format!("Tool '{}' timed out after {:?}", call.name, timeout),
                success: false,
                duration_ms: duration.as_millis() as u64,
                error: Some(format!("Timeout after {:?}", timeout)),
            },
        }
    }
}
