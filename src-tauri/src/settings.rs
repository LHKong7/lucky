use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use chrono::Utc;

use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub working_directory: Option<String>,
    #[serde(default)]
    pub llm: LlmSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmSettings {
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

#[tauri::command]
pub async fn load_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(&app)?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

/// Initialize the working directory structure if it doesn't exist.
fn init_work_dir(dir: &str) -> Result<(), String> {
    let base = PathBuf::from(dir);
    std::fs::create_dir_all(base.join("sessions")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(base.join("config")).map_err(|e| e.to_string())?;

    // Create placeholder config files if they don't exist
    let mcp_path = base.join("config/mcp.json");
    if !mcp_path.exists() {
        std::fs::write(&mcp_path, "{ \"servers\": [] }\n").map_err(|e| e.to_string())?;
    }
    let skills_path = base.join("config/skills.json");
    if !skills_path.exists() {
        std::fs::write(&skills_path, "{ \"skills\": [] }\n").map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn save_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Auto-init working directory structure
    if let Some(ref dir) = settings.working_directory {
        init_work_dir(dir)?;
    }
    let data = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pick_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let result = app
        .dialog()
        .file()
        .blocking_pick_folder();
    Ok(result.map(|p| p.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMsgInput {
    pub role: String,
    pub text: String,
}

// Tools that require user approval before execution
const DANGEROUS_TOOLS: &[&str] = &["bash", "write_file", "edit_file"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ChatResponse {
    #[serde(rename = "reply")]
    Reply { text: String },
    #[serde(rename = "approval_needed")]
    ApprovalNeeded {
        tool_call_id: String,
        tool_name: String,
        arguments: serde_json::Value,
        display: String,
        conversation: Vec<serde_json::Value>,
    },
}

#[tauri::command]
pub async fn chat_message(
    app: tauri::AppHandle,
    messages: Vec<ChatMsgInput>,
) -> Result<ChatResponse, String> {
    let settings = load_settings(app).await?;
    let llm = settings.llm;
    let provider_name = llm.provider.unwrap_or_else(|| "openai".into());

    let api_key = llm.api_key.unwrap_or_else(|| "ollama".into());

    let default_model = match provider_name.as_str() {
        "anthropic" => "claude-sonnet-4-20250514",
        "google" => "gemini-2.0-flash",
        "ollama" => "gemma4:e4b",
        _ => "gpt-4o",
    };
    let mut model = llm.model.unwrap_or_else(|| default_model.into());

    let mut base_url = llm.base_url;

    if provider_name == "ollama" {
        model = model.to_lowercase();
        let url = base_url.unwrap_or_else(|| "http://localhost:11434/v1".into());
        let trimmed = url.trim_end_matches('/').to_string();
        base_url = Some(if trimmed.ends_with("/v1") {
            trimmed
        } else {
            format!("{}/v1", trimmed)
        });
    }

    // Build initial conversation
    let mut chat_messages: Vec<serde_json::Value> = vec![
        json!({
            "role": "system",
            "content": "You are Lucky, a cute and friendly panda companion. Keep your responses short, warm, and playful. Use simple language. You have access to tools that let you execute shell commands and read files. Use them when the user asks you to do something on their computer."
        })
    ];
    for msg in &messages {
        let role = if msg.role == "pet" { "assistant" } else { &msg.role };
        chat_messages.push(json!({"role": role, "content": msg.text}));
    }

    let url = format!("{}/chat/completions", base_url.as_deref().unwrap_or("https://api.openai.com/v1"));
    run_agent_loop(chat_messages, &model, &url, &api_key).await
}

#[tauri::command]
pub async fn continue_chat(
    app: tauri::AppHandle,
    conversation: Vec<serde_json::Value>,
    approved: bool,
    tool_call_id: String,
    tool_name: String,
    arguments: serde_json::Value,
) -> Result<ChatResponse, String> {
    let settings = load_settings(app).await?;
    let llm = settings.llm;
    let provider_name = llm.provider.unwrap_or_else(|| "openai".into());
    let api_key = llm.api_key.unwrap_or_else(|| "ollama".into());
    let mut model = llm.model.unwrap_or_else(|| match provider_name.as_str() {
        "anthropic" => "claude-sonnet-4-20250514".into(),
        "google" => "gemini-2.0-flash".into(),
        "ollama" => "gemma4:e4b".into(),
        _ => "gpt-4o".into(),
    });
    let mut base_url = llm.base_url;

    if provider_name == "ollama" {
        model = model.to_lowercase();
        let url = base_url.unwrap_or_else(|| "http://localhost:11434/v1".into());
        let trimmed = url.trim_end_matches('/').to_string();
        base_url = Some(if trimmed.ends_with("/v1") {
            trimmed
        } else {
            format!("{}/v1", trimmed)
        });
    }

    let url = format!("{}/chat/completions", base_url.as_deref().unwrap_or("https://api.openai.com/v1"));

    let mut chat_messages = conversation;

    // Execute or reject the tool
    let result = if approved {
        execute_tool(&tool_name, arguments).await
    } else {
        "User denied this action.".to_string()
    };

    // Add tool result
    chat_messages.push(json!({
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": result
    }));

    run_agent_loop(chat_messages, &model, &url, &api_key).await
}

/// Core agent loop — shared between chat_message and continue_chat.
async fn run_agent_loop(
    mut chat_messages: Vec<serde_json::Value>,
    model: &str,
    url: &str,
    api_key: &str,
) -> Result<ChatResponse, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| format!("Client build failed: {}", e))?;

    let tools = build_tool_definitions();
    let max_rounds = 10;

    for _ in 0..max_rounds {
        let body = json!({
            "model": model,
            "messages": chat_messages,
            "tools": tools,
            "stream": false
        });

        let body_str = serde_json::to_string(&body).map_err(|e| format!("Serialize: {}", e))?;

        let resp = client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .body(body_str)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("Read body: {}", e))?;

        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status, &text[..text.len().min(200)]));
        }

        let resp_json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("Parse JSON: {}", e))?;

        let choice = &resp_json["choices"][0];
        let message = &choice["message"];
        let finish_reason = choice["finish_reason"].as_str().unwrap_or("");

        let tool_calls = message["tool_calls"].as_array();
        eprintln!("[agent] finish_reason={:?} tool_calls={}", finish_reason, tool_calls.map_or(0, |t| t.len()));

        if finish_reason == "tool_calls" || tool_calls.map_or(false, |tc| !tc.is_empty()) {
            // Add assistant message to conversation
            chat_messages.push(message.clone());

            let calls = tool_calls.unwrap();
            for tc in calls {
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or(json!({}));

                // Check if this tool needs approval
                if DANGEROUS_TOOLS.contains(&name.as_str()) {
                    let display = match name.as_str() {
                        "bash" => format!("$ {}", args["command"].as_str().unwrap_or("(unknown)")),
                        "write_file" => format!("Write: {}", args["path"].as_str().unwrap_or("(unknown)")),
                        "edit_file" => format!("Edit: {}", args["path"].as_str().unwrap_or("(unknown)")),
                        _ => format!("{}: {}", name, serde_json::to_string(&args).unwrap_or_default()),
                    };
                    return Ok(ChatResponse::ApprovalNeeded {
                        tool_call_id: id,
                        tool_name: name,
                        arguments: args,
                        display,
                        conversation: chat_messages,
                    });
                }

                // Safe tool — execute immediately
                let result = execute_tool(&name, args).await;
                chat_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": result
                }));
            }
            continue;
        }

        // No tool calls — final response
        let content = message["content"].as_str().unwrap_or("...").to_string();
        return Ok(ChatResponse::Reply { text: content });
    }

    Err("Too many tool call rounds".into())
}

