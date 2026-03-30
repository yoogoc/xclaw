# Binding Agent Loop 完整流程设计

## 1. 概述

Binding 是外部消息与 Agent 执行的桥梁，负责：
- 接收 channel 消息
- 路由到正确的 session/thread
- 驱动 agent loop 执行
- 管理 turn 生命周期
- 处理工具审批流程
- 发送响应回 channel

## 2. 核心流程图

```
┌─────────────────────────────────────────────────────────────┐
│                    Binding.start()                          │
│                         │                                    │
│                         ↓                                    │
│              ┌──────────────────────┐                       │
│              │  channel.receive()   │                       │
│              └──────────┬───────────┘                       │
│                         │                                    │
│                         ↓                                    │
│              ┌──────────────────────┐                       │
│              │  handle_message()    │                       │
│              └──────────┬───────────┘                       │
│                         │                                    │
│         ┌───────────────┼───────────────┐                  │
│         │               │               │                   │
│         ↓               ↓               ↓                   │
│    parse_intent   resolve_session  resolve_thread          │
│         │               │               │                   │
│         └───────────────┴───────────────┘                  │
│                         │                                    │
│         ┌───────────────┼───────────────┐                  │
│         ↓               ↓               ↓                   │
│   UserInput      Approval         Interrupt                │
│         │               │               │                   │
│         ↓               ↓               ↓                   │
│  process_user_   process_        process_                  │
│     input()      approval()      interrupt()               │
│         │               │               │                   │
│         └───────────────┴───────────────┘                  │
│                         │                                    │
│                         ↓                                    │
│              ┌──────────────────────┐                       │
│              │    run_loop()        │                       │
│              └──────────┬───────────┘                       │
│                         │                                    │
│                         ↓                                    │
│              ┌──────────────────────┐                       │
│              │ run_agentic_loop()   │◄─────┐               │
│              └──────────┬───────────┘      │               │
│                         │                   │               │
│         ┌───────────────┼───────────────┐  │               │
│         ↓               ↓               ↓  │               │
│    Response      ToolCall        MaxIter  │               │
│         │               │               │  │               │
│         ↓               ↓               ↓  │               │
│    complete_turn   check_approval   warn  │               │
│         │               │               │  │               │
│         │               ├─auto_approved─┘  │               │
│         │               │                   │               │
│         │               ├─need_approval────►│               │
│         │               │   (wait user)     │               │
│         │               │                   │               │
│         └───────────────┴───────────────────┘              │
│                         │                                    │
│                         ↓                                    │
│              ┌──────────────────────┐                       │
│              │  channel.send()      │                       │
│              └──────────────────────┘                       │
└─────────────────────────────────────────────────────────────┘
```

## 3. 详细流程设计

### 3.1 handle_message() - 消息入口

```rust
pub async fn handle_message(&self, message: &IncomingMessage) -> Result<()> {
    // Step 1: Parse intent
    let intent = self.parse_intent(message).await?;

    // Step 2: Resolve session & thread
    let (session, thread_id) = self.session_manager.resolve_thread(
        &self.binding_id,
        &message.user_id,
        &message.channel,
        message.thread_id.as_deref(),
    ).await;

    // Step 3: Get thread and check state
    let mut sess = session.lock().await;
    let thread = sess.threads.get_mut(&thread_id)
        .ok_or_else(|| anyhow!("Thread not found"))?;

    // Step 4: Dispatch by intent and thread state
    match (intent, thread.state) {
        (Intent::UserInput, ThreadState::Idle) => {
            drop(sess); // Release lock before async call
            self.process_user_input(session, thread_id, message).await
        }
        (Intent::UserInput, ThreadState::AwaitingApproval) => {
            // User sent new message while waiting approval - interrupt current turn
            thread.state = ThreadState::Interrupted;
            drop(sess);
            self.process_user_input(session, thread_id, message).await
        }
        (Intent::ApprovalAccept, ThreadState::AwaitingApproval) => {
            drop(sess);
            self.process_approval(session, thread_id, true, false).await
        }
        (Intent::ApprovalReject, ThreadState::AwaitingApproval) => {
            drop(sess);
            self.process_approval(session, thread_id, false, false).await
        }
        (Intent::ApprovalAlways, ThreadState::AwaitingApproval) => {
            drop(sess);
            self.process_approval(session, thread_id, true, true).await
        }
        (Intent::Interrupt, _) => {
            thread.interrupt();
            drop(sess);
            self.send_message(message, "⏸️ Interrupted").await
        }
        _ => {
            drop(sess);
            Ok(()) // Ignore invalid state transitions
        }
    }
}
```

