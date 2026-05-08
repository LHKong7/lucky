//! Agent instance — the main runtime agent with chat/stream/session APIs.

use std::sync::Arc;

use crate::borderless::agent_core::*;
use crate::borderless::context::guardrails::GuardPipeline;
use crate::borderless::skills::lifecycle::SkillLifecycleManager;
use crate::borderless::skills::registry::SkillRegistry;

use super::agent_loop;
use super::autonomous_loop;
use super::harness::AgentHarness;

/// The main runtime agent instance.
pub struct AgentInstance {
    harness: AgentHarness,
    skill_registry: Arc<SkillRegistry>,
    skill_lifecycle: SkillLifecycleManager,
    guards: GuardPipeline,
    system_prompt: String,
    max_tool_rounds: u32,
    max_tokens: u32,
    _enable_memory: bool,
    _enable_streaming: bool,
}

impl AgentInstance {
    pub(crate) fn new(
        harness: AgentHarness,
        skill_registry: Arc<SkillRegistry>,
        guards: GuardPipeline,
        system_prompt: String,
        max_tool_rounds: u32,
        max_tokens: u32,
        enable_memory: bool,
        enable_streaming: bool,
    ) -> Self {
        Self {
            harness,
            skill_registry,
            skill_lifecycle: SkillLifecycleManager::new(None),
            guards,
            system_prompt,
            max_tool_rounds,
            max_tokens,
            _enable_memory: enable_memory,
            _enable_streaming: enable_streaming,
        }
    }

    /// Send a single chat message (stateless — no session).
    pub async fn chat(
        &mut self,
        message: &str,
        history: Option<Vec<ChatMessage>>,
    ) -> Result<ChatResult, AgentError> {
        // Build messages
        let mut messages = history.unwrap_or_default();

        // Prepend system prompt if not already present
        if !messages.iter().any(|m| matches!(m, ChatMessage::System { .. })) {
            let sys_prompt = self.build_system_prompt();
            messages.insert(0, ChatMessage::system(sys_prompt));
        }

        // Run input guard
        let guard_result = self.guards.run_input(message).await;
        if guard_result.blocked {
            return Ok(ChatResult {
                reply: "Input blocked by guard pipeline.".into(),
                history: messages,
                had_tool_calls: false,
                session_id: None,
                usage: None,
                estimated_cost: None,
            });
        }

        // Auto-load triggered skills
        self.skill_lifecycle
            .match_and_load(&guard_result.value, &self.skill_registry)
            .await;

        // Add user message
        messages.push(ChatMessage::user(guard_result.value));

        // Run agent loop
        let result = agent_loop::agent_loop(
            &self.harness,
            &mut messages,
            &self.guards,
            Some(self.max_tool_rounds),
            Some(self.max_tokens),
        )
        .await?;

        // Estimate cost
        let cost = estimate_cost(&result.usage, &result.model);

        Ok(ChatResult {
            reply: result.reply,
            history: messages,
            had_tool_calls: result.had_tool_calls,
            session_id: None,
            usage: Some(result.usage),
            estimated_cost: Some(cost),
        })
    }

    /// Run an autonomous task loop.
    pub async fn run_task(
        &mut self,
        config: AutonomousTaskConfig,
    ) -> Result<AutonomousTaskResult, AgentError> {
        autonomous_loop::run_autonomous_task(
            &self.harness,
            &config,
            &self.guards,
            None,
        )
        .await
    }

    /// Get a reference to the metrics collector.
    pub fn metrics(&self) -> &crate::borderless::telemetry::MetricsCollector {
        &self.harness.metrics
    }

    /// Build the full system prompt (base + skill bodies).
    fn build_system_prompt(&self) -> String {
        let mut prompt = self.system_prompt.clone();

        // Inject active skill bodies
        let skill_bodies = self
            .skill_lifecycle
            .get_active_skill_bodies(&self.skill_registry);

        for (name, body) in &skill_bodies {
            prompt.push_str(&format!("\n\n## Skill: {}\n{}", name, body));
        }

        prompt
    }
}
