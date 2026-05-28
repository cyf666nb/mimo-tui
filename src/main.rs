mod api;
mod setup;
 mod config;
 mod constitution;
 mod git;
mod highlight;
 mod lsp;
 mod session;
 mod mcp;
mod subagent;
 mod tools;
 mod tui;

use clap::Parser;
use std::io::{self, Write};

use api::{MiMoClient, StreamEvent};
use config::Config;

#[derive(Parser, Debug)]
#[command(name = "mimo", version, about = "MiMo TUI — Terminal Coding Agent for Xiaomi MiMo")]
struct Cli {
    /// One-shot command (non-interactive)
    #[arg(short = 'c', long)]
    command: Option<String>,

    /// Model to use
    #[arg(short, long)]
    model: Option<String>,

    /// Permission mode: plan, agent, yolo
    #[arg(long)]
    mode: Option<String>,

    /// Enable/disable thinking
    #[arg(long)]
    thinking: Option<String>,

    /// Enable web search
    #[arg(long)]
    search: bool,

    /// API key
    #[arg(long)]
    api_key: Option<String>,

    /// Base URL
    #[arg(long)]
    base_url: Option<String>,

    /// Force simple terminal mode (no TUI)
    #[arg(long)]
    simple: bool,

    /// Resume a saved session
    #[arg(long)]
    resume: Option<String>,

    /// List saved sessions
    #[arg(long)]
    sessions: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // First-run setup wizard
    if setup::needs_setup() {
        let config = setup::run_setup();
        // Launch directly after setup
        if let Some(command) = cli.command {
            one_shot(config, &command).await;
        } else if cli.simple {
            simple_interactive(config).await;
        } else {
            if let Err(e) = tui::run_tui(config).await {
                eprintln!("TUI error: {}", e);
            }
        }
        return;
    }

    let mut config = Config::load();

    if let Some(key) = cli.api_key {
        config.api_key = key;
    }
    if let Some(url) = cli.base_url {
        config.base_url = url;
    }
    if let Some(model) = cli.model {
        config.model = model;
    }
    if let Some(mode) = cli.mode {
        config.permission_mode = mode;
    }
    if let Some(thinking) = cli.thinking {
        config.thinking = thinking == "on";
    }
    if cli.search {
        config.web_search = true;
    }

    // List sessions
    if cli.sessions {
        match session::Session::list() {
            Ok(sessions) => {
                if sessions.is_empty() {
                    println!("No saved sessions.");
                } else {
                    println!("Saved sessions:\n");
                    for s in &sessions {
                        println!("  {} │ {} │ {} msgs │ {} │ {}",
                            s.id, s.title, s.message_count, s.model, s.created_at);
                    }
                    println!("\nResume with: mimo --resume <id>");
                }
            }
            Err(e) => eprintln!("Error: {}", e),
        }
        return;
    }

    if config.api_key.is_empty() {
        eprintln!("Error: No API key set.");
        eprintln!("Set MIMO_API_KEY environment variable or use --api-key");
        eprintln!("Get your key at: https://platform.xiaomimimo.com/");
        std::process::exit(1);
    }

    config.save();

    if let Some(command) = cli.command {
        one_shot(config, &command).await;
    } else if let Some(session_id) = cli.resume {
        resume_session(config, &session_id).await;
    } else if cli.simple {
        simple_interactive(config).await;
    } else {
        if let Err(e) = tui::run_tui(config).await {
            eprintln!("TUI error: {}", e);
        }
    }
}

