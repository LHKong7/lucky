//! Built-in tools: bash, read_file, write_file, grep, edit_file, WebSearch, WebFetch, etc.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::borderless::agent_core::{ParameterDef, PermissionLevel, ToolDefinition, ToolError};

// ---------------------------------------------------------------------------
// ToolContext — shared runtime state for callback-dependent tools
// ---------------------------------------------------------------------------

/// Callback type for human-in-the-loop input.
pub type HumanInputCallback = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = String> + Send>> + Send + Sync,
>;

/// Shared runtime context for built-in tools that need external state.
pub struct ToolContext {
    /// Callback for ask_user tool.
    pub human_input: Option<HumanInputCallback>,
    /// Skill registry for the Skill loader tool.
    pub skill_registry: Option<Arc<crate::borderless::skills::registry::SkillRegistry>>,
    /// Set of already-loaded skill names (per session).
    pub loaded_skills: Arc<Mutex<HashSet<String>>>,
    /// In-memory todo list for the TodoWrite tool.
    pub todos: Arc<Mutex<Vec<TodoItem>>>,
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            human_input: None,
            skill_registry: None,
            loaded_skills: Arc::new(Mutex::new(HashSet::new())),
            todos: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

/// A todo item for the TodoWrite tool.
#[derive(Debug, Clone)]
pub struct TodoItem {
    pub content: String,
    pub status: String,
    pub active_form: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Create all built-in tool definitions.
/// If `ctx` is provided, callback-dependent tools (ask_user, Skill, TodoWrite) are included.
pub fn create_builtin_tools(ctx: Option<Arc<ToolContext>>) -> Vec<ToolDefinition> {
    let mut tools = vec![
        create_bash_tool(),
        create_read_file_tool(),
        create_write_file_tool(),
        create_grep_tool(),
        create_edit_file_tool(),
        create_web_search_tool(),
        create_web_fetch_tool(),
        create_search_knowledge_base_tool(),
        create_read_email_tool(),
    ];

    if let Some(ctx) = ctx {
        tools.push(create_ask_user_tool(ctx.clone()));
        tools.push(create_skill_tool(ctx.clone()));
        tools.push(create_todo_write_tool(ctx.clone()));
    }

    tools
}

// ---------------------------------------------------------------------------
// Helper: build a parameter map
// ---------------------------------------------------------------------------

fn param(param_type: &str, description: &str) -> ParameterDef {
    ParameterDef {
        param_type: param_type.into(),
        description: Some(description.into()),
        enum_values: None,
    }
}

fn tool_err(name: &str, msg: impl Into<String>) -> ToolError {
    ToolError::Execution {
        name: name.into(),
        message: msg.into(),
        source: None,
    }
}

// ---------------------------------------------------------------------------
// bash
// ---------------------------------------------------------------------------

fn create_bash_tool() -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("command".into(), param("string", "The shell command to execute"));