### 3.2 parse_intent() - 意图识别

```rust
enum Intent {
    UserInput,
    ApprovalAccept,
    ApprovalReject,
    ApprovalAlways,
    Interrupt,
    Command(String),
}

async fn parse_intent(&self, message: &IncomingMessage) -> Result<Intent> {
    let content = message.content.trim();

    // Check for approval responses
    if content.eq_ignore_ascii_case("yes") || content.eq_ignore_ascii_case("y") {
        return Ok(Intent::ApprovalAccept);
    }
    if content.eq_ignore_ascii_case("no") || content.eq_ignore_ascii_case("n") {
        return Ok(Intent::ApprovalReject);
    }
    if content.eq_ignore_ascii_case("always") || content.eq_ignore_ascii_case("a") {
        return Ok(Intent::ApprovalAlways);
    }

    // Check for interrupt
    if content.starts_with("/stop") || content.starts_with("/interrupt") {
        return Ok(Intent::Interrupt);
    }

    // Check for commands
    if content.starts_with('/') {
        return Ok(Intent::Command(content.to_string()));
    }

    // Default: user input
    Ok(Intent::UserInput)
}
```

### 3.3 process_user_input() - 处理用户输入

```rust
async fn process_user_input(
    &self,
    session: Arc<Mutex<Session>>,
    thread_id: Uuid,
    message: &IncomingMessage,
) -> Result<()> {
    // Step 1: Start new turn
    {
        let mut sess = session.lock().await;
        let thread = sess.threads.get_mut(&thread_id).unwrap();
        thread.start_turn(&message.content);
        // Store image attachments if any
        if !message.attachments.is_empty() {
            if let Some(turn) = thread.last_turn_mut() {
                // Convert attachments to ContentPart
                // turn.image_content_parts = ...
            }
        }
    }

    // Step 2: Run agent loop
    self.run_loop(session, thread_id).await
}
```

### 3.4 run_loop() - 主循环

```rust
async fn run_loop(
    &self,
    session: Arc<Mutex<Session>>,
    thread_id: Uuid,
) -> Result<()> {
    loop {
        let outcome = self.run_agentic_loop(session.clone(), thread_id).await?;

        match outcome {
            LoopOutcome::Response(response) => {
                // Complete turn
                {
                    let mut sess = session.lock().await;
                    let thread = sess.threads.get_mut(&thread_id).unwrap();
                    thread.complete_turn(&response.content);
                    thread.state = ThreadState::Idle;
                }

                // Send response
                self.send_response(&response).await?;
                break;
            }

            LoopOutcome::ToolCall { approvals, not_found } => {
                // Handle not found tools
                if !not_found.is_empty() {
                    self.send_error(&format!("Tools not found: {:?}", not_found)).await?;
                }

                // Check if all auto-approved
                let all_auto = approvals.iter().all(|a| a.auto_approved);

                if all_auto {
                    // Execute tools and continue loop
                    self.execute_tools(session.clone(), thread_id, &approvals).await?;
                    continue; // Loop back to run_agentic_loop
                } else {
                    // Need user approval
                    {
                        let mut sess = session.lock().await;
                        let thread = sess.threads.get_mut(&thread_id).unwrap();
                        thread.state = ThreadState::AwaitingApproval;
                        thread.pending_approvals = approvals.into_iter()
                            .map(|b| *b)
                            .collect();
                    }

                    // Send approval request
                    self.send_approval_request(&approvals).await?;
                    break; // Wait for user response
                }
            }

            LoopOutcome::MaxIterations => {
                {
                    let mut sess = session.lock().await;
                    let thread = sess.threads.get_mut(&thread_id).unwrap();
                    thread.fail_turn("Max iterations reached");
                }
                self.send_error("⚠️ Max tool iterations reached").await?;
                break;
            }

            LoopOutcome::Stopped => {
                {
                    let mut sess = session.lock().await;
                    let thread = sess.threads.get_mut(&thread_id).unwrap();
                    thread.interrupt();
                }
                self.send_message_str("⏸️ Stopped").await?;
                break;
            }
        }
    }

    Ok(())
}
```


