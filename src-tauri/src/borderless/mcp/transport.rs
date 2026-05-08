//! MCP transport implementations (stdio and HTTP).

use async_trait::async_trait;
use serde_json;

use super::protocol::{JsonRpcRequest, JsonRpcResponse};

/// Transport layer for MCP communication.
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&mut self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, String>;
    async fn close(&mut self) -> Result<(), String>;
}

/// Stdio-based transport (spawns a child process).
pub struct StdioTransport {
    child: Option<tokio::process::Child>,
    stdin: Option<tokio::process::ChildStdin>,
    stdout_reader: Option<tokio::io::BufReader<tokio::process::ChildStdout>>,
}

impl StdioTransport {
    pub async fn new(command: &str, args: &[String], env: &std::collections::HashMap<String, String>) -> Result<Self, String> {
        use tokio::process::Command;

        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn MCP server: {}", e))?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take().map(tokio::io::BufReader::new);

        Ok(Self {
            child: Some(child),
            stdin,
            stdout_reader: stdout,
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&mut self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, String> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

        let stdin = self.stdin.as_mut().ok_or("Stdin not available")?;
        let reader = self.stdout_reader.as_mut().ok_or("Stdout not available")?;

        let payload = serde_json::to_string(request).map_err(|e| e.to_string())?;
        stdin
            .write_all(format!("{}\n", payload).as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;

        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(|e| e.to_string())?;

        serde_json::from_str(&line).map_err(|e| format!("Failed to parse MCP response: {}", e))
    }

    async fn close(&mut self) -> Result<(), String> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
        Ok(())
    }
}

/// HTTP-based transport.
pub struct HttpTransport {
    url: String,
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send(&mut self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, String> {
        let resp = self
            .client
            .post(&self.url)
            .json(request)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        resp.json::<JsonRpcResponse>()
            .await
            .map_err(|e| e.to_string())
    }

    async fn close(&mut self) -> Result<(), String> {
        Ok(())
    }
}
