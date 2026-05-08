//! Agent harness — composition root owning cross-cutting dependencies.

use std::sync::Arc;

use crate::borderless::agent_core::LlmProvider;
use crate::borderless::telemetry::{MetricsCollector, Telemetry};
use crate::borderless::tools::{executor::ToolExecutor, registry::ToolRegistry, sandbox::Sandbox};

/// Composition root that owns all cross-cutting agent dependencies.
pub struct AgentHarness {
    pub llm: Arc<dyn LlmProvider>,
    pub tool_registry: ToolRegistry,
    pub tool_executor: ToolExecutor,
    pub sandbox: Sandbox,
    pub telemetry: Arc<Telemetry>,
    pub metrics: Arc<MetricsCollector>,
}
