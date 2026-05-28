# 🐋 mimo-tui

**Terminal coding agent for Xiaomi MiMo — a fast, lightweight alternative to Claude Code and CodeWhale.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)
[![Version](https://img.shields.io/badge/version-0.2.0-green.svg)](Cargo.toml)

```
 ███╗   ███╗██╗███╗   ███╗ ██████╗       ████████╗██╗   ██╗██╗
 ████╗ ████║██║████╗ ████║██╔═══██╗      ╚══██╔══╝██║   ██║██║
 ██╔████╔██║██║██╔████╔██║██║   ██║█████╗   ██║   ██║   ██║██║
 ██║╚██╔╝██║██║██║╚██╔╝██║██║   ██║╚════╝   ██║   ╚██╗ ██╔╝██║
 ██║ ╚═╝ ██║██║██║ ╚═╝ ██║╚██████╔╝         ██║    ╚████╔╝ ██║
 ╚═╝     ╚═╝╚═╝╚═╝     ╚═╝ ╚═════╝          ╚═╝     ╚═══╝  ╚═╝
```

---

## Why mimo-tui?

| Feature | mimo-tui | Claude Code | CodeWhale |
|---------|----------|-------------|-----------|
| **Binary size** | **5.7 MB** | 200+ MB (Node.js) | 15 MB |
| **Memory usage** | **~20 MB** | 200+ MB | 50+ MB |
| **TUI mode** | ✅ ratatui | ❌ | ✅ |
| **Web search** | ✅ native | ❌ | ❌ |
| **1M context** | ✅ | 200K | 256K |
| **Deep thinking** | ✅ streaming | ❌ | ❌ |
| **MCP protocol** | ✅ | ✅ | ✅ |
| **Sub-agents** | ✅ 20 parallel | ✅ | ✅ 20 parallel |
| **Git integration** | ✅ side-git | ✅ | ✅ |
| **LSP diagnostics** | ✅ | ✅ | ✅ |
| **Session persistence** | ✅ | ✅ | ✅ |
| **Permission modes** | ✅ plan/agent/yolo | ✅ | ✅ |
| **Language** | Rust 🦀 | TypeScript | Rust 🦀 |

---

## Quick Start

### Install

```bash
# npm (recommended — auto-downloads binary)
npm install -g mimo-tui

# Or from crates.io
cargo install mimo-tui

# Or from source
git clone https://github.com/nousresearch/mimo-tui-rs.git
cd mimo-tui-rs
cargo build --release
sudo cp target/release/mimo-tui /usr/local/bin/
```

### Configure

```bash
# Set your API key
export MIMO_API_KEY="your-key-here"

# Or create config file
mkdir -p ~/.mimo-tui
cat > ~/.mimo-tui/config.toml << 'EOF'
api_key = "your-key-here"
model = "MiMo-7B-RL"
base_url = "https://api.xiaomimimo.com/v1"
thinking = true
web_search = true
permission_mode = "agent"
EOF
```

### Run

```bash
# Interactive TUI mode (default)
mimo-tui

# One-shot command
mimo-tui -c "fix all warnings in this project"

# With web search
mimo-tui --search -c "what's the latest Rust async runtime benchmarks"

# Simple terminal mode (no TUI)
mimo-tui --simple

# Resume a session
mimo-tui --resume <session-id>
```

---

## Features

### 🖥️ TUI Mode
Full-screen terminal interface with:
- Real-time streaming with syntax highlighting
- Thinking/reasoning display (collapsible)
- Mouse support and keyboard navigation
- Dark theme optimized for terminals

### 🔧 Tool System
Built-in tools for autonomous coding:
- `shell` — Execute shell commands with timeout
- `read_file` / `write_file` / `edit_file` — File operations with line numbers
- `grep` — Regex search with glob filters
- `list_dir` / `project_index` — Project exploration

### 🤖 Sub-agents
Spawn parallel workers for complex tasks:
```
/agent fix all warnings in src/main.rs | add tests for subagent module | refactor session.rs
```

### 🔌 MCP (Model Context Protocol)
Connect to external tool servers:
```json
// ~/.mimo-tui/mcp.json
{
  "filesystem": {
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path"]
  }
}
```

### 🔒 Permission Modes
- **plan** — Read-only, suggestions only
- **agent** — Ask before modifications (default)
- **yolo** — Full autonomy (use with caution)

### 💾 Session Management
```bash
/save              # Save current session
/sessions          # List saved sessions
--resume <id>      # Resume a session
```

### 📸 Git Snapshots
Automatic snapshots before tool calls:
```
/snapshot [label]  # Take a manual snapshot
/snapshots         # List snapshots
/snapshot-restore   # Restore a snapshot
```

---

## Configuration

### Environment Variables
| Variable | Description |
|----------|-------------|
| `MIMO_API_KEY` | API key (required) |
| `MIMO_MODEL` | Model name |
| `MIMO_BASE_URL` | API base URL |
| `MIMO_THINKING` | Enable thinking (true/false) |
| `MIMO_SEARCH` | Enable web search (true/false) |

### Config File
`~/.mimo-tui/config.toml`:
```toml
api_key = "your-key"
model = "MiMo-7B-RL"
base_url = "https://api.xiaomimimo.com/v1"
thinking = true
web_search = true
permission_mode = "agent"
max_output_tokens = 32768
system_prompt_extra = ""
```

---

## Keyboard Shortcuts (TUI Mode)

| Key | Action |
|-----|--------|
| `Tab` | Expand/collapse thinking |
| `Ctrl+C` | Quit |
| `Ctrl+L` | Clear chat |
| `Ctrl+K` | Clear input |
| `↑/↓` | Scroll history |
| `PgUp/PgDn` | Fast scroll |
| `Enter` | Submit |
| `Backspace` | Delete char |

---

## Architecture

```
src/
├── main.rs          # CLI entry, agent loop, command dispatch
├── api.rs           # SSE streaming client, MiMoClient
├── tools.rs         # Built-in tool definitions and execution
├── tui.rs           # ratatui TUI interface
├── config.rs        # TOML configuration
├── constitution.rs  # System prompt templates
├── git.rs           # Side-git snapshots
├── highlight.rs     # Syntect syntax highlighting
├── lsp.rs           # LSP diagnostics integration
├── mcp.rs           # MCP client (JSON-RPC 2.0 over stdio)
├── session.rs       # Session persistence (JSON)
└── subagent.rs      # Parallel sub-agent execution
```

---

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing`)
5. Open a Pull Request

### Development
```bash
# Build
cargo build

# Run tests
cargo test

# Check formatting
cargo fmt --check

# Lint
cargo clippy
```

---

## License

MIT License — see [LICENSE](LICENSE) for details.

---

## Acknowledgments

- [MiMo](https://github.com/xiaomi/mimo) — The AI model powering this agent
- [CodeWhale](https://github.com/nicepkg/codewhale) — Architecture inspiration
- [ratatui](https://ratatui.rs/) — TUI framework
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) — Feature reference
