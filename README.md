     1|# 🐋 mimo-tui
     2|
     3|**Terminal coding agent for Xiaomi MiMo — a fast, lightweight alternative to Claude Code and CodeWhale.**
     4|
     5|[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
     6|[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)
     7|[![Version](https://img.shields.io/badge/version-0.2.0-green.svg)](Cargo.toml)
     8|
     9|```
    10| ███╗   ███╗██╗███╗   ███╗ ██████╗       ████████╗██╗   ██╗██╗
    11| ████╗ ████║██║████╗ ████║██╔═══██╗      ╚══██╔══╝██║   ██║██║
    12| ██╔████╔██║██║██╔████╔██║██║   ██║█████╗   ██║   ██║   ██║██║
    13| ██║╚██╔╝██║██║██║╚██╔╝██║██║   ██║╚════╝   ██║   ╚██╗ ██╔╝██║
    14| ██║ ╚═╝ ██║██║██║ ╚═╝ ██║╚██████╔╝         ██║    ╚████╔╝ ██║
    15| ╚═╝     ╚═╝╚═╝╚═╝     ╚═╝ ╚═════╝          ╚═╝     ╚═══╝  ╚═╝
    16|```
    17|
    18|---
    19|
    20|## Why mimo-tui?
    21|
    22|| Feature | mimo-tui | Claude Code | CodeWhale |
    23||---------|----------|-------------|-----------|
    24|| **Binary size** | **5.7 MB** | 200+ MB (Node.js) | 15 MB |
    25|| **Memory usage** | **~20 MB** | 200+ MB | 50+ MB |
    26|| **TUI mode** | ✅ ratatui | ❌ | ✅ |
    27|| **Web search** | ✅ native | ❌ | ❌ |
    28|| **1M context** | ✅ | 200K | 256K |
    29|| **Deep thinking** | ✅ streaming | ❌ | ❌ |
    30|| **MCP protocol** | ✅ | ✅ | ✅ |
    31|| **Sub-agents** | ✅ 20 parallel | ✅ | ✅ 20 parallel |
    32|| **Git integration** | ✅ side-git | ✅ | ✅ |
    33|| **LSP diagnostics** | ✅ | ✅ | ✅ |
    34|| **Session persistence** | ✅ | ✅ | ✅ |
    35|| **First-run wizard** | ✅ interactive | ❌ | ❌ |
| **Permission modes** | ✅ plan/agent/yolo | ✅ | ✅ |
    36|| **Language** | Rust 🦀 | TypeScript | Rust 🦀 |
    37|
    38|---
    39|
    40|## Quick Start
    41|
    42|### Install
    43|
    44|```bash
    45|# npm (recommended — auto-downloads binary)
    46|npm install -g @cyf666nb/mimo-code
    47|
    48|# Or from crates.io
    49|cargo install mimo-tui
    50|
    51|# Or from source
    52|git clone https://github.com/cyf666nb/mimo-tui.git
    53|cd mimo-tui
    54|cargo build --release
    55|sudo cp target/release/mimo-tui /usr/local/bin/
    56|```
    57|
    58|### First Run
    59|
    60|Just launch `mimo-code` — the setup wizard will walk you through:
    61|
    62|```
    63|$ mimo-code
    64|
    65|  ╔══════════════════════════════════════════════════╗
    66|  ║   ███╗   ███╗ ██╗ ███╗   ███╗  ██████╗          ║
    67|  ║   Terminal Coding Agent                     v0.2 ║
    68|  ╚══════════════════════════════════════════════════╝
    69|
    70|  Welcome! Let's set up your AI coding agent.
    71|
    72|  ┌─ Choose your AI provider ─────────────────────┐
    73|  │  1. MiMo (TokenPlan)     best value           │
    74|  │  2. MiMo (Official)      official API          │
    75|  │  3. OpenAI               GPT-4o, o1, o3        │
    76|  │  4. DeepSeek             V3/R1 reasoning       │
    77|  │  5. OpenRouter           200+ models            │
    78|  │  6. Custom               any OpenAI-compatible  │
    79|  └───────────────────────────────────────────────┘
    80|```
    81|
    82|Config saved to `~/.mimo-tui/config.toml`. Reconfigure anytime with `/config`.
    83|
    84|### Manual Configure
    85|
    86|```bash
    87|# Set your API key
    88|export MIMO_API_KEY="your-key-here"
    89|
    90|# Or create config file
    91|mkdir -p ~/.mimo-tui
    92|cat > ~/.mimo-tui/config.toml << 'EOF'
    93|api_key = "your-key-here"
    94|model = "MiMo-7B-RL"
    95|base_url = "https://api.xiaomimimo.com/v1"
    96|thinking = true
    97|web_search = true
    98|permission_mode = "agent"
    99|EOF
   100|```
   101|
   102|### Run
   103|
   104|```bash
   105|# Interactive TUI mode (default)
   106|mimo-tui
   107|
   108|# One-shot command
   109|mimo-tui -c "fix all warnings in this project"
   110|
   111|# With web search
   112|mimo-tui --search -c "what's the latest Rust async runtime benchmarks"
   113|
   114|# Simple terminal mode (no TUI)
   115|mimo-tui --simple
   116|
   117|# Resume a session
   118|mimo-tui --resume <session-id>
   119|```
   120|
   121|---
   122|
   123|## Features
   124|
   125|### 🖥️ TUI Mode
   126|Full-screen terminal interface with:
   127|- Real-time streaming with syntax highlighting
   128|- Thinking/reasoning display (collapsible)
   129|- Mouse support and keyboard navigation
   130|- Dark theme optimized for terminals
   131|
   132|### 🔧 Tool System
   133|Built-in tools for autonomous coding:
   134|- `shell` — Execute shell commands with timeout
   135|- `read_file` / `write_file` / `edit_file` — File operations with line numbers
   136|- `grep` — Regex search with glob filters
   137|- `list_dir` / `project_index` — Project exploration
   138|
   139|### 🤖 Sub-agents
   140|Spawn parallel workers for complex tasks:
   141|```
   142|/agent fix all warnings in src/main.rs | add tests for subagent module | refactor session.rs
   143|```
   144|
   145|### 🔌 MCP (Model Context Protocol)
   146|Connect to external tool servers:
   147|```json
   148|// ~/.mimo-tui/mcp.json
   149|{
   150|  "filesystem": {
   151|    "command": "npx",
   152|    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
   153|  }
   154|}
   155|```
   156|
   157|### 🔒 Permission Modes
   158|- **plan** — Read-only, suggestions only
   159|- **agent** — Ask before modifications (default)
   160|- **yolo** — Full autonomy (use with caution)
   161|
   162|### 💾 Session Management
   163|```bash
   164|/save              # Save current session
   165|/sessions          # List saved sessions
   166|--resume <id>      # Resume a session
   167|```
   168|
   169|### 📸 Git Snapshots
   170|Automatic snapshots before tool calls:
   171|```
   172|/snapshot [label]  # Take a manual snapshot
   173|/snapshots         # List snapshots
   174|/snapshot-restore   # Restore a snapshot
   175|```
   176|
   177|---
   178|
   179|## Configuration
   180|
   181|### Environment Variables
   182|| Variable | Description |
   183||----------|-------------|
   184|| `MIMO_API_KEY` | API key (required) |
   185|| `MIMO_MODEL` | Model name |
   186|| `MIMO_BASE_URL` | API base URL |
   187|| `MIMO_THINKING` | Enable thinking (true/false) |
   188|| `MIMO_SEARCH` | Enable web search (true/false) |
   189|
   190|### Config File
   191|`~/.mimo-tui/config.toml`:
   192|```toml
   193|api_key = "your-key"
   194|model = "MiMo-7B-RL"
   195|base_url = "https://api.xiaomimimo.com/v1"
   196|thinking = true
   197|web_search = true
   198|permission_mode = "agent"
   199|max_output_tokens = 32768
   200|system_prompt_extra = ""
   201|```
   202|
   203|---
   204|
   205|## Keyboard Shortcuts (TUI Mode)
   206|
   207|| Key | Action |
   208||-----|--------|
   209|| `Tab` | Expand/collapse thinking |
   210|| `Ctrl+C` | Quit |
   211|| `Ctrl+L` | Clear chat |
   212|| `Ctrl+K` | Clear input |
   213|| `↑/↓` | Scroll history |
   214|| `PgUp/PgDn` | Fast scroll |
   215|| `Enter` | Submit |
   216|| `Backspace` | Delete char |
   217|
   218|---
   219|
   220|## Architecture
   221|
   222|```
   223|src/
   224|├── main.rs          # CLI entry, agent loop, command dispatch
   225|├── api.rs           # SSE streaming client, MiMoClient
   226|├── tools.rs         # Built-in tool definitions and execution
   227|├── tui.rs           # ratatui TUI interface
   228|├── config.rs        # TOML configuration
   229|├── constitution.rs  # System prompt templates
   230|├── git.rs           # Side-git snapshots
   231|├── highlight.rs     # Syntect syntax highlighting
   232|├── lsp.rs           # LSP diagnostics integration
   233|├── mcp.rs           # MCP client (JSON-RPC 2.0 over stdio)
   234|├── session.rs       # Session persistence (JSON)
   235|└── subagent.rs      # Parallel sub-agent execution
   236|```
   237|
   238|---
   239|
   240|## Contributing
   241|
   242|1. Fork the repository
   243|2. Create a feature branch (`git checkout -b feature/amazing`)
   244|3. Commit your changes (`git commit -m 'Add amazing feature'`)
   245|4. Push to the branch (`git push origin feature/amazing`)
   246|5. Open a Pull Request
   247|
   248|### Development
   249|```bash
   250|# Build
   251|cargo build
   252|
   253|# Run tests
   254|cargo test
   255|
   256|# Check formatting
   257|cargo fmt --check
   258|
   259|# Lint
   260|cargo clippy
   261|```
   262|
   263|---
   264|
   265|## License
   266|
   267|MIT License — see [LICENSE](LICENSE) for details.
   268|
   269|---
   270|
   271|## Acknowledgments
   272|
   273|- [MiMo](https://github.com/xiaomi/mimo) — The AI model powering this agent
   274|- [CodeWhale](https://github.com/nicepkg/codewhale) — Architecture inspiration
   275|- [ratatui](https://ratatui.rs/) — TUI framework
   276|- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) — Feature reference
   277|