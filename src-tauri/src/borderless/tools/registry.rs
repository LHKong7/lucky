//! Tool registry — immutable lookup map of registered tools.

use std::collections::HashMap;

use crate::borderless::agent_core::ToolDefinition;

/// Immutable registry of tools available to the agent.
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites if a tool with the same name exists.
    pub fn register(&mut self, tool: ToolDefinition) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Check if a tool exists by name.
    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// List all tool names.
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|k| k.as_str()).collect()
    }

    /// Get the full map (for converting to OpenAI tool format, etc.).
    pub fn as_map(&self) -> &HashMap<String, ToolDefinition> {
        &self.tools
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a tool definition to OpenAI function-calling format.
pub fn tool_to_openai_format(tool: &ToolDefinition) -> serde_json::Value {
    let mut properties = serde_json::Map::new();

    if let Some(ref params) = tool.parameters {
        for (name, param) in params {
            let mut prop = serde_json::Map::new();
            prop.insert("type".into(), serde_json::json!(param.param_type));
            if let Some(ref desc) = param.description {
                prop.insert("description".into(), serde_json::json!(desc));
            }
            if let Some(ref enums) = param.enum_values {
                prop.insert("enum".into(), serde_json::json!(enums));
            }
            properties.insert(name.clone(), serde_json::Value::Object(prop));
        }
    }

    serde_json::json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": tool.required,
            }
        }
    })
}