### 3.5 run_agentic_loop() - Agent 执行循环

```rust
async fn run_agentic_loop(
    &self,
    session: Arc<Mutex<Session>>,
    thread_id: Uuid,
) -> Result<LoopOutcome> {
    let current_iteration = {
        let sess = session.lock().await;
        let thread = sess.threads.get(&thread_id).unwrap();
        thread.last_turn().map(|t| t.current_tool_iterations).unwrap_or(0)
    };

    for iteration in current_iteration..self.agent.config.max_iterations {
        // Update iteration
        {
            let mut sess = session.lock().await;
            let thread = sess.threads.get_mut(&thread_id).unwrap();
            if let Some(turn) = thread.last_turn_mut() {
                turn.current_tool_iterations = iteration;
            }
        }

        // Call LLM
        let response = self.call_llm(session.clone(), thread_id).await?;

        match response.finish_reason {
            FinishReason::Stop => return Ok(LoopOutcome::Response(Box::new(response))),
            FinishReason::ToolUse => {
                let approvals = self.prepare_tool_approvals(session.clone(), &response.tool_calls).await?;
                return Ok(LoopOutcome::ToolCall { approvals, not_found: vec![] });
            }
            FinishReason::Length => {
                self.compact_thread(session.clone(), thread_id).await?;
                continue;
            }
            _ => return Err(anyhow!("LLM error: {:?}", response.finish_reason)),
        }
    }

    Ok(LoopOutcome::MaxIterations)
}
```

### 3.6 call_llm() - 调用 LLM

```rust
async fn call_llm(&self, session: Arc<Mutex<Session>>, thread_id: Uuid) -> Result<LLMResponse> {
    // Build context from thread
    let messages = {
        let sess = session.lock().await;
        let thread = sess.threads.get(&thread_id).unwrap();
        thread.messages()
    };

    // Get tools
    let tools = self.agent.tools().await;
    let tool_defs = tools.values().map(|t| t.to_definition()).collect();

    // Stream LLM
    let llm = self.agent.llm.llm.clone();
    let mut stream = llm.stream(CompletionRequest {
        preamble: Some(self.build_system_prompt().await?),
        chat_history: OneOrMany::many(messages)?,
        tools: tool_defs,
        ..Default::default()
    }).await?;

    let mut builder = LLMResponseBuilder::new();
    while let Some(chunk) = stream.next().await {
        match chunk? {
            StreamedAssistantContent::Text(text) => {
                builder.append_text(&text);
                self.channel.chunk_send(&text).await?;
            }
            StreamedAssistantContent::ToolCall { tool_call, .. } => builder.add_tool_call(tool_call),
            StreamedAssistantContent::Final(result) => builder.set_finish_reason(result.finish_reason),
            _ => {}
        }
    }

    Ok(builder.build())
}
```

### 3.7 process_approval() - 处理审批