async fn one_shot(config: Config, command: &str) {
    let show_thinking = config.thinking;
    let mcp = mcp::McpClient::new().await;
    let mut client = MiMoClient::new(config);
    client.extra_tools = mcp.get_tool_schemas();
    client.add_system_prompt();
    client.add_user_message(command.to_string());

    let mut thinking_active = false;
    let mut content_active = false;

    for _ in 0..20 {
        let (events, result) = match client.stream_turn().await {
            Ok(r) => r,
            Err(e) => { eprintln!("\n❌ {}", e); break; }
        };

        for event in &events {
            match event {
                StreamEvent::Thinking(text) => {
                    if !show_thinking { continue; }
                    if !thinking_active { eprint!("\n💭 Thinking..."); thinking_active = true; }
                    eprint!("{}", text);
                }
                StreamEvent::Content(text) => {
                    if thinking_active { eprintln!(); thinking_active = false; }
                    if !content_active { print!("\nMiMo: "); content_active = true; }
                    print!("{}", text);
                    let _ = io::stdout().flush();
                }
                StreamEvent::ToolCall { name, id, arguments } => {
                    thinking_active = false; content_active = false;
                    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
                    let args_summary = format_tool_args(&args);
                    eprintln!("\n  ⚡ {}({})", name, args_summary);
                    let tool_result = if mcp.is_mcp_tool(name) {
                        mcp.execute_tool(name, &args).await.unwrap_or_else(|e| format!("MCP Error: {}", e))
                    } else {
                        tools::execute_tool(name, &args)
                    };
                    client.add_tool_result(id.clone(), tool_result);
                }
                StreamEvent::AssistantMessage(msg) => {
                    client.messages.push(msg.clone());
                }
                StreamEvent::Error(msg) => eprintln!("\n❌ {}", msg),
                StreamEvent::Done => {}
            }
        }

        if !result.has_tool_calls { break; }
    }

    if thinking_active { eprintln!(); }
    if content_active { println!(); }
    eprintln!("\nTokens — input: {} | output: {} | reasoning: {}",
        client.total_input_tokens, client.total_output_tokens, client.total_reasoning_tokens);
}

async fn resume_session(config: Config, session_id: &str) {
    let session = match session::Session::load(session_id) {
        Ok(s) => s,
        Err(e) => { eprintln!("Error loading session: {}", e); return; }
    };

    println!("Resuming session: {} ({})", session.title, session.id);
    println!("Model: {} | Messages: {}", session.model, session.messages.len());
    println!();

    let mut client = MiMoClient::new(config);
    client.messages = session.messages;
    client.total_input_tokens = session.total_input_tokens;
    client.total_output_tokens = session.total_output_tokens;

    let mcp = mcp::McpClient::new().await;
    client.extra_tools = mcp.get_tool_schemas();
    simple_interactive_with_client(client, mcp).await;
}

async fn simple_interactive(config: Config) {
    let mut client = MiMoClient::new(config.clone());
    let mcp = mcp::McpClient::new().await;
    client.extra_tools = mcp.get_tool_schemas();
    simple_interactive_with_client(client, mcp).await;
}

async fn simple_interactive_with_client(mut client: MiMoClient, mcp: mcp::McpClient) {
    if client.messages.is_empty() || client.messages[0].role != "system" {
        client.add_system_prompt();
    }

    let git = git::GitOps::new(std::env::current_dir().unwrap_or_default());
    if git.is_git_repo() {
        let _ = git.init();
        let _ = git.snapshot("session-start");
    }

    print_banner();

    loop {
        let mode = client.config.permission_mode.clone();
        print!("\n{} ", mode_prompt(&mode));
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            println!("\nBye! 🐋");
            break;
        }
        let input = input.trim().to_string();
        if input.is_empty() { continue; }

        if input.starts_with('/') {
            handle_command(&input, &mut client, &git, &mcp).await;
            continue;
        }

        client.add_user_message(input);
        let show_thinking = client.config.thinking;
        let mut thinking_active = false;
        let mut content_active = false;

        for _ in 0..20 {
            let (events, result) = match client.stream_turn().await {
                Ok(r) => r,
                Err(e) => { eprintln!("\n❌ {}", e); break; }
            };

            for event in &events {
                match event {
                    StreamEvent::Thinking(text) => {
                        if !show_thinking { continue; }
                        if !thinking_active { eprint!("\n💭 Thinking..."); thinking_active = true; }
                        eprint!("{}", text);
                    }
                    StreamEvent::Content(text) => {
                        if thinking_active { eprintln!(); thinking_active = false; }
                        if !content_active { eprint!("\nMiMo: "); content_active = true; }
                        eprint!("{}", text);
                        let _ = io::stdout().flush();
                    }
                    StreamEvent::ToolCall { name, id, arguments } => {
                        thinking_active = false; content_active = false;
                        let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
                        let args_summary = format_tool_args(&args);
                        eprintln!("\n  ⚡ {}({})", name, args_summary);
                        let tool_result = if mcp.is_mcp_tool(name) {
                            mcp.execute_tool(name, &args).await.unwrap_or_else(|e| format!("MCP Error: {}", e))
                        } else {
                            tools::execute_tool(name, &args)
                        };
                        client.add_tool_result(id.clone(), tool_result.clone());

                        // LSP check after file edits
                        if name == "write_file" || name == "edit_file" {
                            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                                if let Ok(diags) = lsp::quick_check(std::path::Path::new(path)) {
                                    for d in &diags {
                                        eprintln!("  {}", d);
                                    }
                                }
                            }
                        }

                        // Git snapshot after tool execution
                        if git.is_git_repo() {
                            let _ = git.snapshot(&format!("tool:{}", name));
                        }
                    }
                    StreamEvent::AssistantMessage(msg) => {
                        client.messages.push(msg.clone());
                    }
                    StreamEvent::Error(msg) => eprintln!("\n❌ {}", msg),
                    StreamEvent::Done => {}
                }
            }

            if !result.has_tool_calls { break; }
        }

        if content_active { println!(); }

        // Auto-save session
        let session = session::Session::new(
            client.messages.clone(),
            client.config.model.clone(),
            client.total_input_tokens,
            client.total_output_tokens,
        );
        if let Ok(path) = session.save() {
            // Silent save
            let _ = path;
        }
    }
}

