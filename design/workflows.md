# 7. 完整工作流程

## 7.1 任务分配流程

### 7.1.1 用户请求触发

```
用户: "@a 帮我分析这个代码库"

Discord 消息到达:
  author: user#1234
  content: "@a 帮我分析这个代码库"
  mentions: ["agent-a"]

Agent A 处理:
  1. 检测到 @mention
  2. 提取内容: "帮我分析这个代码库"
  3. 构建 LLM Prompt (包含 SOUL.md + TOOL.md 上下文)
```

### 7.1.2 任务拆解与创建

```
Agent A LLM 推理:
  "用户需要代码库分析，这可以拆解为：
   1. 读取文件结构
   2. 分析模块依赖
   需要分配给 agent-b 和 agent-c"

工具调用 1:
  task_create(
    title: "读取项目文件结构",
    description: "分析代码库的目录组织和文件分布，识别主要模块",
    assign_to: "agent-b",
    priority: high,
    parameters: {
      "target_path": "./",
      "include_hidden": false
    }
  )

返回:
  {
    "task_id": "a7b3c9d2-e4f5-6789-abcd-ef0123456789",
    "short_id": "a7b3c9d2"
  }

工具调用 2:
  task_create(
    title: "分析模块依赖关系",
    description: "分析项目模块间的依赖关系，生成依赖图",
    assign_to: "agent-c",
    priority: high,
    parameters: {
      "depth": 3,
      "include_dev_deps": true
    }
  )

返回:
  {
    "task_id": "f8e2d1a3-9b8c-7d6e-5f4a-3210fedcba98",
    "short_id": "f8e2d1a3"
  }

任务文件创建:
  workspace/tasks/pending/a7b3c9d2.json
  workspace/tasks/pending/f8e2d1a3.json
```

### 7.1.3 发送任务通知

```
工具调用 3:
  send_notification(
    to: "agent-b",
    message: "@agent-b 新任务 #a7b3c9d2：\n\
             **读取项目文件结构**\n\
             分析代码库的目录组织和文件分布\n\n\
             任务ID: a7b3c9d2-e4f5-6789-abcd-ef0123456789\n\
             优先级: High\n\
             👉 使用 task_read task_id: \"a7b3c9d2\" 查看详情",
    task_id: "a7b3c9d2"
  )

工具调用 4:
  send_notification(
    to: "agent-c",
    message: "@agent-c 新任务 #f8e2d1a3：\n\
             **分析模块依赖关系**\n\
             分析项目模块间的依赖关系，生成依赖图\n\n\
             任务ID: f8e2d1a3-9b8c-7d6e-5f4a-3210fedcba98\n\
             优先级: High\n\
             👉 使用 task_read task_id: \"f8e2d1a3\" 查看详情",
    task_id: "f8e2d1a3"
  )

Discord 显示:
  @agent-b 新任务 #a7b3c9d2：
  **读取项目文件结构**
  分析代码库的目录组织和文件分布
  
  任务ID: a7b3c9d2-e4f5-6789-abcd-ef0123456789
  优先级: High
  👉 使用 task_read task_id: "a7b3c9d2" 查看详情
```

## 7.2 任务认领流程

### 7.2.1 Agent B 接收通知

```
Discord 消息到达 Agent B:
  content: "@agent-b 新任务 #a7b3c9d2..."
  mentions: ["agent-b"]

Agent B 处理:
  1. 检测到 @mention
  2. LLM 从消息中提取 task_id: "a7b3c9d2"
  3. 决定查看详情
```

### 7.2.2 查看并认领任务

```
工具调用 1:
  task_read(task_id: "a7b3c9d2")

返回:
  {
    "task_id": "a7b3c9d2-e4f5-6789-abcd-ef0123456789",
    "title": "读取项目文件结构",
    "description": "分析代码库的目录组织和文件分布",
    "status": "pending",
    "assigned_to": null,
    "created_by": "agent-a",
    "parameters": { "target_path": "./" }
  }

工具调用 2:
  task_claim(task_id: "a7b3c9d2")

任务文件移动:
  pending/a7b3c9d2.json → active/a7b3c9d2.json

任务状态更新:
  status: "active"
  assigned_to: "agent-b"
  started_at: "2024-01-15T09:05:00Z"

返回:
  {
    "success": true,
    "task": {
      "task_id": "a7b3c9d2",
      "status": "active",
      "assigned_to": "agent-b"
    }
  }

工具调用 3:
  send_notification(
    to: "agent-a",
    message: "@a 已认领任务 #a7b3c9d2，开始执行",
    task_id: "a7b3c9d2"
  )

Discord 显示:
  @a 已认领任务 #a7b3c9d2，开始执行
```

