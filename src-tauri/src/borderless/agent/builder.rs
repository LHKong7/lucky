//! Agent builder — fluent API for configuring and constructing an AgentInstance.

use std::sync::Arc;

use crate::borderless::agent_core::*;
use crate::borderless::context::guardrails::GuardPipeline;
use crate::borderless::skills::registry::SkillRegistry;
use crate::borderless::telemetry::{MetricsCollector, Telemetry};
use crate::borderless::tools::executor::{ApprovalCallback, ToolExecutor};
use crate::borderless::tools::registry::ToolRegistry;
use crate::borderless::tools::sandbox::{Sandbox, SandboxConfig};
use crate::borderless::tools::{HumanInputCallback, ToolContext};

use super::harness::AgentHarness;
use super::instance::AgentInstance;

/// Fluent builder for creating an AgentInstance.
pub struct AgentBuilder {
    llm: Option<Arc<dyn LlmProvider>>,
    llm_config: Option<LlmConfig>,
    system_prompt: Option<String>,
    tools: Vec<ToolDefinition>,
    skills: Vec<SkillDefinition>,
    include_builtin_tools: bool,
    enable_memory: bool,
    enable_streaming: bool,
    enable_context: bool,
    max_tool_rounds: u32,
    max_tokens: u32,
    approval_callback: Option<ApprovalCallback>,
    human_input_callback: Option<HumanInputCallback>,
    sandbox_config: SandboxConfig,
    telemetry: Option<Arc<Telemetry>>,
    guards: Option<GuardPipeline>,
    storage_config: Option<StorageConfig>,
}

impl AgentBuilder {
    pub fn new() -> Self {
        Self {
            llm: None,
            llm_config: None,
            system_prompt: None,
            tools: Vec::new(),
            skills: Vec::new(),
            include_builtin_tools: true,
            enable_memory: false,
            enable_streaming: false,
            enable_context: true,
            max_tool_rounds: 20,
            max_tokens: 8192,
            approval_callback: None,
            human_input_callback: None,
            sandbox_config: SandboxConfig::default(),
            telemetry: None,
            guards: None,
            storage_config: None,
        }
    }

    /// Set a pre-built LLM provider.
    pub fn set_llm(mut self, llm: Arc<dyn LlmProvider>) -> Self {
        self.llm = Some(llm);
        self
    }

    /// Set LLM configuration (provider will be constructed from this).
    pub fn set_llm_config(mut self, config: LlmConfig) -> Self {
        self.llm_config = Some(config);
        self
    }

    /// Set the base system prompt.
    pub fn set_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Add a tool.
    pub fn add_tool(mut self, tool: ToolDefinition) -> Self {
        self.tools.push(tool);
        self
    }

    /// Add a skill.
    pub fn add_skill(mut self, skill: SkillDefinition) -> Self {
        self.skills.push(skill);
        self
    }

    /// Whether to include built-in tools. Default: true.
    pub fn include_builtin_tools(mut self, include: bool) -> Self {
        self.include_builtin_tools = include;
        self
    }

    /// Enable long-term memory. Default: false.
    pub fn enable_memory(mut self, enable: bool) -> Self {
        self.enable_memory = enable;
        self
    }

    /// Enable streaming by default. Default: false.
    pub fn enable_streaming(mut self, enable: bool) -> Self {
        self.enable_streaming = enable;
        self
    }

    /// Max tool rounds per turn. Default: 20.
    pub fn set_max_tool_rounds(mut self, rounds: u32) -> Self {
        self.max_tool_rounds = rounds;
        self
    }

