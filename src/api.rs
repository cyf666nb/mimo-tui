use anyhow::Result;
use futures::StreamExt;
use reqwest_eventsource::{Event, RequestBuilderExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::Config;
use crate::constitution::{get_system_prompt, get_mode_instruction};
use crate::tools::get_tool_schemas;

// ── MiMo-specific constants ──────────────────────────────────────────
/// Max chars per tool result before truncation (saves tokens, MiMo has 1M context
/// but large tool results bloat the prompt and reduce cache hit rate)
const MAX_TOOL_RESULT_CHARS: usize = 30_000;
/// Token budget for message history (~800K of 1M context for history)
const MAX_HISTORY_TOKENS: usize = 800_000;
/// Rough chars-per-token estimate for Chinese + code mix
const CHARS_PER_TOKEN: f64 = 2.5;
/// Retry config for transient API errors
const MAX_RETRIES: u32 = 3;
const RETRY_BASE_MS: u64 = 1000;

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

    /// Token-budget based trimming (MiMo-optimized).
    /// Estimates token count from character count and trims oldest messages
    /// to stay within the budget. Preserves system prompt and recent context.
    /// Unlike fixed message-count trimming, this adapts to variable-length
    /// tool results and conversation patterns.
    fn trim_messages(&mut self) {
        if self.messages.len() <= 2 { return; }

        let max_tokens = MAX_HISTORY_TOKENS;
        let est_tokens: usize = self.messages.iter()
            .map(|m| {
                let chars = m.content.as_ref().map_or(0, |c| c.len())
                    + m.reasoning_content.as_ref().map_or(0, |r| r.len());
                (chars as f64 / CHARS_PER_TOKEN) as usize
            })
            .sum();

        if est_tokens <= max_tokens { return; }

        // Keep system message (index 0) and trim from oldest non-system messages
        let system = self.messages.first().filter(|m| m.role == "system").cloned();
        let mut kept: Vec<Message> = Vec::new();
        let mut token_budget = max_tokens;

        // Walk backwards from newest, keep messages until budget exhausted
        for msg in self.messages.iter().rev() {
            if msg.role == "system" { continue; }
            let chars = msg.content.as_ref().map_or(0, |c| c.len())
                + msg.reasoning_content.as_ref().map_or(0, |r| r.len());
            let tokens = (chars as f64 / CHARS_PER_TOKEN) as usize;
            if tokens > token_budget && !kept.is_empty() { break; }
            token_budget = token_budget.saturating_sub(tokens);
            kept.push(msg.clone());
        }
        kept.reverse();

        self.messages.clear();
        if let Some(sys) = system {
            self.messages.push(sys);
        }
        self.messages.extend(kept);
    }

    /// Add a tool result, with automatic truncation for large outputs.
    /// MiMo has 1M context but huge tool results bloat the prompt,
    /// reduce cache hit rate, and waste tokens.
    pub fn add_tool_result(&mut self, call_id: String, result: String) {
        let truncated = if result.len() > MAX_TOOL_RESULT_CHARS {
            let head = &result[..MAX_TOOL_RESULT_CHARS.min(result.len())];
            format!("{}...[truncated {} chars → {} total]", head, result.len() - MAX_TOOL_RESULT_CHARS, result.len())
        } else {
            result
        };
        self.messages.push(Message {
            role: "tool".into(),
            content: Some(truncated),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(call_id),
        });
    }

    fn build_request(&self) -> Value {
        let mut tools = get_tool_schemas(self.config.web_search);
        tools.extend(self.extra_tools.iter().cloned());

        // Build messages array with mode instruction injected before last user message.
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
            // CRITICAL for MiMo: reasoning_content MUST be preserved in multi-turn
            // tool-calling conversations. Omitting it causes 400 errors.
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

    /// Stream a single API turn with automatic retry on transient errors.
    /// Retries on: 429 (rate limit), 500 (server), 503 (overloaded).
    /// Does NOT execute tools — the caller is responsible for that.
    pub async fn stream_turn(&mut self) -> Result<(Vec<StreamEvent>, TurnResult)> {
        let body = self.build_request();
        let url = format!("{}/chat/completions", self.config.base_url);

        // Retry loop for transient errors
        let mut last_err = String::new();
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay_ms = RETRY_BASE_MS * 2u64.pow(attempt - 1);
                eprintln!("\n⏳ Retry {}/{} after {}ms: {}", attempt + 1, MAX_RETRIES, delay_ms, last_err);
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }

            let es_result = self.http
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .eventsource();

            let mut es = match es_result {
                Ok(es) => es,
                Err(e) => {
                    last_err = format!("Request error: {}", e);
                    continue;
                }
            };

            let mut events = Vec::new();
            let mut full_content = String::new();
            let mut full_reasoning = String::new();
            let mut tool_call_map: std::collections::HashMap<usize, Value> = std::collections::HashMap::new();
            let mut should_retry = false;

            while let Some(event) = es.next().await {
                match event {
                    Ok(Event::Open) => {}
                    Ok(Event::Message(msg)) => {
                        if msg.data == "[DONE]" { break; }
                        if let Ok(chunk) = serde_json::from_str::<Value>(&msg.data) {
                            // Check for API errors in streaming
                            if let Some(error) = chunk.get("error") {
                                let code = error.get("code").and_then(|v| v.as_u64()).unwrap_or(0);
                                let message = error.get("message").and_then(|v| v.as_str()).unwrap_or("unknown");
                                if code == 429 || code == 500 || code == 503 {
                                    last_err = format!("API {}: {}", code, message);
                                    should_retry = true;
                                    break;
                                }
                                events.push(StreamEvent::Error(format!("API {}: {}", code, message)));
                                break;
                            }

                            // Usage tracking
                            if let Some(usage) = chunk.get("usage") {
                                if let Some(pt) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                                    self.total_input_tokens += pt;
                                }
                                if let Some(ct) = usage.get("completion_tokens").and_then(|v| v.as_u64()) {
                                    self.total_output_tokens += ct;
                                }
                                // MiMo cache: prompt_tokens_details.cached_tokens
                                if let Some(details) = usage.get("prompt_tokens_details") {
                                    if let Some(cached) = details.get("cached_tokens").and_then(|v| v.as_u64()) {
                                        self.total_cache_hit_tokens += cached;
                                        // Miss = total input - cached
                                        if let Some(pt) = usage.get("prompt_tokens").and_then(|v| v.as_u64()) {
                                            if pt > cached {
                                                self.total_cache_miss_tokens += pt - cached;
                                            }
                                        }
                                    }
                                }
                                // DeepSeek cache fields (fallback)
                                if let Some(cht) = usage.get("prompt_cache_hit_tokens").and_then(|v| v.as_u64()) {
                                    if self.total_cache_hit_tokens == 0 { self.total_cache_hit_tokens += cht; }
                                }
                                if let Some(cmt) = usage.get("prompt_cache_miss_tokens").and_then(|v| v.as_u64()) {
                                    if self.total_cache_miss_tokens == 0 { self.total_cache_miss_tokens += cmt; }
                                }
                            }

                            if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
                                if let Some(choice) = choices.first() {
                                    let default_delta = json!({});
                                    let delta = choice.get("delta").unwrap_or(&default_delta);

                                    // Reasoning content (MiMo: MUST preserve for multi-turn tool-calling)
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
                                                if !id.is_empty() { entry["id"] = json!(id); }
                                            }
                                            if let Some(func) = tc_delta.get("function") {
                                                if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                                                    if !name.is_empty() { entry["function"]["name"] = json!(name); }
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
                        last_err = format!("SSE error: {}", e);
                        should_retry = true;
                        break;
                    }
                }
            }
            es.close();

            if should_retry { continue; }

            // Collect tool calls in order
            let mut sorted_indices: Vec<usize> = tool_call_map.keys().cloned().collect();
            sorted_indices.sort();
            let mut tool_calls = Vec::new();
            for idx in sorted_indices {
                if let Some(tc) = tool_call_map.remove(&idx) {
                    let id = tc["id"].as_str().unwrap_or("").to_string();
                    let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                    let arguments = tc["function"]["arguments"].as_str().unwrap_or("{}").to_string();
                    events.push(StreamEvent::ToolCall { id: id.clone(), name, arguments });
                    tool_calls.push(tc);
                }
            }

            events.push(StreamEvent::Done);

            // Build assistant message — reasoning_content is ALWAYS preserved
            // (MiMo requires it in multi-turn tool-calling or gets 400 error)
            let assistant_msg = Message {
                role: "assistant".into(),
                content: Some(full_content.clone()),
                reasoning_content: if full_reasoning.is_empty() { None } else { Some(full_reasoning) },
                tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls.clone()) },
                tool_call_id: None,
            };
            let msg_for_result = assistant_msg.clone();
            self.messages.push(assistant_msg);

            return Ok((events, TurnResult {
                content: full_content,
                has_tool_calls: !tool_calls.is_empty(),
                tool_calls,
                assistant_message: msg_for_result,
            }));
        }

        // All retries exhausted
        anyhow::bail!("API failed after {} retries: {}", MAX_RETRIES, last_err)
    }
}

pub struct TurnResult {
    #[allow(dead_code)] pub content: String,
    pub has_tool_calls: bool,
    #[allow(dead_code)] pub tool_calls: Vec<Value>,
    pub assistant_message: Message,
}
