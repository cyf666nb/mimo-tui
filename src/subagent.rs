// mimo-tui - Sub-agent Module
// Spawn multiple parallel agent workers for concurrent task execution

use crate::config::Config;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex, Semaphore};

const MAX_CONCURRENT: usize = 20;

/// Sub-agent task definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentTask {
    pub id: String,
    pub description: String,
    pub model: Option<String>,
    pub max_turns: Option<usize>,
}

/// Sub-agent result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub turns: usize,
    pub error: Option<String>,
}

/// Spawn multiple sub-agents in parallel
pub async fn run_parallel(
    config: &Config,
    tasks: Vec<SubAgentTask>,
    workdir: &PathBuf,
    progress_tx: mpsc::Sender<(String, String)>,
) -> Vec<SubAgentResult> {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let results = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for task in tasks {
        let sem = semaphore.clone();
        let config = config.clone();
        let tools = crate::tools::get_tool_schemas(config.web_search);
        let workdir = workdir.clone();
        let progress = progress_tx.clone();
        let results = results.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let task_id = task.id.clone();
            let _ = progress.send((task_id.clone(), "started".to_string())).await;

            let result = run_single(&config, &tools, &workdir, &task).await;
            let _ = progress.send((task_id.clone(), if result.success { "done" } else { "failed" }.to_string())).await;
            results.lock().await.push(result);
        });

        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    Arc::try_unwrap(results).map(|m| m.into_inner()).unwrap_or_default()
}

/// Run a single sub-agent loop (non-streaming)
async fn run_single(
    config: &Config,
    tools: &[Value],
    workdir: &PathBuf,
    task: &SubAgentTask,
) -> SubAgentResult {
    let task_id = task.id.clone();
    match run_single_inner(config, tools, workdir, task).await {
        Ok(r) => r,
        Err(e) => SubAgentResult {
            task_id,
            success: false,
            output: String::new(),
            turns: 0,
            error: Some(e.to_string()),
        },
    }
}

async fn run_single_inner(
    config: &Config,
    tools: &[Value],
    workdir: &PathBuf,
    task: &SubAgentTask,
) -> Result<SubAgentResult> {
    let client = reqwest::Client::new();
    let model = task.model.as_deref().unwrap_or(&config.model);
    let max_turns = task.max_turns.unwrap_or(8);

    let system_prompt = format!(
        "You are a sub-agent. Task: {}\nWorking dir: {}\nExecute autonomously. Use tools as needed. Output a concise summary when done.",
        task.description, workdir.display()
    );

    let mut messages: Vec<Value> = vec![
        json!({"role": "system", "content": system_prompt}),
        json!({"role": "user", "content": task.description}),
    ];

    let mut turn = 0;
    while turn < max_turns {
        turn += 1;

        let body = json!({
            "model": model,
            "messages": messages,
            "max_completion_tokens": config.max_output_tokens,
            "stream": false,
            "tools": tools,
            "thinking": {"type": "disabled"},
        });

        let url = format!("{}/chat/completions", config.base_url);
        let resp = client.post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let resp_json: Value = resp.json().await?;

        // Handle API errors
        if let Some(err) = resp_json.get("error") {
            return Ok(SubAgentResult {
                task_id: task.id.clone(),
                success: false,
                output: String::new(),
                turns: turn,
                error: Some(err["message"].as_str().unwrap_or("API error").to_string()),
            });
        }

        let choice = match resp_json.get("choices").and_then(|c| c.as_array()).and_then(|a| a.first()) {
            Some(c) => c,
            None => return Ok(SubAgentResult {
                task_id: task.id.clone(),
                success: false,
                output: String::new(),
                turns: turn,
                error: Some(format!("API returned no choices: {}", resp_json)),
            }),
        };
        let message = &choice["message"];

        // Check for tool calls
        if let Some(tool_calls) = message.get("tool_calls").and_then(|tc| tc.as_array()) {
            if !tool_calls.is_empty() {
                // Add assistant message with tool calls
                messages.push(message.clone());

                // Execute each tool
                for tc in tool_calls {
                    let call_id = tc["id"].as_str().unwrap_or("").to_string();
                    let func_name = tc["function"]["name"].as_str().unwrap_or("");
                    let func_args = tc["function"]["arguments"].as_str().unwrap_or("{}");

                    let args: Value = serde_json::from_str(func_args).unwrap_or(json!({}));
                    let output = crate::tools::execute_tool(func_name, &args);

                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": output,
                    }));
                }
                continue;
            }
        }

        // No tool calls — done
        let output = message["content"].as_str().unwrap_or("").to_string();
        return Ok(SubAgentResult {
            task_id: task.id.clone(),
            success: true,
            output,
            turns: turn,
            error: None,
        });
    }

    Ok(SubAgentResult {
        task_id: task.id.clone(),
        success: false,
        output: "Max turns exceeded".into(),
        turns: turn,
        error: Some("Reached max turns limit".into()),
    })
}

/// Parse parallel tasks from user input
pub fn parse_tasks(input: &str) -> Vec<SubAgentTask> {
    // Split on pipe
    if input.contains(" | ") {
        return input.split(" | ").enumerate().map(|(i, t)| SubAgentTask {
            id: format!("task_{}", i + 1),
            description: t.trim().to_string(),
            model: None,
            max_turns: Some(5),
        }).collect();
    }

    // Split on bullet points
    let lines: Vec<&str> = input.lines().collect();
    let mut tasks = Vec::new();
    let mut current = String::new();

    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            if !current.is_empty() {
                tasks.push(current.trim().to_string());
                current.clear();
            }
            current.push_str(&trimmed[2..]);
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        tasks.push(current.trim().to_string());
    }

    if tasks.len() > 1 {
        tasks.into_iter().enumerate().map(|(i, t)| SubAgentTask {
            id: format!("task_{}", i + 1),
            description: t,
            model: None,
            max_turns: Some(5),
        }).collect()
    } else {
        vec![SubAgentTask {
            id: "task_1".to_string(),
            description: input.to_string(),
            model: None,
            max_turns: Some(5),
        }]
    }
}

/// Format results for display
pub fn format_results(results: &[SubAgentResult]) -> String {
    let mut out = String::new();
    let success = results.iter().filter(|r| r.success).count();
    out.push_str(&format!("## Sub-agents: {}/{} completed\n\n", success, results.len()));

    for r in results {
        let icon = if r.success { "✓" } else { "✗" };
        out.push_str(&format!("### {} {}\n", icon, r.task_id));
        if let Some(err) = &r.error {
            out.push_str(&format!("**Error:** {}\n", err));
        }
        let display = if r.output.len() > 500 {
            let end = r.output.char_indices().nth(500).map(|(i, _)| i).unwrap_or(r.output.len());
            format!("{}…", &r.output[..end])
        } else {
            r.output.clone()
        };
        out.push_str(&format!("{}\n*{} turns*\n\n", display, r.turns));
    }

    out
}