async fn handle_command(cmd: &str, client: &mut MiMoClient, git: &git::GitOps, mcp: &mcp::McpClient) {
    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
    let command = parts[0];
    let arg = parts.get(1).unwrap_or(&"");

    match command {
        "/help" | "/h" | "/?" => print_help(),
        "/mode" => {
            if ["plan", "agent", "yolo"].contains(arg) {
                client.config.permission_mode = arg.to_string();
                client.add_system_prompt();
                println!("Mode: {}", arg.to_uppercase());
            } else {
                println!("Usage: /mode <plan|agent|yolo>");
            }
        }
        "/model" => {
            if !arg.is_empty() {
                client.config.model = arg.to_string();
                println!("Model: {}", arg);
            } else {
                println!("Current: {}", client.config.model);
            }
        }
        "/thinking" => {
            if *arg == "on" || *arg == "off" {
                client.config.thinking = *arg == "on";
                println!("Thinking: {}", arg);
            }
        }
        "/clear" => {
            client.reset();
            client.add_system_prompt();
            println!("Cleared.");
        }
        "/tokens" => {
            println!("Tokens — input: {} | output: {} | reasoning: {}",
                client.total_input_tokens, client.total_output_tokens, client.total_reasoning_tokens);
        }
        "/save" => {
            let session = session::Session::new(
                client.messages.clone(),
                client.config.model.clone(),
                client.total_input_tokens,
                client.total_output_tokens,
            );
            match session.save() {
                Ok(path) => println!("Saved session: {} → {}", session.id, path.display()),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        "/sessions" => {
            match session::Session::list() {
                Ok(sessions) => {
                    if sessions.is_empty() {
                        println!("No saved sessions.");
                    } else {
                        for s in &sessions {
                            println!("  {} │ {} │ {} msgs │ {}", s.id, s.title, s.message_count, s.created_at);
                        }
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        "/snapshot" => {
            let label = if arg.is_empty() { "manual" } else { arg };
            match git.snapshot(label) {
                Ok(hash) => println!("Snapshot: {}", hash),
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        "/snapshots" => {
            match git.list(10) {
                Ok(list) => {
                    if list.is_empty() {
                        println!("No snapshots.");
                    } else {
                        for s in &list {
                            println!("  {}", s);
                        }
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        "/git" => {
            match git.status() {
                Ok(status) => {
                    if status.is_empty() {
                        println!("Working tree clean.");
                    } else {
                        println!("{}", status);
                    }
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        "/mcp" => {
            println!("MCP Servers: {} | Tools: {}", mcp.server_count(), mcp.tool_count());
            for name in mcp.server_names() {
                println!("  • {}", name);
            }
            if mcp.tool_count() > 0 {
                println!("
Tools:");
                for schema in mcp.get_tool_schemas() {
                    let name = schema["function"]["name"].as_str().unwrap_or("?");
                    let desc = schema["function"]["description"].as_str().unwrap_or("");
                    let short = if desc.len() > 60 { let end = desc.char_indices().nth(60).map(|(i, _)| i).unwrap_or(desc.len()); format!("{}…", &desc[..end]) } else { desc.to_string() };
                    println!("  {} — {}", name, short);
                }
            }
        }
        "/config" | "/setup" => {
            let new_config = setup::run_setup();
            *client = MiMoClient::new(new_config);
            client.add_system_prompt();
            println!("Config updated!");
        }
        "/quit" | "/exit" => {
            println!("Bye! 🐋");
            std::process::exit(0);
        }
        "/agent" | "/agents" => {
            if arg.is_empty() {
                println!("Usage: /agent <task1> | <task2> | <task3>");
                println!("       /agent multi-line task with - bullet points");
            } else {
                run_subagents(arg, client, git).await;
            }
        }
        _ => println!("Unknown command: {}. Type /help.", command),
    }
}

fn format_tool_args(args: &serde_json::Value) -> String {
    args.as_object()
        .map(|m| {
            m.iter().map(|(k, v)| {
                let default = v.to_string();
                let val = v.as_str().unwrap_or(&default);
                let end = val.char_indices().nth(60).map(|(i, _)| i).unwrap_or(val.len());
                let short = if val.len() > 60 { format!("{}...", &val[..end]) } else { val.to_string() };
                format!("{}={}", k, short)
            }).collect::<Vec<_>>().join(", ")
        })
        .unwrap_or_default()
}

async fn run_subagents(task_str: &str, _client: &MiMoClient, _git: &git::GitOps) {
    let config = Config::load();
    let workdir = std::env::current_dir().unwrap_or_default();

    let tasks = if task_str.contains(" | ") {
        task_str.split(" | ").enumerate().map(|(i, t)| {
            subagent::SubAgentTask {
                id: format!("task_{}", i + 1),
                description: t.trim().to_string(),
                model: None,
                max_turns: Some(5),
            }
        }).collect::<Vec<_>>()
    } else if task_str.contains("\n- ") || task_str.contains("\n* ") {
        subagent::parse_tasks(task_str)
    } else {
        vec![subagent::SubAgentTask {
            id: "task_1".to_string(),
            description: task_str.to_string(),
            model: None,
            max_turns: Some(5),
        }]
    };

    println!("Spawning {} sub-agent(s)...\n", tasks.len());

    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(32);
    let progress_handle = tokio::spawn(async move {
        while let Some((task_id, status)) = progress_rx.recv().await {
            println!("  [{}] {}", task_id, status);
        }
    });

    let results = subagent::run_parallel(&config, tasks, &workdir, progress_tx).await;
    let _ = progress_handle.await;

    println!("{}", subagent::format_results(&results));
}

fn print_banner() {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!("  \x1b[38;5;208;1m             ╭───────────────────────╮");
    println!("             │                       │");
    println!("             │   ███╗   ███╗ ██╗    │");
    println!("             │   ████╗ ████║ ██║    │");
    println!("             │   ██╔████╔██║ ██║    │");
    println!("             │   ██║╚██╔╝██║ ██║    │");
    println!("             │   ██║ ╚═╝ ██║ ██║    │");
    println!("             │   ╚═╝     ╚═╝ ╚═╝    │");
    println!("             │                       │");
    println!("             ╰───────────────────────╯");
    println!("\x1b[0m");
    println!("  \x1b[1;36mMiMo TUI\x1b[0m  \x1b[90mv{}\x1b[0m  \x1b[90m━━━━━━━━━━━━━━━━━━━━━━━━━\x1b[0m", version);
    println!("  \x1b[1m🐋  Terminal Coding Agent\x1b[0m");
    println!("  \x1b[90mPowered by Xiaomi MiMo · Written in Rust\x1b[0m");
    println!();
    println!("  \x1b[90m/help for commands · /quit to exit · /config to reconfigure\x1b[0m");
    println!();
}

fn mode_prompt(mode: &str) -> &'static str {
    match mode {
        "plan" => "📋❯",
        "yolo" => "🔥❯",
        _ => "❯",
    }
}

fn print_help() {
    println!(r#"
Commands:
  /help                  Show this help
  /mode <plan|agent|yolo>  Switch permission mode
  /model <name>          Switch model
  /thinking <on|off>     Toggle thinking
  /clear                 Clear conversation
  /tokens                Show token usage
  /save                  Save session
  /sessions              List saved sessions
  /snapshot [label]      Take git snapshot
  /snapshots             List git snapshots
  /git                   Show git status
  /agent <tasks>         Spawn parallel sub-agents (use | to separate tasks)
  /config                Re-run setup wizard
  /quit                  Exit

Shortcuts (TUI mode):
  Tab        Expand/collapse thinking
  Ctrl+C     Quit
  Ctrl+L     Clear chat
  Ctrl+K     Clear input
  ↑/↓        Scroll
  PgUp/PgDn  Fast scroll
"#);
}
