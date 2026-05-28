// mimo-tui - MCP Client Module
// Model Context Protocol: connect to external tool servers via stdio

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, Mutex};

/// MCP server configuration (from mcp.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// MCP tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub server_name: String,
}

/// JSON-RPC request
#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: i64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC response
#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    id: Option<i64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

/// MCP server connection
struct McpServer {
    #[allow(dead_code)]
    name: String,
    writer_tx: mpsc::Sender<String>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value>>>>>,
    id_counter: AtomicI64,
}

/// MCP Client - manages connections to multiple MCP servers
pub struct McpClient {
    servers: HashMap<String, McpServer>,
    tools: Vec<McpTool>,
}

impl McpClient {
    /// Create and connect to all configured MCP servers
    pub async fn new() -> Self {
        let mut client = Self {
            servers: HashMap::new(),
            tools: Vec::new(),
        };

        match load_mcp_config() {
            Ok(config) => {
                for (name, server_config) in &config {
                    match client.connect_server(name, server_config).await {
                        Ok(()) => eprintln!("MCP: Connected to {}", name),
                        Err(e) => eprintln!("MCP: Failed to connect to {}: {}", name, e),
                    }
                }
            }
            Err(e) => eprintln!("MCP: No config loaded: {}", e),
        }

        client
    }

    /// Connect to a single MCP server via stdio
    async fn connect_server(&mut self, name: &str, config: &McpServerConfig) -> Result<()> {
        let cmd_path = which::which(&config.command)
            .with_context(|| format!("'{}' not found in PATH", config.command))?;

        let mut cmd = Command::new(&cmd_path);
        cmd.args(&config.args);
        for (k, v) in &config.env {
            cmd.env(k, v);
        }
        cmd.stdin(std::process::Stdio::piped())
           .stdout(std::process::Stdio::piped())
           .stderr(std::process::Stdio::null());

        let mut child = cmd.spawn()
            .with_context(|| format!("Failed to spawn '{}'", name))?;

        let stdin = child.stdin.take().context("No stdin")?;
        let stdout = child.stdout.take().context("No stdout")?;

        let pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value>>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Writer task: receives messages via channel, writes to stdin
        let (writer_tx, mut writer_rx) = mpsc::channel::<String>(64);
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(msg) = writer_rx.recv().await {
                let framed = format!("Content-Length: {}\r\n\r\n{}", msg.len(), msg);
                if stdin.write_all(framed.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
            drop(child); // Keep child alive until writer exits
        });

        // Reader task: reads responses from stdout using Content-Length framing
        let pending_clone = pending.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut header_buf = String::new();
            loop {
                header_buf.clear();
                // Read headers
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line).await {
                        Ok(0) => return, // EOF
                        Ok(_) => {}
                        Err(_) => return,
                    }
                    if line.trim().is_empty() {
                        break; // End of headers
                    }
                    header_buf.push_str(&line);
                }