```rust
async fn process_approval(
    &self,
    session: Arc<Mutex<Session>>,
    thread_id: Uuid,
    approved: bool,
    always: bool,
) -> Result<()> {
    let approvals = {
        let mut sess = session.lock().await;
        let thread = sess.threads.get_mut(&thread_id).unwrap();
        if thread.state != ThreadState::AwaitingApproval {
            return Err(anyhow!("Not awaiting approval"));
        }
        thread.state = ThreadState::Processing;
        std::mem::take(&mut thread.pending_approvals)
    };

    if approved {
        if always {
            let mut sess = session.lock().await;
            for approval in &approvals {
                sess.auto_approve_tool(&approval.tool_name);
            }
        }
        self.execute_tools(session.clone(), thread_id, &approvals).await?;
        self.run_loop(session, thread_id).await
    } else {
        let mut sess = session.lock().await;
        let thread = sess.threads.get_mut(&thread_id).unwrap();
        thread.fail_turn("Tool approval rejected");
        drop(sess);
        self.send_message_str("❌ Rejected").await
    }
}
```

### 3.8 execute_tools() - 执行工具

```rust
async fn execute_tools(
    &self,
    session: Arc<Mutex<Session>>,
    thread_id: Uuid,
    approvals: &[PendingApproval],
) -> Result<()> {
    let tools = self.agent.tools().await;

    for approval in approvals {
        let tool = tools.get(&approval.tool_name)
            .ok_or_else(|| anyhow!("Tool not found"))?;

        // Record call
        {
            let mut sess = session.lock().await;
            let thread = sess.threads.get_mut(&thread_id).unwrap();
            if let Some(turn) = thread.last_turn_mut() {
                turn.record_tool_call(&approval.tool_name, approval.parameters.clone());
            }
        }

        // Execute
        let result = tool.execute(&approval.parameters).await;

        // Record result
        {
            let mut sess = session.lock().await;
            let thread = sess.threads.get_mut(&thread_id).unwrap();
            if let Some(turn) = thread.last_turn_mut() {
                match result {
                    Ok(output) => turn.record_tool_result(output),
                    Err(e) => turn.record_tool_error(e.to_string()),
                }
            }
        }
    }

    Ok(())
}
```

## 4. 关键设计点

### 4.1 状态转换

**Thread State:**
```
Idle → Processing → AwaitingApproval → Processing → Idle
Idle → Processing → Interrupted → Idle
```

**Turn State:**
```
Processing → Completed (正常)
Processing → Failed (错误/拒绝)
Processing → Interrupted (中断)
```

### 4.2 工具审批流程

```
ToolCall → check auto_approved
    ├─ Yes → execute → continue loop
    └─ No → AwaitingApproval → wait user
              ↓
         user response
              ├─ Accept → execute → continue
              ├─ Always → add to auto_approved → execute → continue
              └─ Reject → fail turn
```

### 4.3 锁管理

- 尽早释放：读取后立即 drop(sess)
- 避免跨 await 持有锁
- 批量操作时才持有

## 5. 与现有代码对齐

### 5.1 需要修改

1. **LoopType 扩展:**
```rust
pub enum LoopType {
    UserMessage(Box<IncomingMessage>),
    ApprovalAccept,
    ApprovalReject,
    ApprovalAlways,
    Interrupt,
}
```

2. **Binding 方法签名调整:**
- `run_loop(session, thread_id)` 替代 `run_loop(message)`
- `run_agentic_loop(session, thread_id)` 替代 `run_agentic_loop(message)`

### 5.2 复用现有

- `Thread::messages()` - LLM 上下文
- `Thread::start_turn/complete_turn/fail_turn`
- `Turn::record_tool_call/result/error`
- `Session::auto_approved_tools`

## 6. 测试场景

1. **基本对话:** 用户消息 → LLM 响应 → 完成
2. **自动工具:** LLM tool_use → 自动执行 → 继续 → 响应
3. **需要审批:** tool_use → 等待 → 用户批准 → 执行 → 响应
4. **审批拒绝:** 等待 → 用户拒绝 → 失败
5. **Always 审批:** 等待 → always → 加入列表 → 执行
6. **中断:** 处理中 → /stop → 中断
7. **新消息中断审批:** 等待审批 → 新消息 → 中断 → 新 turn
