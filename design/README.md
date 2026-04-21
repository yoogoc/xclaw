# 多 Agent 协作系统设计文档

## 1. 架构概述

### 1.1 核心设计原则

- **配置驱动**: Agent 角色、能力、行为完全由配置文件定义（SOUL.md + TOOL.md）
- **文件即状态**: 任务状态存储在共享 workspace 的文件系统中，所有 Agent 可读写
- **Task ID 中心**: task_id 是所有通信的上下文锚点，必须显式声明
- **异步解耦**: 通过心跳机制监控任务，不依赖同步响应

### 1.2 系统架构图

```
┌─────────────────────────────────────────────────────────────┐
│                     Chat Room Layer                         │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐                     │
│  │Discord  │  │ Slack   │  │ Matrix  │  ...               │
│  └────┬────┘  └────┬────┘  └────┬────┘                     │
│       │            │            │                           │
│       └────────────┴────────────┘                           │
│                    │                                        │
│              ChatRoom (抽象层)                              │
└────────────────────┼────────────────────────────────────────┘
                     │
         ┌───────────┼───────────┐
         ▼           ▼           ▼
    ┌─────────┐ ┌─────────┐ ┌───────────┐
    │ Agent A │ │ Agent B │ │ Agent C   │
    │(统筹者)  │ │(执行者)  │ │(执行者)   │
    └────┬────┘ └────┬────┘ └─────┬─────┘
         │           │            │
         └───────────┼────────────┘
                     │
            ┌────────▼────────┐
            │   Workspace     │
            │  (Shared FS)    │
            ├─────────────────┤
            │  tasks/         │
            │  .agents/       │
            │  memory/        │
            └─────────────────┘
```

### 1.3 目录结构

```
design/
├── README.md              # 本文件 - 架构概述
├── agent-definition.md    # Agent 定义系统
├── task-system.md         # 任务系统
├── communication.md       # 通信协议
├── tools.md              # 工具系统
├── heartbeat.md          # 心跳机制
├── workflows.md          # 完整工作流程
└── attachment.md         # 附件系统（双层类型、binding 转换、LLM 多模态）
```

### 1.4 技术栈

- **语言**: Rust
- **LLM SDK**: rig-core
- **Discord**: serenity
- **配置**: toml + markdown
- **存储**: 文件系统 (JSON)

## 2. 关键设计要点

### 2.1 配置驱动

- **SOUL.md**: 定义 Agent 人格、行为模式、沟通规范
- **TOOL.md**: 定义工具使用方法，明确 task_id 参数要求
- **CONFIG.toml**: 技术配置（模型、权限、Chat Room 绑定等）

### 2.2 Chat Room 抽象

- Chat Room 是跨平台的协作空间抽象
- 支持 Discord、Slack、Matrix 等多种平台
- Agent 通过 Chat Room 进行通信，不关心底层协议
- 一个 Agent 可以存在于多个 Chat Room

- task_id 在 task_create 时生成，全程唯一
- 所有任务相关工具的第一个参数必须是 task_id
- Discord 消息中必须显式包含 task_id（如 #a7b3c9d2）
- 代码层面**不做提取**，完全依赖 LLM 的理解能力

### 2.3 状态管理

- 任务状态存储在 workspace 文件系统中
- JSON 文件按状态分目录存放（pending/active/completed/failed）
- 所有 Agent 通过工具读写，保证一致性

### 2.4 异步通信

- 通过 Discord @mention 进行通知
- 心跳机制定期检查进度（30秒间隔）
- 10分钟无更新视为停滞，主动询问

### 2.5 异常处理

- 停滞检测与主动询问
- 任务可重新分配给其他 Agent
- 支持部分完成和任务取消

## 3. 快速导航

- [Chat Room 系统](chat-room.md) - 多平台通信抽象层（Discord/Slack/Matrix）
- [Agent 定义系统](agent-definition.md) - 如何定义 Agent 角色和能力
- [任务系统](task-system.md) - 任务生命周期和状态管理
- [通信协议](communication.md) - 消息格式规范和最佳实践
- [工具系统](tools.md) - 可用工具列表和使用方法
- [心跳机制](heartbeat.md) - 任务监控和停滞检测
- [工作流程](workflows.md) - 完整的交互流程示例
- [附件系统](attachment.md) - 双层类型架构、binding 转换、LLM 多模态处理