## 7.3 进度更新流程

### 7.3.1 执行中定期更新

```
Agent B 执行步骤 1：扫描根目录

工具调用:
  task_progress(
    task_id: "a7b3c9d2",
    progress: 25,
    step: "扫描根目录完成",
    message: "发现 3 个子目录: src/, tests/, docs/"
  )

任务文件更新:
  progress: 25
  current_step: "扫描根目录完成"
  updated_at: "2024-01-15T09:10:00Z"

工具调用:
  send_notification(
    to: "agent-a",
    message: "@a 任务 #a7b3c9d2 进度更新\n\
             [██░░░░░░░░] 25% - 扫描根目录完成\n\
             发现 3 个子目录",
    task_id: "a7b3c9d2"
  )

---

Agent B 执行步骤 2：分析 src 目录

工具调用:
  task_progress(
    task_id: "a7b3c9d2",
    progress: 60,
    step: "分析 src 目录结构"
  )

工具调用:
  send_notification(
    to: "agent-a",
    message: "@a 任务 #a7b3c9d2 进度 60% - 分析 src 目录结构",
    task_id: "a7b3c9d2"
  )

---

Agent B 执行步骤 3：生成报告

工具调用:
  task_progress(
    task_id: "a7b3c9d2",
    progress: 90,
    step: "生成分析报告"
  )
```

### 7.3.2 任务完成

```
Agent B 完成所有步骤

工具调用 1:
  task_complete(
    task_id: "a7b3c9d2",
    result: {
      "files_found": 42,
      "directories": ["src", "tests", "docs"],
      "main_modules": [
        "src/core/mod.rs",
        "src/agent/mod.rs",
        "src/tools/mod.rs"
      ],
      "analysis_summary": "项目采用标准 Rust 结构..."
    },
    artifacts: [
      "workspace/artifacts/a7b3c9d2_file_tree.json",
      "workspace/artifacts/a7b3c9d2_summary.md"
    ]
  )

任务文件移动:
  active/a7b3c9d2.json → completed/a7b3c9d2.json

状态更新:
  status: "completed"
  completed_at: "2024-01-15T09:30:00Z"
  result: { ... }

工具调用 2:
  send_notification(
    to: "agent-a",
    message: "@a 任务 #a7b3c9d2 已完成 ✅\n\
             发现 42 个文件，3 个主要模块\n\
             生成了文件树和摘要报告",
    task_id: "a7b3c9d2"
  )

Discord 显示:
  @a 任务 #a7b3c9d2 已完成 ✅
  发现 42 个文件，3 个主要模块
  生成了文件树和摘要报告
```

## 7.4 心跳检查与停滞处理流程

### 7.4.1 正常心跳检查

```
Agent A 心跳检查 (每 30 秒):

工具调用:
  task_list(status: "active")

返回:
  [
    {
      "task_id": "a7b3c9d2",
      "title": "读取项目文件结构",
      "status": "active",
      "progress": 60,
      "updated_at": "2024-01-15T09:10:00Z"  // 10 分钟前
    },
    {
      "task_id": "f8e2d1a3",
      "title": "分析模块依赖关系",
      "status": "active",
      "progress": 10,
      "updated_at": "2024-01-15T09:05:00Z"  // 15 分钟前
    }
  ]

当前时间: 09:20:00

检查逻辑:
  任务 a7b3c9d2: 10 分钟无更新 < 阈值(10分钟) ✅ 正常
  任务 f8e2d1a3: 15 分钟无更新 > 阈值(10分钟) ⚠️ 停滞

Agent A LLM 决策:
  "任务 f8e2d1a3 停滞，需要询问 agent-c"
```

### 7.4.2 发送心跳询问

```
工具调用:
  send_notification(
    to: "agent-c",
    message: "@agent-c 任务 #f8e2d1a3 已超过 15 分钟无进度更新，请确认：\n\
             1. 是否仍在执行？\n\
             2. 是否遇到阻塞需要协助？\n\
             3. 预计何时完成？",
    task_id: "f8e2d1a3"
  )

记录:
  pending_queries[f8e2d1a3] = 09:20:00

Discord 显示:
  @agent-c 任务 #f8e2d1a3 已超过 15 分钟无进度更新，请确认：
  1. 是否仍在执行？
  2. 是否遇到阻塞需要协助？
  3. 预计何时完成？
```