                // Parse Content-Length
                let content_length: usize = header_buf
                    .lines()
                    .find_map(|l| l.strip_prefix("Content-Length:"))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);

                if content_length == 0 {
                    continue;
                }

                // Read exact bytes for the body
                let mut body = vec![0u8; content_length];
                match tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body).await {
                    Ok(_) => {}
                    Err(_) => return,
                }

                let body_str = match String::from_utf8(body) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                match serde_json::from_str::<JsonRpcResponse>(&body_str) {
                    Ok(resp) => {
                        if let Some(id) = resp.id {
                            let mut pending = pending_clone.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                if let Some(err) = resp.error {
                                    let _ = tx.send(Err(anyhow::anyhow!("MCP error {}: {}", err.code, err.message)));
                                } else {
                                    let _ = tx.send(Ok(resp.result.unwrap_or(Value::Null)));
                                }
                            }
                        }
                    }
                    Err(_) => continue,
                }
            }
        });

        let server = McpServer {
            name: name.to_string(),
            writer_tx,
            pending: pending.clone(),
            id_counter: AtomicI64::new(1),
        };

        // Initialize handshake
        let init = server.call("initialize", Some(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "clientInfo": {"name": "mimo-tui", "version": "0.2.0"}
        }))).await?;

        eprintln!("MCP: Server '{}' ({})", name,
            init.get("serverInfo").and_then(|s| s.get("name")).and_then(|n| n.as_str()).unwrap_or("unknown"));

        server.notify("notifications/initialized", None).await?;

        // Discover tools
        if let Ok(result) = server.call("tools/list", None).await {
            if let Some(tools) = result.get("tools").and_then(|t| t.as_array()) {
                for tool in tools {
                    self.tools.push(McpTool {
                        name: tool.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                        description: tool.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                        input_schema: tool.get("inputSchema").cloned().unwrap_or(json!({})),
                        server_name: name.to_string(),
                    });
                }
            }
        }

        self.servers.insert(name.to_string(), server);
        Ok(())
    }

    /// Get all MCP tools as OpenAI-format schemas
    pub fn get_tool_schemas(&self) -> Vec<Value> {
        self.tools.iter().map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": format!("mcp__{}__{}", tool.server_name, tool.name),
                    "description": format!("[MCP:{}] {}", tool.server_name, tool.description),
                    "parameters": tool.input_schema
                }
            })
        }).collect()
    }

    /// Execute an MCP tool
    pub async fn execute_tool(&self, full_name: &str, args: &Value) -> Result<String> {
        // Parse "mcp__servername__toolname"
        let stripped = full_name.strip_prefix("mcp__").unwrap_or(full_name);
        let parts: Vec<&str> = stripped.splitn(2, "__").collect();
        if parts.len() != 2 {
            return Ok(format!("[ERROR] Invalid MCP tool name: {}", full_name));
        }

        let server = self.servers.get(parts[0])
            .ok_or_else(|| anyhow::anyhow!("MCP server '{}' not connected", parts[0]))?;

        let result = server.call("tools/call", Some(json!({
            "name": parts[1],
            "arguments": args
        }))).await?;

        // Extract text from MCP content array
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            let texts: Vec<String> = content.iter().filter_map(|item| {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    item.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            }).collect();
            Ok(texts.join("\n"))
        } else {
            Ok(serde_json::to_string_pretty(&result)?)
        }
    }

    /// Check if a tool name belongs to MCP
    pub fn is_mcp_tool(&self, name: &str) -> bool {
        name.starts_with("mcp__")
    }

    /// Check if a specific MCP tool exists
    #[allow(dead_code)]
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.iter().any(|t| format!("mcp__{}__{}", t.server_name, t.name) == name)
    }

    /// Get connected server count
    pub fn server_count(&self) -> usize {
        self.servers.len()
    }

    /// Get total tool count
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// List server names
    pub fn server_names(&self) -> Vec<&str> {
        self.servers.keys().map(|s| s.as_str()).collect()
    }
}

impl McpServer {
    /// Send JSON-RPC request and wait for response
    async fn call(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.id_counter.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        let msg = serde_json::to_string(&request)?;
        self.writer_tx.send(msg).await
            .map_err(|_| anyhow::anyhow!("MCP server '{}' disconnected", self.name))?;

        // Wait for response with timeout
        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(anyhow::anyhow!("Response channel closed")),
            Err(_) => {
                // Remove from pending
                self.pending.lock().await.remove(&id);
                Err(anyhow::anyhow!("MCP request timed out"))
            }
        }
    }

    /// Send notification (no response expected, no id)
    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        #[derive(Serialize)]
        struct Notification {
            jsonrpc: &'static str,
            method: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            params: Option<Value>,
        }
        let request = Notification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };
        let msg = serde_json::to_string(&request)?;
        self.writer_tx.send(msg).await
            .map_err(|_| anyhow::anyhow!("MCP server disconnected"))?;
        Ok(())
    }
}

/// Load MCP configuration from standard paths
fn load_mcp_config() -> Result<HashMap<String, McpServerConfig>> {
    let paths = [
        dirs::home_dir().map(|h| h.join(".mimo-tui").join("mcp.json")),
        dirs::home_dir().map(|h| h.join(".config").join("mimo-tui").join("mcp.json")),
        Some(PathBuf::from("mcp.json")),
    ];

    for path in paths.iter().flatten() {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let config: HashMap<String, McpServerConfig> = serde_json::from_str(&content)?;
            return Ok(config);
        }
    }

    Ok(HashMap::new())
}