    ToolDefinition {
        name: "bash".into(),
        description: "Execute a shell command and return its output".into(),
        parameters: Some(params),
        required: vec!["command".into()],
        execute: Box::new(|args| {
            Box::pin(async move {
                let command = args["command"]
                    .as_str()
                    .ok_or_else(|| tool_err("bash", "Missing 'command' argument"))?;

                let output = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .output()
                    .await
                    .map_err(|e| tool_err("bash", e.to_string()))?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    Ok(stdout.to_string())
                } else {
                    Ok(format!(
                        "STDOUT:\n{}\nSTDERR:\n{}\nExit code: {}",
                        stdout, stderr, output.status
                    ))
                }
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Dangerous,
        timeout: Some(std::time::Duration::from_secs(120)),
        concurrency_safe: false,
    }
}

// ---------------------------------------------------------------------------
// read_file
// ---------------------------------------------------------------------------

fn create_read_file_tool() -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("path".into(), param("string", "Absolute path to the file to read"));
    params.insert("offset".into(), param("integer", "Line number to start reading from (0-based)"));
    params.insert("limit".into(), param("integer", "Maximum number of lines to read"));

    ToolDefinition {
        name: "read_file".into(),
        description: "Read a file's contents with optional pagination".into(),
        parameters: Some(params),
        required: vec!["path".into()],
        execute: Box::new(|args| {
            Box::pin(async move {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| tool_err("read_file", "Missing 'path' argument"))?;

                let content = tokio::fs::read_to_string(path)
                    .await
                    .map_err(|e| tool_err("read_file", format!("Failed to read '{}': {}", path, e)))?;

                let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(2000) as usize;

                let lines: Vec<&str> = content.lines().collect();
                let selected: Vec<String> = lines
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .enumerate()
                    .map(|(i, line)| format!("{}\t{}", offset + i + 1, line))
                    .collect();

                Ok(selected.join("\n"))
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Safe,
        timeout: Some(std::time::Duration::from_secs(30)),
        concurrency_safe: true,
    }
}

// ---------------------------------------------------------------------------
// write_file
// ---------------------------------------------------------------------------

fn create_write_file_tool() -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("path".into(), param("string", "Absolute path to the file to write"));
    params.insert("content".into(), param("string", "Content to write to the file"));

    ToolDefinition {
        name: "write_file".into(),
        description: "Write content to a file (creates or overwrites)".into(),
        parameters: Some(params),
        required: vec!["path".into(), "content".into()],
        execute: Box::new(|args| {
            Box::pin(async move {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| tool_err("write_file", "Missing 'path' argument"))?;
                let content = args["content"]
                    .as_str()
                    .ok_or_else(|| tool_err("write_file", "Missing 'content' argument"))?;

                if let Some(parent) = std::path::Path::new(path).parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| tool_err("write_file", format!("Failed to create directory: {}", e)))?;
                }

                tokio::fs::write(path, content)
                    .await
                    .map_err(|e| tool_err("write_file", format!("Failed to write '{}': {}", path, e)))?;

                Ok(format!("Successfully wrote {} bytes to {}", content.len(), path))
            })
        }),
        requires_approval: true,
        permission_level: PermissionLevel::Moderate,
        timeout: Some(std::time::Duration::from_secs(30)),
        concurrency_safe: false,
    }
}

// ---------------------------------------------------------------------------
// grep (with context lines support)
// ---------------------------------------------------------------------------

fn create_grep_tool() -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("pattern".into(), param("string", "Regex pattern to search for"));
    params.insert("path".into(), param("string", "File or directory to search in"));
    params.insert("context_before".into(), param("integer", "Number of lines to show before each match (max 10)"));
    params.insert("context_after".into(), param("integer", "Number of lines to show after each match (max 10)"));