### 7.4.3 场景 1 - Agent 正常回复

```
Agent C 收到询问后回复:

工具调用:
  send_notification(
    to: "agent-a",
    message: "@a 任务 #f8e2d1a3 仍在执行，正在处理复杂的循环依赖分析，
             预计还需 10 分钟完成，当前实际进度约 40%",
    task_id: "f8e2d1a3"
  )

工具调用:
  task_progress(
    task_id: "f8e2d1a3",
    progress: 40,
    step: "解析循环依赖"
  )

Agent A 处理:
  - 从 pending_queries 移除 f8e2d1a3
  - 重置监控状态
  - 继续正常心跳检查

日志:
  [09:22:00] 收到 agent-c 关于 #f8e2d1a3 的回复
  [09:22:00] 任务 #f8e2d1a3: 恢复监控
```

### 7.4.4 场景 2 - Agent 报告阻塞

```
Agent C 回复:

工具调用:
  send_notification(
    to: "agent-a",
    message: "@a 任务 #f8e2d1a3 遇到阻塞：无法访问私有仓库 npm.example.com，
             需要配置 token 或调整任务范围",
    task_id: "f8e2d1a3"
  )

Agent A LLM 决策:
  "Agent C 遇到权限问题，我需要决定：
   1. 让 agent-b 接手
   2. 调整任务范围
   3. 标记部分完成"

选择重新分配:

工具调用 1:
  task_read(task_id: "f8e2d1a3")
  // 获取任务详情

工具调用 2:
  send_notification(
    to: "agent-b",
    message: "@agent-b agent-c 在执行 #f8e2d1a3 时遇到权限问题，请接手此任务。
             从已分析的部分继续，跳过私有仓库。",
    task_id: "f8e2d1a3"
  )

Agent B 接手:
  task_claim(task_id: "f8e2d1a3")
  // 继续执行...
```

### 7.4.5 场景 3 - Agent 无响应

```
Agent C 无回复...

心跳检查 (09:40:00):
  任务 f8e2d1a3: 询问已发送 20 分钟，无响应

Agent A 处理:

工具调用:
  send_notification(
    to: "agent-a",  // 通知自己/统筹者
    message: "⚠️ Agent agent-c 执行任务 #f8e2d1a3 时失联（超过 20 分钟无响应）",
    task_id: "f8e2d1a3"
  )

决策 - 重新分配:

工具调用:
  send_notification(
    to: "agent-b",
    message: "@agent-b 请紧急接手任务 #f8e2d1a3，原执行者 agent-c 失联。
             任务详情：分析模块依赖关系",
    task_id: "f8e2d1a3"
  )

或决策 - 取消任务:

工具调用:
  task_complete(
    task_id: "f8e2d1a3",
    result: {
      "status": "cancelled",
      "reason": "执行者失联超过 20 分钟",
      "partial_result": null
    }
  )
```

## 7.5 任务完成汇总流程

### 7.5.1 检测所有子任务完成

```
Agent A 心跳检查:
  task_list(status: "active") → 返回空列表
  
  说明：没有活跃任务，所有子任务已完成

或收到完成通知后检查:
  已收到 #a7b3c9d2 完成通知
  已收到 #f8e2d1a3 完成通知
```

### 7.5.2 读取所有结果

```
工具调用 1:
  task_read(task_id: "a7b3c9d2")
  
返回:
  {
    "task_id": "a7b3c9d2",
    "status": "completed",
    "result": {
      "files_found": 42,
      "main_modules": ["src/core", "src/agent", "src/tools"]
    }
  }

工具调用 2:
  task_read(task_id: "f8e2d1a3")
  
返回:
  {
    "task_id": "f8e2d1a3",
    "status": "completed",
    "result": {
      "dependencies": {
        "src/core": ["src/agent", "src/tools"],
        "src/agent": ["src/tools"]
      }
    }
  }
```

### 7.5.3 LLM 汇总并回复用户

```
Agent A LLM 推理:
  "基于两个任务的结果：
   - Agent B 发现了 42 个文件，3 个核心模块
   - Agent C 分析了依赖关系，core 依赖 agent 和 tools
   
   我需要为用户生成一份完整的分析报告"

生成回复:
  "分析完成！代码库结构如下：
   
   📁 项目结构：
   - 共 42 个文件
   - 3 个核心模块：core, agent, tools
   
   🔗 依赖关系：
   - core 依赖 agent 和 tools
   - agent 依赖 tools
   - tools 无依赖（基础层）
   
   这是一个分层架构，建议保持当前设计..."

发送给用户:
  Discord 回复用户消息
```

