<div align="center">

```
 ███╗   ███╗██╗███╗   ███╗ ██████╗       ████████╗██╗   ██╗██╗
 ████╗ ████║██║████╗ ████║██╔═══██╗      ╚══██╔══╝██║   ██║██║
 ██╔████╔██║██║██╔████╔██║██║   ██║█████╗   ██║   ██║   ██║██║
 ██║╚██╔╝██║██║██║╚██╔╝██║██║   ██║╚════╝   ██║   ╚██╗ ██╔╝██║
 ██║ ╚═╝ ██║██║██║ ╚═╝ ██║╚██████╔╝         ██║    ╚████╔╝ ██║
 ╚═╝     ╚═╝╚═╝╚═╝     ╚═╝ ╚═════╝          ╚═╝     ╚═══╝  ╚═╝
```

# 🐋 Mimo TUI

**小米 MiMo 终端编程助手 — 用 Rust 写的极速 AI Agent**

[![npm version](https://img.shields.io/npm/v/@cyf666nb/mimo-code?color=red&label=npm)](https://www.npmjs.com/package/@cyf666nb/mimo-code)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![GitHub Release](https://img.shields.io/github/v/release/cyf666nb/mimo-tui?color=green)](https://github.com/cyf666nb/mimo-tui/releases)

一个比 Claude Code 更轻、比 Cursor 更快的终端编程助手。

支持 **MiMo · OpenAI · DeepSeek · Claude** 等任意 OpenAI 兼容 API。
单文件部署，5.9MB，零依赖，开箱即用。

[English](#-english) · [快速开始](#-快速开始) · [功能](#-功能一览) · [安装](#-安装)

</div>

---

## ⚡ 快速开始

```bash
# 安装（npm 会自动下载对应平台的二进制）
npm install -g @cyf666nb/mimo-code

# 启动 — 首次运行会进入设置向导
mimo-code
```

首次启动会引导你选择 API 提供商、输入 Key、测试连接，30 秒搞定。

```
  ┌─ Choose your AI provider ─────────────────────┐
  │  1. MiMo (TokenPlan)     性价比最高            │
  │  2. MiMo (Official)      小米官方 API           │
  │  3. OpenAI               GPT-4o / o3           │
  │  4. DeepSeek             深度推理               │
  │  5. OpenRouter           200+ 模型网关          │
  │  6. Custom               任意兼容接口           │
  └───────────────────────────────────────────────┘
```

之后直接用：

```bash
# 一行命令搞定
mimo-code -c "帮我重构这个函数，加上错误处理"

# 进入交互模式
mimo-code

# 指定模型和 API
mimo-code --api-key sk-xxx --base-url https://api.deepseek.com/v1 --model deepseek-chat
```

## 🎯 功能一览

| 功能 | 说明 |
|------|------|
| 🖥️ **TUI 界面** | ratatui 渲染，支持 Thinking 实时展开/折叠 |
| 🔧 **8 个内置工具** | 读写文件、执行命令、搜索、Glob、Fetch、Web 搜索 |
| 🌐 **Web 搜索** | 内置搜索，不需要额外 API |
| 🧠 **深度思考** | MiMo / DeepSeek R1 推理链实时流式输出 |
| 📡 **MCP 协议** | 兼容所有 MCP 服务器，配置即用 |
| 🤖 **Sub-Agent** | 最多 20 个并行子任务，用 `\|` 分隔 |
| 📁 **Git 快照** | 每次编辑自动 side-git 备份，随时回滚 |
| 💾 **会话持久化** | 保存/恢复对话历史 |
| 🔒 **权限模式** | plan（只读分析）/ agent（自动执行）/ yolo（全权） |
| ⚙️ **首次设置向导** | 交互式引导配置 API、模型、功能开关 |
| 🪶 **极致轻量** | 5.9MB 单文件，20MB 内存，启动 < 100ms |

## 📦 安装

### npm（推荐）

```bash
npm install -g @cyf666nb/mimo-code
```

自动检测 macOS (x64/arm64)、Linux (x64/arm64)、Windows (x64) 并下载对应二进制。

### cargo

```bash
cargo install mimo-tui
```

### 手动下载

从 [GitHub Releases](https://github.com/cyf666nb/mimo-tui/releases) 下载对应平台的压缩包，解压后放到 PATH 里。

## ⚙️ 配置

首次运行会自动进入设置向导，也可以手动配置：

```bash
# 环境变量
export MIMO_API_KEY="your-api-key"
export MIMO_BASE_URL="https://api.xiaomimimo.com/v1"   # 可选
export MIMO_MODEL="mimo-v2.5-pro"                      # 可选
```

或使用 CLI 参数：

```bash
mimo-code --api-key sk-xxx --base-url https://api.openai.com/v1 --model gpt-4o
```

配置文件保存在 `~/.mimo-tui/config.toml`，运行时输入 `/config` 可重新设置。

## 🔌 MCP 配置

在 `~/.mimo-tui/mcp.json` 添加 MCP 服务器：

```json
{
  "servers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
    }
  }
}
```

MCP 工具会自动注册，用 `mcp__<server>__<tool>` 格式调用。

## 🏗️ 技术栈

- **语言**: Rust 2021
- **异步运行时**: tokio
- **HTTP**: reqwest + SSE streaming
- **TUI**: ratatui + crossterm
- **语法高亮**: syntect
- **配置**: toml + serde
- **构建**: LTO + strip = 5.9MB

## 📊 对比

| | Mimo TUI | Claude Code | Cursor | Aider |
|---|---|---|---|---|
| **体积** | **5.9 MB** | 200+ MB | 500+ MB | 50+ MB |
| **内存** | **~20 MB** | 200+ MB | 1 GB+ | 100+ MB |
| **启动** | **< 100ms** | 2s+ | 5s+ | 1s+ |
| **TUI** | ✅ | ❌ | ❌ | ❌ |
| **Web 搜索** | ✅ | ❌ | ❌ | ❌ |
| **深度思考** | ✅ | ❌ | ❌ | ❌ |
| **MCP** | ✅ | ✅ | ❌ | ❌ |
| **Sub-Agent** | ✅ 20 并行 | ✅ | ❌ | ❌ |
| **多模型** | ✅ 任意 | ❌ 仅 Claude | ✅ | ✅ |
| **开源** | ✅ MIT | ❌ | ❌ | ✅ |

## 📝 命令

```
/help              显示帮助
/config            重新运行设置向导
/mode <模式>       切换权限模式 (plan/agent/yolo)
/model <模型>      切换模型
/thinking <on|off> 开关深度思考
/clear             清空对话
/tokens            查看 token 用量
/save              保存会话
/sessions          列出会话
/snapshot [标签]    创建 git 快照
/snapshots         列出快照
/agent <任务1> | <任务2>   并行执行多个子任务
/quit              退出
```

快捷键：`Tab` 展开 Thinking · `Ctrl+C` 退出 · `Ctrl+L` 清屏 · `↑↓` 滚动

---

<div align="center">

## 🇬🇧 English

**Mimo TUI** — A blazing-fast terminal coding agent written in Rust.

Works with **MiMo, OpenAI, DeepSeek, Claude**, and any OpenAI-compatible API.
Single binary, 5.9MB, zero dependencies.

### Quick Start

```bash
npm install -g @cyf666nb/mimo-code
mimo-code    # First run: interactive setup wizard
```

### Features

- 🖥️ **TUI** — ratatui-powered terminal UI with streaming thinking display
- 🔧 **8 Built-in Tools** — file read/write, shell, search, glob, fetch, web search
- 🌐 **Web Search** — built-in, no extra API needed
- 🧠 **Deep Thinking** — real-time reasoning chain for MiMo / DeepSeek R1
- 📡 **MCP Protocol** — connect any MCP server
- 🤖 **Sub-Agents** — up to 20 parallel tasks
- 📁 **Git Snapshots** — auto side-git backup on every edit
- 🔒 **Permission Modes** — plan / agent / yolo
- 🪶 **5.9MB** — single binary, ~20MB RAM, < 100ms startup

### Supported Providers

MiMo · OpenAI · DeepSeek · OpenRouter · Any OpenAI-compatible API

### Links

- [GitHub Releases](https://github.com/cyf666nb/mimo-tui/releases)
- [npm Package](https://www.npmjs.com/package/@cyf666nb/mimo-code)

---

**Made with 🦀 Rust · MIT License**

</div>