fn build_tool_definitions() -> Vec<serde_json::Value> {
    use crate::borderless::tools::builtin::create_builtin_tools;
    use crate::borderless::tools::registry::tool_to_openai_format;

    let all_tools = create_builtin_tools(None);
    let allowed = ["bash", "read_file", "write_file", "edit_file", "grep"];

    all_tools
        .iter()
        .filter(|t| allowed.contains(&t.name.as_str()))
        .map(|t| tool_to_openai_format(t))
        .collect()
}

async fn execute_tool(name: &str, args: serde_json::Value) -> String {
    use crate::borderless::tools::builtin::create_builtin_tools;

    let tools = create_builtin_tools(None);
    let tool = tools.iter().find(|t| t.name == name);

    match tool {
        Some(t) => match (t.execute)(args).await {
            Ok(output) => {
                if output.len() > 4000 {
                    format!("{}...\n[truncated, {} bytes total]", &output[..4000], output.len())
                } else {
                    output
                }
            }
            Err(e) => format!("Tool error: {:?}", e),
        },
        None => format!("Unknown tool: {}", name),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub messages: Vec<ChatMsgInput>,
    pub updated_at: String,
}

#[tauri::command]
pub async fn save_session(app: tauri::AppHandle, messages: Vec<ChatMsgInput>) -> Result<(), String> {
    let settings = load_settings(app).await?;
    let dir = settings.working_directory.ok_or("No working directory configured")?;
    init_work_dir(&dir)?;

    let session = SessionData {
        messages,
        updated_at: Utc::now().to_rfc3339(),
    };
    let data = serde_json::to_string_pretty(&session).map_err(|e| e.to_string())?;
    std::fs::write(PathBuf::from(&dir).join("sessions/current.json"), data).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn load_session(app: tauri::AppHandle) -> Result<Vec<ChatMsgInput>, String> {
    let settings = load_settings(app).await?;
    let dir = match settings.working_directory {
        Some(d) => d,
        None => return Ok(vec![]),
    };
    let path = PathBuf::from(&dir).join("sessions/current.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let data = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let session: SessionData = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    Ok(session.messages)
}