## 7.6 异常处理流程

### 7.6.1 任务失败

```
Agent B 执行时遇到错误:

工具调用:
  task_complete(
    task_id: "a7b3c9d2",
    result: {
      "status": "failed",
      "error": "Permission denied: cannot read ./secrets",
      "retryable": false
    }
  )

工具调用:
  send_notification(
    to: "agent-a",
    message: "@a 任务 #a7b3c9d2 执行失败 ❌\n\
             错误：无法读取 ./secrets 目录（权限不足）\n\
             建议：调整任务范围或提供必要权限",
    task_id: "a7b3c9d2"
  )

Agent A 处理:
  - 评估是否可以重试
  - 决定重新创建任务（排除敏感目录）
  - 或调整任务参数后重新分配
```

### 7.6.2 任务取消

```
用户: "@a 取消分析任务"

Agent A:
  task_list(status: "active")
  → 找到相关任务

工具调用:
  send_notification(
    to: "agent-b",
    message: "@agent-b 用户要求取消任务 #a7b3c9d2，请停止执行",
    task_id: "a7b3c9d2"
  )
  
  send_notification(
    to: "agent-c",
    message: "@agent-c 用户要求取消任务 #f8e2d1a3，请停止执行",
    task_id: "f8e2d1a3"
  )

Agent B/C 响应:
  停止执行...
  
  task_complete(
    task_id: "a7b3c9d2",
    result: {
      "status": "cancelled",
      "reason": "用户取消",
      "partial_result": { ... }
    }
  )
```

## 7.7 工作流程图

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│  用户请求  │───►│  任务拆解  │───►│  任务创建  │───►│  发送通知  │
└──────────┘    └──────────┘    └────┬─────┘    └────┬─────┘
                                     │               │
                              ┌──────┘               ▼
                              │              ┌──────────────┐
                              │              │  Worker Agent │
                              │              └──────┬───────┘
                              │                     │
                              ▼                     ▼
                       ┌──────────────┐    ┌──────────────┐
                       │   workspace  │◄───┤   认领任务    │
                       │  (task files)│    └──────────────┘
                       └──────┬───────┘           │
                              │                   ▼
                              │            ┌──────────────┐
                              │            │   执行任务    │
                              │            └──────┬───────┘
                              │                   │
                              ▼                   ▼
                       ┌──────────────┐    ┌──────────────┐
                       │   心跳检查   │◄───┤  进度更新    │
                       │  (每30秒)    │    └──────────────┘
                       └──────┬───────┘           │
                              │                   │
              ┌───────────────┼───────────────────┘
              │               │
              ▼               ▼
       ┌──────────┐    ┌──────────┐
       │   正常   │    │   停滞   │
       └────┬─────┘    └────┬─────┘
            │               │
            │               ▼
            │        ┌──────────────┐
            │        │  询问 Agent  │
            │        └──────┬───────┘
            │               │
            │      ┌────────┼────────┐
            │      ▼        ▼        ▼
            │   ┌─────┐  ┌─────┐  ┌─────┐
            │   │正常 │  │帮助 │  │无响应│
            │   └─┬───┘  └─┬───┘  └─┬───┘
            │     │        │        │
            │     │        ▼        ▼
            │     │   ┌────────┐  ┌────────┐
            │     │   │重新分配│  │重新分配│
            │     │   │或调整  │  │或取消  │
            │     │   └────┬───┘  └────┬───┘
            │     │        │           │
            └─────┴────────┴───────────┘
                          │
                          ▼
                   ┌──────────────┐
                   │   任务完成   │
                   │  (所有子任务) │
                   └──────┬───────┘
                          │
                          ▼
                   ┌──────────────┐
                   │   结果汇总   │
                   └──────┬───────┘
                          │
                          ▼
                   ┌──────────────┐
                   │   回复用户   │
                   └──────────────┘
```

## 7.8 设计要点总结

1. **显式 Task ID**: 所有通信必须包含 task_id，LLM 自行提取和使用
2. **工具驱动**: Agent 通过工具读写 workspace，不直接操作文件
3. **异步通信**: Discord @mention 通知，不等待同步回复
4. **心跳监控**: 被动检查任务文件，主动询问停滞任务
5. **灵活处理**: LLM 决策后续处理（继续/协助/重新分配/取消）
6. **状态持久**: 所有状态在 workspace 文件中，可恢复可审计