    ToolDefinition {
        name: "grep".into(),
        description: "Search for a regex pattern in files with optional context lines".into(),
        parameters: Some(params),
        required: vec!["pattern".into(), "path".into()],
        execute: Box::new(|args| {
            Box::pin(async move {
                let pattern = args["pattern"]
                    .as_str()
                    .ok_or_else(|| tool_err("grep", "Missing 'pattern' argument"))?;
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| tool_err("grep", "Missing 'path' argument"))?;

                let before = args.get("context_before")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    .min(10) as usize;
                let after = args.get("context_after")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
                    .min(10) as usize;

                let re = regex::Regex::new(pattern)
                    .map_err(|e| tool_err("grep", format!("Invalid regex: {}", e)))?;

                let content = tokio::fs::read_to_string(path)
                    .await
                    .map_err(|e| tool_err("grep", format!("Failed to read '{}': {}", path, e)))?;

                let lines: Vec<&str> = content.lines().collect();
                let total = lines.len();

                if before == 0 && after == 0 {
                    // Simple mode: just matching lines
                    let matches: Vec<String> = lines
                        .iter()
                        .enumerate()
                        .filter(|(_, line)| re.is_match(line))
                        .map(|(i, line)| format!("{}:{}", i + 1, line))
                        .collect();

                    if matches.is_empty() {
                        Ok("No matches found".into())
                    } else {
                        Ok(matches.join("\n"))
                    }
                } else {
                    // Context mode: show surrounding lines
                    let match_indices: Vec<usize> = lines
                        .iter()
                        .enumerate()
                        .filter(|(_, line)| re.is_match(line))
                        .map(|(i, _)| i)
                        .collect();

                    if match_indices.is_empty() {
                        return Ok("No matches found".into());
                    }

                    // Build set of lines to display
                    let mut display_lines: HashSet<usize> = HashSet::new();
                    for &idx in &match_indices {
                        let start = idx.saturating_sub(before);
                        let end = (idx + after + 1).min(total);
                        for i in start..end {
                            display_lines.insert(i);
                        }
                    }

                    let mut result = Vec::new();
                    let mut last_line: Option<usize> = None;
                    let mut sorted: Vec<usize> = display_lines.into_iter().collect();
                    sorted.sort();

                    for i in sorted {
                        // Add separator if there's a gap
                        if let Some(last) = last_line {
                            if i > last + 1 {
                                result.push("--".to_string());
                            }
                        }
                        let prefix = if match_indices.contains(&i) { ">" } else { " " };
                        result.push(format!("{}{}:{}", prefix, i + 1, lines[i]));
                        last_line = Some(i);
                    }

                    Ok(result.join("\n"))
                }
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Safe,
        timeout: Some(std::time::Duration::from_secs(30)),
        concurrency_safe: true,
    }
}

// ---------------------------------------------------------------------------
// edit_file
// ---------------------------------------------------------------------------

fn create_edit_file_tool() -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("path".into(), param("string", "Absolute path to the file to edit"));
    params.insert("old_text".into(), param("string", "Exact text to find and replace"));
    params.insert("new_text".into(), param("string", "Replacement text"));

    ToolDefinition {
        name: "edit_file".into(),
        description: "Replace an exact text match in a file with new text".into(),
        parameters: Some(params),
        required: vec!["path".into(), "old_text".into(), "new_text".into()],
        execute: Box::new(|args| {
            Box::pin(async move {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| tool_err("edit_file", "Missing 'path' argument"))?;
                let old_text = args["old_text"]
                    .as_str()
                    .ok_or_else(|| tool_err("edit_file", "Missing 'old_text' argument"))?;
                let new_text = args["new_text"]
                    .as_str()
                    .ok_or_else(|| tool_err("edit_file", "Missing 'new_text' argument"))?;

                let content = tokio::fs::read_to_string(path)
                    .await
                    .map_err(|e| tool_err("edit_file", format!("Failed to read '{}': {}", path, e)))?;

                if !content.contains(old_text) {
                    return Ok(format!("Error: Text not found in {}", path));
                }

                // Replace first occurrence only
                let updated = content.replacen(old_text, new_text, 1);
                tokio::fs::write(path, &updated)
                    .await
                    .map_err(|e| tool_err("edit_file", format!("Failed to write '{}': {}", path, e)))?;

                Ok(format!("Edited {}", path))
            })
        }),
        requires_approval: true,
        permission_level: PermissionLevel::Moderate,
        timeout: Some(std::time::Duration::from_secs(30)),
        concurrency_safe: false,
    }
}

// ---------------------------------------------------------------------------
// WebSearch
// ---------------------------------------------------------------------------

const WEB_MAX_CHARS: usize = 50_000;

/// Strip HTML tags and decode common entities.
fn html_to_text(html: &str) -> String {
    let mut text = html.to_string();

    // Remove script/style blocks
    let script_re = regex::Regex::new(r"(?is)<script[\s\S]*?</script>").unwrap();
    let style_re = regex::Regex::new(r"(?is)<style[\s\S]*?</style>").unwrap();
    text = script_re.replace_all(&text, "").to_string();
    text = style_re.replace_all(&text, "").to_string();

    // Convert block elements to newlines
    let block_re = regex::Regex::new(r"(?i)</(p|div|h[1-6]|li|tr)>").unwrap();
    text = block_re.replace_all(&text, "\n").to_string();
    let br_re = regex::Regex::new(r"(?i)<br\s*/?>").unwrap();
    text = br_re.replace_all(&text, "\n").to_string();

    // Strip remaining tags
    let tag_re = regex::Regex::new(r"<[^>]+>").unwrap();
    text = tag_re.replace_all(&text, "").to_string();

    // Decode common HTML entities
    text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // Collapse whitespace
    let space_re = regex::Regex::new(r"[ \t]+").unwrap();
    text = space_re.replace_all(&text, " ").to_string();
    let nl_re = regex::Regex::new(r"\n{3,}").unwrap();
    text = nl_re.replace_all(&text, "\n\n").to_string();

    text.trim().to_string()
}

fn create_web_search_tool() -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("query".into(), param("string", "Search query"));
    params.insert("allowed_domains".into(), param("array", "Only include results from these domains"));
    params.insert("blocked_domains".into(), param("array", "Exclude results from these domains"));

