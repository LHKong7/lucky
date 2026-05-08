//! Core public types for the borderless-agent SDK.
//!
//! Users import these to define tools, skills, and configure agents.

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Permission level
// ---------------------------------------------------------------------------

/// Permission level for sandbox classification of tool operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    /// Read-only operations (ls, cat, git status).
    Safe,
    /// File modifications.
    Moderate,
    /// Command execution.
    Dangerous,
    /// Unrestricted.
    Critical,
}

impl Default for PermissionLevel {
    fn default() -> Self {
        Self::Safe
    }
}

// ---------------------------------------------------------------------------
// Tool definition
// ---------------------------------------------------------------------------

/// The async function signature for a tool's execute handler.
pub type ToolExecuteFn = Box<
    dyn Fn(serde_json::Value) -> Pin<Box<dyn Future<Output = Result<String, super::ToolError>> + Send>>
        + Send
        + Sync,
>;

/// JSON-Schema-style parameter descriptor for a tool parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDef {
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

/// A tool the agent can call. Users provide `execute` — the runtime handler.
pub struct ToolDefinition {
    /// Unique tool name (used by the LLM to invoke it).
    pub name: String,
    /// Human-readable description shown to the LLM.
    pub description: String,
    /// JSON-Schema-style parameter map.
    pub parameters: Option<std::collections::HashMap<String, ParameterDef>>,
    /// Names of required parameters.
    pub required: Vec<String>,
    /// Runtime handler. Receives parsed arguments, returns a string result.
    pub execute: ToolExecuteFn,
    /// If true, requires user approval before execution.
    pub requires_approval: bool,
    /// Permission level for sandbox classification.
    pub permission_level: PermissionLevel,
    /// Per-tool execution timeout. Falls back to executor default (60s).
    pub timeout: Option<std::time::Duration>,
    /// Whether this tool can be safely executed in parallel. Default: true.
    pub concurrency_safe: bool,
}

impl std::fmt::Debug for ToolDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolDefinition")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("requires_approval", &self.requires_approval)
            .field("permission_level", &self.permission_level)
            .field("concurrency_safe", &self.concurrency_safe)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Skill definition
// ---------------------------------------------------------------------------

/// Async lifecycle hook for skills.
pub type SkillHookFn = Box<
    dyn Fn(SkillContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
>;

/// A skill that can be loaded by the agent on demand.
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    /// Markdown body injected into context when the skill is loaded.
    pub body: String,
    /// Semantic version. Default: "1.0.0".
    pub version: String,
    /// Free-form tags for search and filtering.
    pub tags: Vec<String>,
    /// Logical categories for grouping.
    pub categories: Vec<String>,
    /// Names of skills this skill depends on. Auto-loaded transitively.
    pub dependencies: Vec<String>,
    /// Auto-trigger pattern.
    pub trigger: Option<SkillTrigger>,
    /// Few-shot examples.
    pub examples: Vec<SkillExample>,
    /// Hook fired when the skill is first loaded into a session.
    pub on_load: Option<SkillHookFn>,
    /// Hook fired when the skill is unloaded.
    pub on_unload: Option<SkillHookFn>,
}

impl std::fmt::Debug for SkillDefinition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SkillDefinition")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("version", &self.version)
            .field("tags", &self.tags)
            .field("categories", &self.categories)
            .field("dependencies", &self.dependencies)
            .finish_non_exhaustive()
    }
}

/// Trigger pattern for auto-loading skills.
#[derive(Debug, Clone)]
pub enum SkillTrigger {
    /// Match if user input contains this substring.
    Substring(String),
    /// Match if user input matches this regex.
    Regex(regex::Regex),
}

/// A few-shot example for a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExample {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input: String,
    pub output: String,
}

/// Context passed to skill lifecycle hooks.
#[derive(Debug, Clone)]
pub struct SkillContext {
    pub session_id: Option<String>,
    /// Free-form scratch area shared between on_load / on_unload calls.
    pub scratch: serde_json::Map<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Chat messages
// ---------------------------------------------------------------------------

/// Content of a user message — either plain text or multimodal parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

/// A single part of a multimodal message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A tool call issued by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallMsg {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// A chat message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum ChatMessage {
    #[serde(rename = "system")]
    System { content: String },
    #[serde(rename = "user")]
    User { content: MessageContent },
    #[serde(rename = "assistant")]
    Assistant {
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCallMsg>,
        #[serde(skip_serializing_if = "Option::is_none")]
        thinking: Option<String>,
    },
    #[serde(rename = "tool")]
    Tool {
        tool_call_id: String,
        content: String,
    },
}

impl ChatMessage {
    /// Create a system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
        }
    }

    /// Create a user message with plain text.
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            content: MessageContent::Text(content.into()),
        }
    }

    /// Create an assistant message with plain text.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::Assistant {
            content: Some(content.into()),
            tool_calls: Vec::new(),
            thinking: None,
        }
    }

    /// Create a tool result message.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::Tool {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// LLM types
// ---------------------------------------------------------------------------

/// A parsed tool call from the LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Normalized response from any LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub usage: std::collections::HashMap<String, u64>,
    pub model: String,
    /// Extended thinking / reasoning content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
}

/// Options for LLM chat calls.
#[derive(Debug, Clone, Default)]
pub struct ChatOptions {
    /// Tool definitions in OpenAI function-calling format.
    pub tools: Option<serde_json::Value>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
}

// ---------------------------------------------------------------------------
// Chat result types
// ---------------------------------------------------------------------------

/// Result of a single agent chat turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResult {
    /// Final assistant text.
    pub reply: String,
    /// Full updated message history.
    pub history: Vec<ChatMessage>,
    /// Whether tools were called during this turn.
    pub had_tool_calls: bool,
    /// Session ID (if session is active).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Token usage for this turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<super::pricing::TokenUsage>,
    /// Estimated cost in USD for this turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
}

/// A chunk of a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Content delta (partial text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    /// Final reply. Present on the last chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// Whether this is the final chunk.
    pub done: bool,
    /// Token usage (present on the final chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<super::pricing::TokenUsage>,
    /// Estimated cost in USD (present on the final chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<f64>,
}

// ---------------------------------------------------------------------------
// Autonomous task types
// ---------------------------------------------------------------------------

/// Phase within a single iteration of the autonomous loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutonomousPhase {
    Plan,
    Execute,
    Review,
    Evaluate,
}

/// Configuration for `agent.run_task()`.
#[derive(Debug, Clone)]
pub struct AutonomousTaskConfig {
    /// Task description from the user.
    pub task: String,
    /// Quality threshold (1-10). Default: 7.
    pub quality_threshold: u8,
    /// Maximum outer-loop iterations. Default: 10.
    pub max_iterations: u32,
}

impl Default for AutonomousTaskConfig {
    fn default() -> Self {
        Self {
            task: String::new(),
            quality_threshold: 7,
            max_iterations: 10,
        }
    }
}

/// Progress snapshot emitted after each phase of an iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationProgress {
    pub iteration: u32,
    pub phase: AutonomousPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_score: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<String>,
}

/// Result of `agent.run_task()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousTaskResult {
    pub result: String,
    pub iterations: u32,
    pub quality_score: u8,
    pub threshold_met: bool,
    pub progress_history: Vec<IterationProgress>,
    pub history: Vec<ChatMessage>,
}

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// LLM connection configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<super::provider_meta::ProviderName>,
}

/// Storage backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub backend: StorageBackendType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackendType {
    File,
    Cloud,
    Memory,
}
