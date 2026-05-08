//! Autonomous task loop — plan → execute → review → evaluate.

use crate::borderless::agent_core::*;
use crate::borderless::context::guardrails::GuardPipeline;

use super::harness::AgentHarness;
use super::agent_loop;

/// Run the autonomous task loop.
pub async fn run_autonomous_task(
    harness: &AgentHarness,
    config: &AutonomousTaskConfig,
    guards: &GuardPipeline,
    on_progress: Option<&dyn Fn(&IterationProgress) -> bool>,
) -> Result<AutonomousTaskResult, AgentError> {
    let mut history: Vec<ChatMessage> = Vec::new();
    let mut progress_history: Vec<IterationProgress> = Vec::new();
    let mut final_score: u8 = 0;

    for iteration in 1..=config.max_iterations {
        // Phase 1: PLAN
        let plan_prompt = if iteration == 1 {
            format!(
                "You are tasked with: {}\n\nCreate a detailed numbered plan to accomplish this task. \
                 Be specific about each step.",
                config.task
            )
        } else {
            "Based on the review feedback, create an updated plan to address the identified issues.".into()
        };

        history.push(ChatMessage::user(plan_prompt));
        let plan_result = agent_loop::agent_loop(harness, &mut history, guards, Some(5), None).await?;

        let plan_progress = IterationProgress {
            iteration,
            phase: AutonomousPhase::Plan,
            quality_score: None,
            plan: Some(plan_result.reply.clone()),
            output: None,
            review: None,
            evaluation: None,
        };
        progress_history.push(plan_progress.clone());
        if let Some(cb) = on_progress {
            if !cb(&plan_progress) {
                break;
            }
        }

        // Phase 2: EXECUTE
        history.push(ChatMessage::user(
            "Now execute the plan step by step. Use the available tools to accomplish each step.",
        ));
        let exec_result = agent_loop::agent_loop(harness, &mut history, guards, None, None).await?;

        let exec_progress = IterationProgress {
            iteration,
            phase: AutonomousPhase::Execute,
            quality_score: None,
            plan: None,
            output: Some(exec_result.reply.clone()),
            review: None,
            evaluation: None,
        };
        progress_history.push(exec_progress.clone());
        if let Some(cb) = on_progress {
            if !cb(&exec_progress) {
                break;
            }
        }

        // Phase 3: REVIEW
        history.push(ChatMessage::user(
            "Review the work done so far. Be harsh but fair. \
             Identify any gaps, errors, or improvements needed. \
             Be specific about what needs to change.",
        ));
        let review_result = agent_loop::agent_loop(harness, &mut history, guards, Some(5), None).await?;

        let review_progress = IterationProgress {
            iteration,
            phase: AutonomousPhase::Review,
            quality_score: None,
            plan: None,
            output: None,
            review: Some(review_result.reply.clone()),
            evaluation: None,
        };
        progress_history.push(review_progress.clone());
        if let Some(cb) = on_progress {
            if !cb(&review_progress) {
                break;
            }
        }

        // Phase 4: EVALUATE
        history.push(ChatMessage::user(
            "Evaluate the quality of the work on a scale of 1-10. \
             Respond with JSON: {\"score\": <number>, \"reasoning\": \"<text>\", \"improvements\": [\"<text>\"]}",
        ));
        let eval_result = agent_loop::agent_loop(harness, &mut history, guards, Some(3), None).await?;

        // Parse score from the evaluation
        let score = parse_quality_score(&eval_result.reply);
        final_score = score;

        let eval_progress = IterationProgress {
            iteration,
            phase: AutonomousPhase::Evaluate,
            quality_score: Some(score),
            plan: None,
            output: None,
            review: None,
            evaluation: Some(eval_result.reply.clone()),
        };
        progress_history.push(eval_progress.clone());
        if let Some(cb) = on_progress {
            if !cb(&eval_progress) {
                break;
            }
        }

        // Check if quality threshold is met
        if score >= config.quality_threshold {
            break;
        }
    }

    // Get the final result
    let final_reply = history
        .iter()
        .rev()
        .find_map(|m| match m {
            ChatMessage::Assistant { content, .. } => content.clone(),
            _ => None,
        })
        .unwrap_or_default();

    let iterations = progress_history
        .last()
        .map(|p| p.iteration)
        .unwrap_or(0);

    Ok(AutonomousTaskResult {
        result: final_reply,
        iterations,
        quality_score: final_score,
        threshold_met: final_score >= config.quality_threshold,
        progress_history,
        history,
    })
}

/// Parse quality score from evaluation text (looks for JSON with "score" field).
fn parse_quality_score(text: &str) -> u8 {
    // Try to find JSON in the text
    if let Some(start) = text.find('{') {
        if let Some(end) = text[start..].rfind('}') {
            let json_str = &text[start..start + end + 1];
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                if let Some(score) = json["score"].as_u64() {
                    return score.min(10) as u8;
                }
            }
        }
    }

    // Fallback: look for a number
    for word in text.split_whitespace() {
        if let Ok(n) = word.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u8>() {
            if (1..=10).contains(&n) {
                return n;
            }
        }
    }

    5 // Default middle score
}