    ToolDefinition {
        name: "WebSearch".into(),
        description: "Search the web for current information using DuckDuckGo".into(),
        parameters: Some(params),
        required: vec!["query".into()],
        execute: Box::new(|args| {
            Box::pin(async move {
                let query = args["query"]
                    .as_str()
                    .ok_or_else(|| tool_err("WebSearch", "Missing 'query' argument"))?;

                if query.trim().is_empty() {
                    return Ok("Error: search query is empty".into());
                }

                let encoded = urlencoding::encode(query);
                let url = format!("https://html.duckduckgo.com/html/?q={}", encoded);

                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .map_err(|e| tool_err("WebSearch", e.to_string()))?;

                let resp = client
                    .get(&url)
                    .header("User-Agent", "Mozilla/5.0 (compatible; BorderlessAgent/1.0)")
                    .send()
                    .await
                    .map_err(|e| tool_err("WebSearch", format!("Search failed: {}", e)))?;

                if !resp.status().is_success() {
                    return Ok(format!("Error: search returned HTTP {}", resp.status()));
                }

                let html = resp.text().await
                    .map_err(|e| tool_err("WebSearch", e.to_string()))?;

                // Parse DuckDuckGo HTML results
                let result_re = regex::Regex::new(
                    r#"<a[^>]+class="result__a"[^>]*href="([^"]+)"[^>]*>([\s\S]*?)</a>[\s\S]*?<a[^>]+class="result__snippet"[^>]*>([\s\S]*?)</a>"#
                ).unwrap();

                let mut results: Vec<(String, String, String)> = Vec::new();
                for cap in result_re.captures_iter(&html) {
                    let raw_url = cap[1].replace("//duckduckgo.com/l/?uddg=", "");
                    let raw_url = raw_url.split('&').next().unwrap_or(&raw_url);
                    let decoded_url = urlencoding::decode(raw_url).unwrap_or_default().to_string();
                    let title = html_to_text(&cap[2]);
                    let snippet = html_to_text(&cap[3]);
                    if !title.is_empty() && !decoded_url.is_empty() {
                        results.push((title, decoded_url, snippet));
                    }
                }

                // Domain filtering
                if let Some(allowed) = args.get("allowed_domains").and_then(|v| v.as_array()) {
                    let domains: Vec<&str> = allowed.iter().filter_map(|d| d.as_str()).collect();
                    if !domains.is_empty() {
                        results.retain(|(_, url, _)| domains.iter().any(|d| url.contains(d)));
                    }
                }
                if let Some(blocked) = args.get("blocked_domains").and_then(|v| v.as_array()) {
                    let domains: Vec<&str> = blocked.iter().filter_map(|d| d.as_str()).collect();
                    results.retain(|(_, url, _)| !domains.iter().any(|d| url.contains(d)));
                }

                if results.is_empty() {
                    return Ok(format!("No search results found for: \"{}\"", query));
                }

                let formatted: Vec<String> = results
                    .iter()
                    .take(10)
                    .enumerate()
                    .map(|(i, (title, url, snippet))| {
                        format!("[{}] {}\n    URL: {}\n    {}", i + 1, title, url, snippet)
                    })
                    .collect();

                let output = format!("Search results for \"{}\":\n\n{}", query, formatted.join("\n\n"));
                Ok(output.chars().take(WEB_MAX_CHARS).collect())
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Safe,
        timeout: Some(std::time::Duration::from_secs(30)),
        concurrency_safe: true,
    }
}

// ---------------------------------------------------------------------------
// WebFetch
// ---------------------------------------------------------------------------

fn create_web_fetch_tool() -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("url".into(), param("string", "URL to fetch content from"));
    params.insert("prompt".into(), param("string", "Context for why you are fetching this URL"));

    ToolDefinition {
        name: "WebFetch".into(),
        description: "Fetch content from a URL and return as plain text".into(),
        parameters: Some(params),
        required: vec!["url".into()],
        execute: Box::new(|args| {
            Box::pin(async move {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| tool_err("WebFetch", "Missing 'url' argument"))?;
                let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

                if url.trim().is_empty() {
                    return Ok("Error: URL is empty".into());
                }

                // Basic URL validation
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Ok(format!("Error: invalid URL — \"{}\"", url));
                }

                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .redirect(reqwest::redirect::Policy::limited(10))
                    .build()
                    .map_err(|e| tool_err("WebFetch", e.to_string()))?;

                let resp = client
                    .get(url)
                    .header("User-Agent", "Mozilla/5.0 (compatible; BorderlessAgent/1.0)")
                    .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
                    .send()
                    .await
                    .map_err(|e| tool_err("WebFetch", format!("Fetch failed: {}", e)))?;

                if !resp.status().is_success() {
                    return Ok(format!("Error: fetch returned HTTP {} for {}", resp.status(), url));
                }

                let content_type = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();

                let raw = resp.text().await
                    .map_err(|e| tool_err("WebFetch", e.to_string()))?;

                let mut content = if content_type.contains("html") {
                    html_to_text(&raw)
                } else {
                    raw
                };

                if content.len() > WEB_MAX_CHARS {
                    content.truncate(WEB_MAX_CHARS);
                    content.push_str("\n...[truncated]");
                }

                Ok(format!(
                    "— Fetched: {}\n— Prompt: {}\n— Content ({} chars):\n\n{}",
                    url, prompt, content.len(), content
                ))
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Safe,
        timeout: Some(std::time::Duration::from_secs(30)),
        concurrency_safe: true,
    }
}

// ---------------------------------------------------------------------------
// ask_user
// ---------------------------------------------------------------------------

fn create_ask_user_tool(ctx: Arc<ToolContext>) -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("question".into(), param("string", "The question to ask the user"));

    ToolDefinition {
        name: "ask_user".into(),
        description: "Ask the user a question and wait for their response".into(),
        parameters: Some(params),
        required: vec!["question".into()],
        execute: Box::new(move |args| {
            let ctx = ctx.clone();
            Box::pin(async move {
                let question = args["question"]
                    .as_str()
                    .ok_or_else(|| tool_err("ask_user", "Missing 'question' argument"))?;

                if let Some(ref callback) = ctx.human_input {
                    let answer = callback(question.to_string()).await;
                    if answer.is_empty() {
                        Ok("(User provided no response)".into())
                    } else {
                        Ok(answer)
                    }
                } else {
                    Ok("(Human input is not available in this mode)".into())
                }
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Safe,
        timeout: Some(std::time::Duration::from_secs(300)),
        concurrency_safe: false,
    }
}

// ---------------------------------------------------------------------------
// Skill
// ---------------------------------------------------------------------------

fn create_skill_tool(ctx: Arc<ToolContext>) -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("skill".into(), param("string", "Name of the skill to load"));

    ToolDefinition {
        name: "Skill".into(),
        description: "Load a skill's knowledge into the current conversation context".into(),
        parameters: Some(params),
        required: vec!["skill".into()],
        execute: Box::new(move |args| {
            let ctx = ctx.clone();
            Box::pin(async move {
                let skill_name = args["skill"]
                    .as_str()
                    .ok_or_else(|| tool_err("Skill", "Missing 'skill' argument"))?;

                // Check if already loaded
                {
                    let loaded = ctx.loaded_skills.lock().unwrap();
                    if loaded.contains(skill_name) {
                        return Ok(format!(
                            "(Skill '{}' is already loaded for this task. \
                             Use the previously loaded knowledge to answer the user directly, \
                             and do NOT call the Skill tool again.)",
                            skill_name
                        ));
                    }
                }

                let registry = match &ctx.skill_registry {
                    Some(r) => r,
                    None => return Ok("Error: Skill registry is not configured".into()),
                };

                match registry.get(skill_name) {
                    Some(skill) => {
                        ctx.loaded_skills.lock().unwrap().insert(skill_name.to_string());
                        Ok(format!(
                            "<skill-loaded name=\"{}\">\n{}\n</skill-loaded>\n\n\
                             You have now loaded this skill. Use the knowledge above to complete the user's task.\n\
                             Do NOT call the Skill tool again for this task; respond with your full answer in natural language.",
                            skill_name, skill.body
                        ))
                    }
                    None => {
                        let available: Vec<String> = registry.list().iter().map(|s| s.to_string()).collect();
                        let names = if available.is_empty() {
                            "none".to_string()
                        } else {
                            available.join(", ")
                        };
                        Ok(format!("Error: Unknown skill '{}'. Available: {}", skill_name, names))
                    }
                }
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Safe,
        timeout: Some(std::time::Duration::from_secs(10)),
        concurrency_safe: false,
    }
}

// ---------------------------------------------------------------------------
// TodoWrite
// ---------------------------------------------------------------------------

fn create_todo_write_tool(ctx: Arc<ToolContext>) -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("items".into(), param("array", "Array of todo items with content, status, and activeForm fields"));

    ToolDefinition {
        name: "TodoWrite".into(),
        description: "Update the task/todo list. Each item needs content, status (pending/in_progress/completed), and activeForm.".into(),
        parameters: Some(params),
        required: vec!["items".into()],
        execute: Box::new(move |args| {
            let ctx = ctx.clone();
            Box::pin(async move {
                let items = args["items"]
                    .as_array()
                    .ok_or_else(|| tool_err("TodoWrite", "Missing 'items' array"))?;

                let mut validated: Vec<TodoItem> = Vec::new();
                let mut in_progress_count = 0;

                for (i, item) in items.iter().enumerate() {
                    let content = item.get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let status = item.get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("pending")
                        .to_lowercase();
                    let active_form = item.get("activeForm")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();

                    if content.is_empty() || active_form.is_empty() {
                        return Ok(format!("Error: Item {}: content and activeForm required", i));
                    }
                    if !["pending", "in_progress", "completed"].contains(&status.as_str()) {
                        return Ok(format!("Error: Item {}: invalid status '{}'", i, status));
                    }
                    if status == "in_progress" {
                        in_progress_count += 1;
                    }

                    validated.push(TodoItem {
                        content,
                        status,
                        active_form,
                    });
                }

                if in_progress_count > 1 {
                    return Ok("Error: Only one task can be in_progress".into());
                }

                // Truncate to 20 items
                validated.truncate(20);

                // Render
                let rendered = if validated.is_empty() {
                    "No todos.".to_string()
                } else {
                    let done = validated.iter().filter(|t| t.status == "completed").count();
                    let total = validated.len();
                    let lines: Vec<String> = validated
                        .iter()
                        .map(|t| {
                            let mark = match t.status.as_str() {
                                "completed" => "[x]",
                                "in_progress" => "[>]",
                                _ => "[ ]",
                            };
                            format!("{} {}", mark, t.content)
                        })
                        .collect();
                    format!("{}\n({}/{} done)", lines.join("\n"), done, total)
                };

                *ctx.todos.lock().unwrap() = validated;
                Ok(rendered)
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Safe,
        timeout: Some(std::time::Duration::from_secs(10)),
        concurrency_safe: false,
    }
}

// ---------------------------------------------------------------------------
// search_knowledge_base (stub)
// ---------------------------------------------------------------------------

fn create_search_knowledge_base_tool() -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("query".into(), param("string", "Search query"));

    ToolDefinition {
        name: "search_knowledge_base".into(),
        description: "Search a connected knowledge base for relevant information".into(),
        parameters: Some(params),
        required: vec!["query".into()],
        execute: Box::new(|_args| {
            Box::pin(async move {
                Ok("[Stub] Knowledge base is not connected. \
                    Use read_file and grep on local files under the workspace for retrieval."
                    .into())
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Safe,
        timeout: Some(std::time::Duration::from_secs(10)),
        concurrency_safe: true,
    }
}

// ---------------------------------------------------------------------------
// read_email (stub)
// ---------------------------------------------------------------------------

fn create_read_email_tool() -> ToolDefinition {
    let mut params = HashMap::new();
    params.insert("folder".into(), param("string", "Email folder to read from (default: Inbox)"));
    params.insert("limit".into(), param("integer", "Maximum number of emails to return (default: 10)"));

    ToolDefinition {
        name: "read_email".into(),
        description: "Read emails from a connected email account".into(),
        parameters: Some(params),
        required: vec![],
        execute: Box::new(|_args| {
            Box::pin(async move {
                Ok("[Stub] Email is not connected. \
                    When integrated, this would list emails from the specified folder."
                    .into())
            })
        }),
        requires_approval: false,
        permission_level: PermissionLevel::Safe,
        timeout: Some(std::time::Duration::from_secs(10)),
        concurrency_safe: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_edit_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world\nfoo bar\n").unwrap();

        let tool = create_edit_file_tool();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "foo bar",
            "new_text": "baz qux"
        });
        let result = (tool.execute)(args).await.unwrap();
        assert!(result.contains("Edited"));

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("baz qux"));
        assert!(!content.contains("foo bar"));
    }

    #[tokio::test]
    async fn test_edit_file_not_found_text() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world\n").unwrap();

        let tool = create_edit_file_tool();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_text": "nonexistent",
            "new_text": "replacement"
        });
        let result = (tool.execute)(args).await.unwrap();
        assert!(result.contains("Text not found"));
    }

    #[tokio::test]
    async fn test_grep_with_context() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(
            &file_path,
            "line 1\nline 2\nMATCH here\nline 4\nline 5\n",
        )
        .unwrap();

        let tool = create_grep_tool();
        let args = serde_json::json!({
            "pattern": "MATCH",
            "path": file_path.to_str().unwrap(),
            "context_before": 1,
            "context_after": 1
        });
        let result = (tool.execute)(args).await.unwrap();
        assert!(result.contains(">3:MATCH here"));
        assert!(result.contains(" 2:line 2"));
        assert!(result.contains(" 4:line 4"));
    }

    #[tokio::test]
    async fn test_todo_write() {
        let ctx = Arc::new(ToolContext::default());
        let tool = create_todo_write_tool(ctx.clone());

        let args = serde_json::json!({
            "items": [
                {"content": "Task 1", "status": "in_progress", "activeForm": "Working on task 1"},
                {"content": "Task 2", "status": "pending", "activeForm": "Waiting for task 2"},
                {"content": "Task 3", "status": "completed", "activeForm": "Done with task 3"}
            ]
        });
        let result = (tool.execute)(args).await.unwrap();
        assert!(result.contains("[>] Task 1"));
        assert!(result.contains("[ ] Task 2"));
        assert!(result.contains("[x] Task 3"));
        assert!(result.contains("(1/3 done)"));
    }

    #[tokio::test]
    async fn test_todo_write_multiple_in_progress() {
        let ctx = Arc::new(ToolContext::default());
        let tool = create_todo_write_tool(ctx);

        let args = serde_json::json!({
            "items": [
                {"content": "Task 1", "status": "in_progress", "activeForm": "Working 1"},
                {"content": "Task 2", "status": "in_progress", "activeForm": "Working 2"}
            ]
        });
        let result = (tool.execute)(args).await.unwrap();
        assert!(result.contains("Only one task can be in_progress"));
    }

    #[test]
    fn test_html_to_text() {
        let html = "<p>Hello <b>world</b></p><script>evil()</script>";
        let text = html_to_text(html);
        assert!(text.contains("Hello world"));
        assert!(!text.contains("evil"));
        assert!(!text.contains("<script>"));
    }
}
