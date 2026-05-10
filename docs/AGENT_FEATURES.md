# Lucky Agent - Feature Documentation

## Overview

Lucky is a desktop pet application built with Tauri v2 + React + Rust. It features a pixel art panda companion that users can chat with via configurable LLM providers. The app runs as a transparent, frameless, always-on-top window on macOS.

---

## Architecture

```
Frontend (React + TypeScript)       Backend (Rust + Tauri)
┌─────────────────────────┐        ┌──────────────────────────────┐
│  App.tsx                 │  IPC   │  lib.rs (Tauri setup + menu) │
│  - Panda sprite          │◄─────►│  settings.rs (commands)      │
│  - Speech bubble         │        │  borderless/ (agent modules) │
│  - Chat input            │        └──────────────────────────────┘
│  - Settings panel        │
└─────────────────────────┘
```

- **Frontend**: `src/` — React 19 + TypeScript, Vite bundler
- **Backend**: `src-tauri/src/` — Rust, Tauri v2
- **Agent Core**: `src-tauri/src/borderless/` — Modular AI agent framework (providers, tools, memory, sessions, skills, MCP)

---

## Features

### 1. Desktop Pet (Pixel Panda)

- 16x16 pixel art panda rendered with CSS `box-shadow`
- Idle bounce animation (2s cycle)
- Transparent frameless window (140x160px)
- Always-on-top, draggable via `startDragging()` API
- Hover reveals chat toolbar button

### 2. Chat with AI

- Click chat button → window expands to 280x240
- User types in input bar at the bottom
- AI response appears as a **speech bubble** above-right of the panda
- Bubble auto-fades after 10 seconds (1s fade-out animation)
- New messages replace the current bubble (single bubble, no history displayed)
- Conversation context preserved in memory for follow-up messages
- "Thinking" indicator (animated "...") while waiting for response

### 3. LLM Provider Support

Supports any OpenAI-compatible API via the `OpenAIProvider`:

| Provider | Base URL | API Key Required |
|----------|----------|-----------------|
| OpenAI | `https://api.openai.com/v1` | Yes |
| Anthropic | `https://api.anthropic.com` | Yes |
| Google | `https://generativelanguage.googleapis.com` | Yes |
| Ollama (Local) | `http://localhost:11434/v1` | No |

- Ollama auto-appends `/v1` and lowercases model name
- All providers use the same OpenAI-compatible chat/completions endpoint
- System prompt: *"You are Lucky, a cute and friendly panda companion. Keep your responses short, warm, and playful."*

### 4. Settings (Menu Bar)

Accessed via **Lucky > Settings... (Cmd+,)** in the macOS menu bar.

Configurable fields:
- **Working Directory** — where session data, configs, and skills are stored (with native folder picker)
- **Provider** — OpenAI / Anthropic / Google / Ollama
- **API Key** — required for cloud providers, optional for Ollama
- **Model** — e.g. `gpt-4o`, `gemma4:e4b`, `claude-sonnet-4-20250514`
- **Base URL** — custom endpoint override

Settings persist to: `~/Library/Application Support/com.casperLHK.lucky/settings.json`

Auto-opens on first launch if no LLM is configured.

### 5. Session Persistence

Chat history is saved to the working directory:

```
{working_directory}/
  sessions/
    current.json      # Full message history (role + text + timestamp)
  config/
    mcp.json          # MCP server configs (placeholder)
    skills.json       # Skill definitions (placeholder)
```

- History restored on app restart (AI retains context)
- Auto-saved after each AI response
- Directory structure auto-created on first use

### 6. Menu Bar Integration

macOS menu bar with "Lucky" submenu:
- **Settings... (Cmd+,)** — opens settings panel
- **Quit Lucky (Cmd+Q)** — exits the app

---

## Borderless Agent Framework

The `src-tauri/src/borderless/` module contains a full AI agent toolkit integrated as part of the project:

### Modules

| Module | Purpose |
|--------|---------|
| `agent_core/` | Core types: `LlmConfig`, `ChatMessage`, `LlmError`, `ToolDefinition`, `ProviderName` |
| `providers/` | LLM providers: OpenAI, Anthropic, Google (feature-gated) |
| `tools/` | Tool registry, executor, sandbox, 12 built-in tools (bash, read/write file, grep, etc.) |
| `skills/` | Skill registry and lifecycle management |
| `memory/` | Episodic + semantic memory with hybrid retrieval |
| `session/` | Session persistence (file + S3 backends) |
| `context/` | Context assembly, token budgeting, guardrails |
| `telemetry/` | Spans, metrics, exporters |
| `mcp/` | Model Context Protocol client (feature-gated) |
| `agent/` | AgentBuilder, AgentInstance, agent loop, autonomous task loop |

### Feature Flags

```toml
[features]
default = ["openai"]
openai = []
anthropic = []
google = []
embeddings = []
cloud-storage = ["dep:aws-sdk-s3", "dep:aws-config"]
mcp = []
full = ["openai", "anthropic", "google", "embeddings", "cloud-storage", "mcp"]
```

### Supported Models (Context Windows)

| Model Pattern | Context Window |
|---------------|---------------|
| gpt-4o | 128K |
| claude-opus-4 / claude-sonnet-4 | 200K |
| gemini-2.5 / gemini-2.0-flash | 1M |
| llama3 | 128K |
| deepseek | 128K |
| mistral / mixtral | 32K |

---

## Tauri Commands (IPC API)

| Command | Description |
|---------|-------------|
| `load_settings` | Load app settings from disk |
| `save_settings` | Save settings + auto-init working directory |
| `pick_directory` | Open native folder picker dialog |
| `chat_message` | Send messages to LLM, get response |
| `load_session` | Load chat history from working directory |
| `save_session` | Save chat history to working directory |

---

## Development

```bash
# Start dev mode (frontend + backend)
bun run tauri dev

# Frontend only
bun run dev

# Rust only
cd src-tauri && cargo build

# Type check
npx tsc --noEmit

# Build production
bun run tauri build
```

---

## File Structure

```
lucky/
├── src/                    # React frontend
│   ├── App.tsx             # Main UI (panda + bubble + settings)
│   ├── App.css             # Styles (pixel art, bubble, panels)
│   └── main.tsx            # Entry point
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── lib.rs          # Tauri setup, menu, command registration
│   │   ├── settings.rs     # Settings, chat, session commands
│   │   ├── main.rs         # Binary entry point
│   │   └── borderless/     # Agent framework (10 modules)
│   ├── Cargo.toml          # Dependencies + feature flags
│   ├── tauri.conf.json     # Window config (140x160, transparent, frameless)
│   └── capabilities/       # Tauri permissions
├── docs/                   # Documentation
├── package.json            # Frontend deps (React, Tauri API)
└── CLAUDE.md               # Development instructions
```
