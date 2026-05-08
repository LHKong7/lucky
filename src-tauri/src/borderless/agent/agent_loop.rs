//! Core agent loop — LLM call → tool execution → observation folding.

use crate::borderless::agent_core::*;
use crate::borderless::context::guardrails::GuardPipeline;
use crate::borderless::tools::executor::ToolCallRequest;
use crate::borderless::tools::registry::tool_to_openai_format;

use super::harness::AgentHarness;

/// Maximum tool rounds per turn (default).
const DEFAULT_MAX_TOOL_ROUNDS: u32 = 20;

/// Run the core agent loop: call LLM, execute tools, fold observations, repeat.
pub async fn agent_loop(
    harness: &AgentHarness,
    messages: &mut Vec<ChatMessage>,
    guards: &GuardPipeline,
    max_tool_rounds: Option<u32>,
    max_tokens: Option<u32>,
) -> Result<AgentLoopResult, AgentError> {
    let max_rounds = max_tool_rounds.unwrap_or(DEFAULT_MAX_TOOL_ROUNDS);
    let mut had_tool_calls = false;
    let mut total_usage = TokenUsage::default();

    // Build tools in OpenAI format
    let tools: Vec<serde_json::Value> = harness
        .tool_registry
        .as_map()
        .values()
        .map(tool_to_openai_format)
        .collect();

    let tools_json = if tools.is_empty() {
        None
    } else {
        Some(serde_json::json!(tools))
    };

    for _round in 0..max_rounds {
        let options = ChatOptions {
            tools: tools_json.clone(),
            temperature: None,
            max_tokens,
            stream: false,
        };

        // Call LLM
        let response = harness.llm.chat(messages, &options).await?;

        // Accumulate usage
        let usage = to_token_usage(&response.usage);
        total_usage = merge_token_usage(&total_usage, &usage);

        // If no tool calls, we're done
        if response.tool_calls.is_empty() {
            // Add assistant message
            messages.push(ChatMessage::Assistant {
                content: response.content.clone(),
                tool_calls: Vec::new(),
                thinking: response.thinking.clone(),
            });

            return Ok(AgentLoopResult {
                reply: response.content.unwrap_or_default(),
                had_tool_calls,
                usage: total_usage,
                model: response.model,
            });
        }

        // We have tool calls
        had_tool_calls = true;

        // Add assistant message with tool calls
        let tool_call_msgs: Vec<ToolCallMsg> = response
            .tool_calls
            .iter()
            .map(|tc| ToolCallMsg {
                id: tc.id.clone(),
                call_type: "function".into(),
                function: FunctionCall {
                    name: tc.name.clone(),
                    arguments: serde_json::to_string(&tc.arguments).unwrap_or_default(),
                },
            })
            .collect();

        messages.push(ChatMessage::Assistant {
            content: response.content.clone(),
            tool_calls: tool_call_msgs,
            thinking: response.thinking.clone(),
        });

        // Execute tool calls
        let requests: Vec<ToolCallRequest> = response
            .tool_calls
            .into_iter()
            .map(ToolCallRequest::from)
            .collect();

        let results = harness
            .tool_executor
            .execute_all(requests, &harness.tool_registry)
            .await;

        // Fold observations back into messages
        for result in results {
            // Run observation guard
            let guard_result = guards
                .run_observation(&result.output, Some(&result.name))
                .await;

            messages.push(ChatMessage::Tool {
                tool_call_id: result.id,
                content: guard_result.value,
            });

            // Record metrics
            harness.metrics.record_tool_call(
                &result.name,
                result.duration_ms,
                result.success,
            );
        }
    }

    // Max rounds exceeded — return last assistant content
    let last_reply = messages
        .iter()
        .rev()
        .find_map(|m| match m {
            ChatMessage::Assistant { content, .. } => content.clone(),
            _ => None,
        })
        .unwrap_or_else(|| "Max tool rounds exceeded".into());

    Ok(AgentLoopResult {
        reply: last_reply,
        had_tool_calls,
        usage: total_usage,
        model: harness.llm.model().to_string(),
    })
}

/// Result of the agent loop.
pub struct AgentLoopResult {
    pub reply: String,
    pub had_tool_calls: bool,
    pub usage: TokenUsage,
    pub model: String,
}
