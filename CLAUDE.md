# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**xclaw** is an AI Agent framework written in Rust, inspired by OpenClaw/ZeroClaw. It enables multi-agent collaboration through a configuration-driven architecture where agents communicate via chat platforms (Discord, Slack, Matrix).

## Build and Development Commands

```bash
# Build the project
cargo build

# Build for release
cargo build --release

# Run the application
cargo run

# Run tests
cargo test

# Run a specific test
cargo test <test_name>

# Check for compile errors without building
cargo check

# Format code
cargo fmt

# Run clippy lints
cargo clippy
```

## High-Level Architecture

### Core Design Principles

1. **Configuration-Driven**: Agent roles, capabilities, and behaviors are defined entirely through configuration files (SOUL.md + TOOL.md + CONFIG.toml)
2. **File-as-State**: Task states are stored in a shared workspace file system (JSON files), readable and writable by all agents
3. **Task ID Centric**: `task_id` is the context anchor for all communications and must be explicitly declared
4. **Async Decoupled**: Agents communicate asynchronously via chat platforms; no synchronous request-response

### Module Structure

```
src/
├── main.rs              # Application entry point
├── lib.rs               # Module exports
├── agent/               # Agent core logic and heartbeat monitoring
│   └── heartbeat.rs     # Stalled task detection
├── attachment/          # AttachmentManager (OpenDAL, 待实现)
├── binding/             # Channel→Session 边界层
│   ├── mod.rs           # process_user_input(), 类型转换
│   └── message_convert.rs # MessageAttachment→rig SDK UserContent
├── channel/             # 通道层 (Discord/Slack/Matrix 集成)
│   └── message/         # IncomingMessage, IncomingAttachment
├── config/              # 配置加载
├── message/             # 消息域类型
│   ├── message.rs       # ChatMessage (含 attachments)
│   └── attachment.rs    # MessageAttachment, MediaKind, base64 serde
├── session/             # 会话管理
│   ├── manager.rs       # SessionManager, process_turn()
│   └── turn.rs          # Turn (含 attachments)
├── hooks/               # Event hook system
├── memory/              # Memory/state management
├── skills/              # Agent skill system
├── supervisor/          # Supervisor/orchestrator pattern
└── tools/               # Tool calling system
```

### Key Technical Decisions

**Task System**: Tasks move through directories based on state (`workspace/tasks/{pending,active,completed,failed}/`). Each task is a JSON file named with its full UUID. Task IDs have both full UUID and short (8-char) formats; short IDs are used for display, full IDs for storage.

**Agent Configuration**: Each agent has three config files in `workspace/.agents/{agent_id}/`:
- `SOUL.md`: Personality, behavior patterns, communication norms
- `TOOL.md`: Tool usage instructions
- `CONFIG.toml`: Technical config (model, permissions, Discord bindings)

**Chat Room Abstraction**: The `ChatRoom` trait (`src/chat_room/`) abstracts over Discord, Slack, and Matrix. Agents bind to specific channels; the system routes messages automatically.

**Heartbeat Monitoring**: Every 30 seconds, the heartbeat monitor checks active tasks. Tasks with no updates for 10 minutes trigger a status query to the assigned agent. After 20 minutes without response, the task may be reassigned.

### Communication Protocol

All inter-agent communication happens through Discord messages (or other chat platforms). Messages must explicitly include task IDs using the `#short-id` format (e.g., `#a7b3c9d2`). The system does **not** extract task IDs from messages programmatically—it relies on the LLM to understand and use them correctly.

Standard message patterns:
- Task assignment: `@agent-b 新任务 #a7b3c9d2：[任务标题]`
- Progress update: `@agent-a 任务 #a7b3c9d2 进度 50% - [步骤描述]`
- Task completion: `@agent-a 任务 #a7b3c9d2 已完成 ✅`

### Technology Stack

- **Language**: Rust (Edition 2024)
- **LLM SDK**: rig-core (v0.31.0)
- **Web Framework**: Axum (v0.8.8) with WebSocket support
- **Database**: SQLite via Diesel ORM
- **Discord Integration**: serenity (v0.12.5)
- **Async Runtime**: Tokio with full features
- **Serialization**: serde/serde_json
- **CLI**: clap

### Design Documentation

The `design/` directory contains comprehensive design documents:
- `README.md`: Architecture overview
- `agent-definition.md`: Agent configuration system
- `task-system.md`: Task lifecycle and state management
- `communication.md`: Message formats and protocols
- `chat-room.md`: Chat room abstraction and multi-platform support
- `tools.md`: Available tools and usage
- `heartbeat.md`: Task monitoring and stall detection
- `workflows.md`: Complete interaction examples
- `mention-protocol.md`: @mention handling
- `attachment.md`: Attachment system (persist-first, AttachmentManager, storage backends, LLM multimodal)