    /// Max output tokens per LLM call. Default: 8192.
    pub fn set_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = tokens;
        self
    }

    /// Set the approval callback for dangerous tools.
    pub fn set_approval_callback(mut self, callback: ApprovalCallback) -> Self {
        self.approval_callback = Some(callback);
        self
    }

    /// Set the human input callback for the ask_user tool.
    pub fn set_human_input_callback(mut self, callback: HumanInputCallback) -> Self {
        self.human_input_callback = Some(callback);
        self
    }

    /// Set sandbox configuration.
    pub fn set_sandbox(mut self, config: SandboxConfig) -> Self {
        self.sandbox_config = config;
        self
    }

    /// Set a custom telemetry instance.
    pub fn set_telemetry(mut self, telemetry: Arc<Telemetry>) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    /// Set a custom guard pipeline.
    pub fn set_guards(mut self, guards: GuardPipeline) -> Self {
        self.guards = Some(guards);
        self
    }

    /// Set storage configuration.
    pub fn set_storage(mut self, config: StorageConfig) -> Self {
        self.storage_config = Some(config);
        self
    }

    /// Build the AgentInstance.
    pub fn build(self) -> Result<AgentInstance, AgentError> {
        // Resolve LLM provider
        let llm = if let Some(llm) = self.llm {
            llm
        } else if let Some(config) = self.llm_config {
            build_llm_provider(&config)?
        } else {
            return Err(AgentError::Configuration(
                "No LLM provider or config provided. Call set_llm() or set_llm_config().".into(),
            ));
        };

        // Build skill registry (shared via Arc for ToolContext)
        let mut skill_registry = SkillRegistry::new();
        for skill in self.skills {
            skill_registry.register(skill);
        }
        let skill_registry = Arc::new(skill_registry);

        // Build tool registry with ToolContext for callback-dependent tools
        let mut tool_registry = ToolRegistry::new();

        if self.include_builtin_tools {
            let tool_ctx = Arc::new(ToolContext {
                human_input: self.human_input_callback,
                skill_registry: Some(skill_registry.clone()),
                ..ToolContext::default()
            });

            for tool in crate::borderless::tools::create_builtin_tools(Some(tool_ctx)) {
                tool_registry.register(tool);
            }
        }

        for tool in self.tools {
            tool_registry.register(tool);
        }

        // Build components
        let tool_executor = ToolExecutor::new(self.approval_callback);
        let sandbox = Sandbox::new(self.sandbox_config);
        let telemetry = self.telemetry.unwrap_or_else(|| Arc::new(Telemetry::noop()));
        let metrics = Arc::new(MetricsCollector::new());
        let guards = self.guards.unwrap_or_default();

        let harness = AgentHarness {
            llm,
            tool_registry,
            tool_executor,
            sandbox,
            telemetry,
            metrics,
        };

        Ok(AgentInstance::new(
            harness,
            skill_registry,
            guards,
            self.system_prompt.unwrap_or_default(),
            self.max_tool_rounds,
            self.max_tokens,
            self.enable_memory,
            self.enable_streaming,
        ))
    }
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Build an LLM provider from configuration.
fn build_llm_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, AgentError> {
    let provider = config.provider.unwrap_or(ProviderName::OpenAI);
    let model = config.model.clone().unwrap_or_else(|| "gpt-4o".into());

    match provider {
        #[cfg(feature = "openai")]
        ProviderName::OpenAI => {
            Ok(Arc::new(crate::borderless::providers::openai::OpenAIProvider::new(
                &config.api_key,
                &model,
                config.base_url.clone(),
            )))
        }
        #[cfg(feature = "anthropic")]
        ProviderName::Anthropic => {
            Ok(Arc::new(crate::borderless::providers::anthropic::AnthropicProvider::new(
                &config.api_key,
                &model,
                config.base_url.clone(),
            )))
        }
        #[cfg(feature = "google")]
        ProviderName::Google => {
            Ok(Arc::new(crate::borderless::providers::google::GoogleProvider::new(
                &config.api_key,
                &model,
                config.base_url.clone(),
            )))
        }
        #[allow(unreachable_patterns)]
        _ => Err(AgentError::Configuration(format!(
            "Provider '{}' is not enabled. Enable the corresponding feature flag.",
            provider
        ))),
    }
}
