//! Aggregate per-turn / per-tool / per-error counters.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

/// Metrics for a single agent turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnMetrics {
    pub turn_number: u32,
    pub had_tool_calls: bool,
    pub tool_call_count: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
    pub timestamp: i64,
}

/// Aggregate metrics for a single tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetrics {
    pub name: String,
    pub call_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub total_duration_ms: u64,
    pub avg_duration_ms: f64,
}

/// Full metrics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetricsSnapshot {
    pub turn_count: u32,
    pub turns: Vec<TurnMetrics>,
    pub tool_metrics: HashMap<String, ToolMetrics>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost: f64,
    pub error_count: u32,
    pub errors_by_type: HashMap<String, u32>,
}

/// Thread-safe metrics collector.
pub struct MetricsCollector {
    inner: RwLock<MetricsInner>,
}

struct MetricsInner {
    turns: Vec<TurnMetrics>,
    tool_metrics: HashMap<String, ToolMetrics>,
    error_count: u32,
    errors_by_type: HashMap<String, u32>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(MetricsInner {
                turns: Vec::new(),
                tool_metrics: HashMap::new(),
                error_count: 0,
                errors_by_type: HashMap::new(),
            }),
        }
    }

    pub fn record_turn(&self, turn: TurnMetrics) {
        self.inner.write().unwrap().turns.push(turn);
    }

    pub fn record_tool_call(&self, name: &str, duration_ms: u64, success: bool) {
        let mut inner = self.inner.write().unwrap();
        let entry = inner.tool_metrics.entry(name.to_string()).or_insert_with(|| ToolMetrics {
            name: name.to_string(),
            call_count: 0,
            success_count: 0,
            failure_count: 0,
            total_duration_ms: 0,
            avg_duration_ms: 0.0,
        });
        entry.call_count += 1;
        entry.total_duration_ms += duration_ms;
        entry.avg_duration_ms = entry.total_duration_ms as f64 / entry.call_count as f64;
        if success {
            entry.success_count += 1;
        } else {
            entry.failure_count += 1;
        }
    }

    pub fn record_error(&self, error_type: &str) {
        let mut inner = self.inner.write().unwrap();
        inner.error_count += 1;
        *inner.errors_by_type.entry(error_type.to_string()).or_insert(0) += 1;
    }

    pub fn get_metrics(&self) -> AgentMetricsSnapshot {
        let inner = self.inner.read().unwrap();
        let mut total_input = 0u64;
        let mut total_output = 0u64;
        let mut total_cost = 0.0f64;
        for t in &inner.turns {
            total_input += t.input_tokens;
            total_output += t.output_tokens;
            total_cost += t.estimated_cost.unwrap_or(0.0);
        }
        AgentMetricsSnapshot {
            turn_count: inner.turns.len() as u32,
            turns: inner.turns.clone(),
            tool_metrics: inner.tool_metrics.clone(),
            total_input_tokens: total_input,
            total_output_tokens: total_output,
            total_cost,
            error_count: inner.error_count,
            errors_by_type: inner.errors_by_type.clone(),
        }
    }

    pub fn reset(&self) {
        let mut inner = self.inner.write().unwrap();
        inner.turns.clear();
        inner.tool_metrics.clear();
        inner.error_count = 0;
        inner.errors_by_type.clear();
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector() {
        let collector = MetricsCollector::new();

        collector.record_turn(TurnMetrics {
            turn_number: 1,
            had_tool_calls: true,
            tool_call_count: 2,
            input_tokens: 100,
            output_tokens: 50,
            duration_ms: 1000,
            estimated_cost: Some(0.01),
            timestamp: 0,
        });

        collector.record_tool_call("bash", 500, true);
        collector.record_tool_call("bash", 300, false);
        collector.record_error("TOOL_TIMEOUT");

        let snapshot = collector.get_metrics();
        assert_eq!(snapshot.turn_count, 1);
        assert_eq!(snapshot.total_input_tokens, 100);
        assert_eq!(snapshot.total_output_tokens, 50);
        assert_eq!(snapshot.error_count, 1);

        let bash = &snapshot.tool_metrics["bash"];
        assert_eq!(bash.call_count, 2);
        assert_eq!(bash.success_count, 1);
        assert_eq!(bash.failure_count, 1);
    }
}
