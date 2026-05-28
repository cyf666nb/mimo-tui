use anyhow::Result;
use futures::StreamExt;
use reqwest_eventsource::{Event, RequestBuilderExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::Config;
use crate::constitution::{get_system_prompt, get_mode_instruction};
use crate::tools::get_tool_schemas;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

pub struct MiMoClient {
    pub config: Config,
    pub messages: Vec<Message>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_reasoning_tokens: u64,
    pub total_cache_hit_tokens: u64,
    pub total_cache_miss_tokens: u64,
    pub extra_tools: Vec<Value>,
    pub http: reqwest::Client,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Thinking(String),
    Content(String),
    ToolCall { id: String, name: String, arguments: String },
    Error(String),
    Done,
    AssistantMessage(Message),
}

impl MiMoClient {
    pub fn new(config: Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self {
            config,
            messages: Vec::new(),
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_reasoning_tokens: 0,
            total_cache_hit_tokens: 0,
            total_cache_miss_tokens: 0,
            extra_tools: Vec::new(),
            http,
        }
    }

    pub fn reset(&mut self) {
        self.messages.clear();
        self.total_input_tokens = 0;
        self.total_output_tokens = 0;
        self.total_reasoning_tokens = 0;
        self.total_cache_hit_tokens = 0;
        self.total_cache_miss_tokens = 0;
    }

    pub fn add_system_prompt(&mut self) {
        let prompt = get_system_prompt(&self.config.permission_mode, &self.config.system_prompt_extra);
        if !self.messages.is_empty() && self.messages[0].role == "system" {
            self.messages[0].content = Some(prompt);
        } else {
            self.messages.insert(0, Message {
                role: "system".into(),
                content: Some(prompt),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
            });
        }
    }

    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(Message {
            role: "user".into(),
            content: Some(content),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
        });
        self.trim_messages();
    }

    /// Trim message history to prevent unbounded memory growth.
    /// Keeps system prompt + last 100 messages (tool results can be large).
    fn trim_messages(&mut self) {
        const MAX_MESSAGES: usize = 100;
        if self.messages.len() > MAX_MESSAGES + 1 {
            // Keep system message (index 0) and last MAX_MESSAGES
            let system = self.messages.first().filter(|m| m.role == "system").cloned();
            let keep_from = self.messages.len() - MAX_MESSAGES;
            let kept: Vec<Message> = self.messages[keep_from..].to_vec();
            self.messages.clear();
            if let Some(sys) = system {
                self.messages.push(sys);
            }
            self.messages.extend(kept);
        }
    }

    pub fn add_tool_result(&mut self, call_id: String, result: String) {
        self.messages.push(Message {
            role: "tool".into(),
            content: Some(result),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(call_id),
        });
    }

    fn build_request(&self) -> Value {
        let mut tools = get_tool_schemas(self.config.web_search);
        tools.extend(self.extra_tools.iter().cloned());

        // Build messages array with mode instruction injected before last user message
        // This maximizes prefix cache hits: system prompt + history stays stable,
        // only the mode instruction changes when switching modes.
        let mut messages: Vec<Value> = Vec::new();
        let mode_inst = get_mode_instruction(&self.config.permission_mode);

        for (i, m) in self.messages.iter().enumerate() {
            // Inject mode instruction as system message before the LAST user message
            if m.role == "user" && i == self.messages.len() - 1 {
                messages.push(json!({
                    "role": "system",
                    "content": mode_inst
                }));
            }
            let mut msg = json!({ "role": m.role });
            if let Some(ref content) = m.content {
                msg["content"] = json!(content);
            }
            if let Some(ref rc) = m.reasoning_content {
                msg["reasoning_content"] = json!(rc);
            }
            if let Some(ref tc) = m.tool_calls {
                msg["tool_calls"] = json!(tc);
            }
            if let Some(ref tid) = m.tool_call_id {
                msg["tool_call_id"] = json!(tid);
            }
            messages.push(msg);
        }

        // thinking goes in extra_body (MiMo API requirement)
        let thinking = if self.config.thinking {
            json!({"type": "enabled"})
        } else {
            json!({"type": "disabled"})
        };

        let mut body = json!({
            "model": self.config.model,
            "messages": messages,
            "max_completion_tokens": self.config.max_output_tokens,
            "stream": true,
            "tools": tools,
        });
        body["thinking"] = thinking;
        body
    }

    /// Stream a single API turn. Returns events as a Vec.
    /// Does NOT execute tools — the caller is responsible for that.
    pub async fn stream_turn(&mut self) -> Result<(Vec<StreamEvent>, TurnResult)> {
        let body = self.build_request();
        let url = format!("{}/chat/completions", self.config.base_url);

        let mut es = self.http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .eventsource()?;

        let mut events = Vec::new();
        let mut full_content = String::new();
        let mut full_reasoning = String::new();
        let mut tool_call_map: std::collections::HashMap<usize, Value> = std::collections::HashMap::new();

        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Open) => {}
                Ok(Event::Message(msg)) => {
                    if msg.data == "[DONE]" {
                        break;
                    }
                    if let Ok(chunk) = serde_json::from_str::<Value>(&msg.data) {
                        // Usage
                        if let Some(usage) = chunk.get("usage") {
                            if let Some(pt) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                                self.total_input_tokens += pt;
                            }
                            if let Some(ct) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                                self.total_output_tokens += ct;
                            }
                            // DeepSeek / MiMo cache hit tracking
                            if let Some(cht) = usage.get("prompt_cache_hit_tokens").and_then(|v| v.as_u64()) {
                                self.total_cache_hit_tokens += cht;
                            }
                            if let Some(cmt) = usage.get("prompt_cache_miss_tokens").and_then(|v| v.as_u64()) {
                                self.total_cache_miss_tokens += cmt;
                            }
                        }

                        if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                let default_delta = json!({});
                                let delta = choice.get("delta").unwrap_or(&default_delta);

                                // Reasoning content
                                if let Some(rc) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                                    if !rc.is_empty() {
                                        full_reasoning.push_str(rc);
                                        events.push(StreamEvent::Thinking(rc.to_string()));
                                    }
                                }

                                // Content
                                if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                    if !content.is_empty() {
                                        full_content.push_str(content);
                                        events.push(StreamEvent::Content(content.to_string()));
                                    }
                                }

                                // Tool calls
                                if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                                    for tc_delta in tcs {
                                        let idx = tc_delta.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                        let entry = tool_call_map.entry(idx).or_insert_with(|| {
                                            json!({"id": "", "function": {"name": "", "arguments": ""}})
                                        });

                                        if let Some(id) = tc_delta.get("id").and_then(|v| v.as_str()) {
                                            if !id.is_empty() {
                                                entry["id"] = json!(id);
                                            }
                                        }
                                        if let Some(func) = tc_delta.get("function") {
                                            if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                                if !name.is_empty() {
                                                    entry["function"]["name"] = json!(name);
                                                }
                                            }
                                            if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                                                let existing = entry["function"]["arguments"].as_str().unwrap_or("");
                                                entry["function"]["arguments"] = json!(format!("{}{}", existing, args));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    events.push(StreamEvent::Error(format!("SSE error: {}", e)));
                    break;
                }
            }
        }
        es.close();

        // Collect tool calls in order
        let mut sorted_indices: Vec<usize> = tool_call_map.keys().cloned().collect();
        sorted_indices.sort();
        let mut tool_calls = Vec::new();
        for idx in sorted_indices {
            if let Some(tc) = tool_call_map.remove(&idx) {
                // Emit ToolCall event
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                let arguments = tc["function"]["arguments"].as_str().unwrap_or("{}").to_string();
                events.push(StreamEvent::ToolCall { id: id.clone(), name, arguments });
                tool_calls.push(tc);
            }
        }

        events.push(StreamEvent::Done);

        // Build assistant message with reasoning_content preserved
        let assistant_msg = Message {
            role: "assistant".into(),
            content: Some(full_content.clone()),
            reasoning_content: if full_reasoning.is_empty() { None } else { Some(full_reasoning) },
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls.clone()) },
            tool_call_id: None,
        };
        let msg_for_result = assistant_msg.clone();
        self.messages.push(assistant_msg);

        Ok((events, TurnResult {
            content: full_content,
            has_tool_calls: !tool_calls.is_empty(),
            tool_calls,
            assistant_message: msg_for_result,
        }))
    }
}

pub struct TurnResult {
    #[allow(dead_code)] pub content: String,
    pub has_tool_calls: bool,
    #[allow(dead_code)] pub tool_calls: Vec<Value>,
    pub assistant_message: Message,
}
