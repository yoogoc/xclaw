# 附件系统设计

## 概述

xclaw 支持用户在聊天平台中发送附件（图片、音频、文档），并将其传递给 LLM 进行多模态处理。附件系统的核心设计是**持久化优先**：附件在进入 Session/Turn 流程之前，先通过 `AttachmentManager` 持久化到存储后端（Local/S3），后续 LLM 调用时再按需取出 base64 编码数据。

## 架构

### 三层职责

```
Channel 层               AttachmentManager              Message/Session 层
(外部域)                  (持久化 + 检索)                 (内部域)

IncomingAttachment  ──→  create() 持久化到存储  ──→  MessageAttachment (引用 ID)
  ├── source_url             │                         ├── attachment_id
  ├── data (可能为空)         │                         ├── kind (MediaKind)
  ├── kind                   ▼                         ├── mime_type
  ├── mime_type          Storage (Local/S3)             └── filename
  ├── filename               │
  └── ...平台元数据          │
                             │
                     get_base64(id)  ←────────────  LLM 调用前取出
                             │
                             ▼
                     base64 编码数据  ──→  rig SDK UserContent
```

### 核心数据流

```
Discord/Slack/Matrix 消息到达
        │
        ▼
  IncomingAttachment (channel 层, 携带 source_url 或 data)
        │
        ▼
  AttachmentManager.create()  ← 持久化边界
        │  1. 如果 data 为空但有 source_url → HTTP 下载
        │  2. 将字节写入存储后端 (Local FS / S3)
        │  3. 写入 DB 元数据记录
        │  4. 返回 Attachment 记录 (含 ID)
        │
        ▼
  构造 MessageAttachment (引用存储 ID, 不含原始数据)
        │
        ├──→ Turn.attachments         (会话回合)
        ├──→ ChatMessage.attachments  (消息持久化)
        │
  ───── LLM 调用时 ─────
        │
        ▼
  AttachmentManager.get_base64(id)  ← 从存储取出
        │
        ▼
  message_convert::to_rig_message()  ← 转为 rig SDK 格式
        │
        ├──→ UserContent::Image { data, media_type }
        ├──→ UserContent::Audio { data, media_type }
        └──→ UserContent::Document { data, media_type }
```

**关键设计**：`MessageAttachment` 不内联二进制数据，只持有存储 ID。这样：
- 序列化到 SQLite 时不会膨胀（避免 base64 图片数据直接写入 JSON）
- 历史消息恢复时，附件数据从存储按需加载
- 同一附件在多处引用不会重复存储

## 类型定义

### Channel 层: IncomingAttachment

定义在 `src/channel/message/attachment.rs`，携带平台原始信息：

```rust
pub struct IncomingAttachment {
    pub kind: AttachmentKind,       // Audio / Image / Document / Unknown
    pub mime_type: String,
    pub filename: Option<String>,
    pub data: Vec<u8>,              // 可能为空（Discord 只给 URL）
    pub source_url: Option<String>, // 平台 CDN URL
    pub storage_key: Option<String>,
    pub extracted_text: Option<String>,
    pub duration_secs: Option<u32>,
    pub size_bytes: Option<u64>,
    pub id: String,
}
```

### Message 层: MessageAttachment

定义在 `src/message/attachment.rs`，持久化到 Turn/ChatMessage 中：

```rust
pub struct MessageAttachment {
    pub kind: MediaKind,            // Audio / Image / Document
    pub mime_type: String,
    pub filename: Option<String>,
    #[serde(with = "base64_bytes")]
    pub data: Vec<u8>,              // 当前 WIP: 内联数据; 目标: 改为存储引用
}

pub enum MediaKind {
    Audio,
    Image,
    Document,
}
```

> **WIP 说明**：当前 `MessageAttachment.data` 直接内联二进制数据（临时方案）。目标架构是将 `data` 字段替换为 `attachment_id: String`，通过 `AttachmentManager.get_base64(id)` 按需获取。

### 存储层: Attachment 记录

定义在 `src/storage/models.rs`，DB 中的元数据记录：

```rust
pub struct Attachment {
    pub id: String,                 // UUID, 同时派生 storage key
    pub mime_type: String,
    pub kind: String,               // "image" | "audio" | "document"
    pub filename: Option<String>,
    pub size_bytes: Option<u64>,
    pub source_url: Option<String>, // 原始来源（留档）
    pub created_at: DateTime<Utc>,
}
```

## AttachmentManager

`src/attachment/mod.rs` — 附件持久化与检索的核心组件。

```rust
pub struct AttachmentManager {
    config: AttachmentStorageConfig,  // Local / S3
    db: Arc<Database>,
}
```

### 存储后端配置

```rust
pub enum AttachmentStorageConfig {
    Local { path: PathBuf },
    S3 {
        endpoint: Option<String>,
        bucket: String,
        prefix: Option<String>,
        region: Option<String>,
        access_key: String,
        secret_key: String,
    },
}
```

### API

| 方法 | 职责 |
|------|------|
| `create(IncomingAttachment) -> Attachment` | 下载(如需) + 写入存储 + 写入 DB 元数据 + 返回记录 |
| `get_base64(id) -> String` | 从存储读取字节 → base64 编码返回 |
| `delete(id)` | 删除存储文件 + DB 记录 |

### create 流程

```rust
pub fn create(&self, incoming: &IncomingAttachment) -> Result<Attachment> {
    // 1. 获取字节
    let bytes = if !incoming.data.is_empty() {
        incoming.data.clone()
    } else if let Some(url) = &incoming.source_url {
        http_download(url)?        // Discord CDN 等外部 URL
    } else {
        return Err(anyhow!("No data source"));
    };

    // 2. 生成 ID, 派生 storage key: {id}.{ext}
    let id = Uuid::new_v4().to_string();
    let ext = mime_to_ext(&incoming.mime_type);
    let key = format!("{}.{}", id, ext);

    // 3. 写入存储后端
    self.storage_write(&key, &bytes)?;

    // 4. 写入 DB 元数据
    let attachment = Attachment { id, mime_type, kind, filename, ... };
    self.db.insert_attachment(&attachment)?;

    // 5. 返回记录
    Ok(attachment)
}
```

### get_base64 流程

```rust
pub fn get_base64(&self, id: String) -> Result<String> {
    // 1. 从 DB 查元数据 (获取 mime_type 以派生 key)
    let record = self.db.get_attachment(&id)?;

    // 2. 派生 storage key
    let key = format!("{}.{}", id, mime_to_ext(&record.mime_type));

    // 3. 从存储读取字节
    let bytes = self.storage_read(&key)?;

    // 4. base64 编码返回
    Ok(base64::encode(&bytes))
}
```

### Storage Key 派生

Storage key 不单独存储，从 `id` + `mime_type` 实时派生：

```
key = {id}.{extension}
```

- `id`：UUID（DB 主键）
- `extension`：从 `mime_type` 推断（`image/jpeg` → `jpg`），未知时用 `bin`

## Binding 层集成

### 当前实现 (WIP)

`Binding::process_user_input()` 目前直接做类型映射，将 `IncomingAttachment.data` 克隆到 `MessageAttachment.data`：

```rust
// src/binding/mod.rs (当前 WIP 代码)
let attachments: Vec<MessageAttachment> = message.attachments.iter().map(|a| {
    MessageAttachment {
        kind: match a.kind { ... },
        mime_type: a.mime_type.clone(),
        filename: a.filename.clone(),
        data: a.data.clone(),           // 临时: 直接克隆字节
    }
}).collect();
```

### 目标实现

```rust
// src/binding/mod.rs (目标架构)
let attachments: Vec<MessageAttachment> = Vec::new();
for a in &message.attachments {
    // 1. 持久化到存储
    let record = self.attachment_manager.create(a)?;

    // 2. 构造 MessageAttachment (引用 ID, 不含原始数据)
    attachments.push(MessageAttachment {
        attachment_id: record.id,
        kind: match a.kind { ... },
        mime_type: a.mime_type.clone(),
        filename: a.filename.clone(),
    });
}
```

## LLM 消息转换

`message_convert::to_rig_message()` 需要在发送前从存储获取附件数据：

### 目标流程

```rust
fn attachment_to_user_content(
    a: &MessageAttachment,
    attachment_mgr: &AttachmentManager,
) -> Result<UserContent> {
    // 从存储取出 base64 数据
    let base64_data = attachment_mgr.get_base64(&a.attachment_id)?;
    let bytes = base64::decode(&base64_data)?;

    match a.kind {
        MediaKind::Image => UserContent::Image(Image {
            data: DocumentSourceKind::Raw(bytes),
            media_type: ImageMediaType::from_mime_type(&a.mime_type),
            ..
        }),
        MediaKind::Audio => UserContent::Audio(Audio {
            data: DocumentSourceKind::Raw(bytes),
            media_type: AudioMediaType::from_mime_type(&a.mime_type),
            ..
        }),
        MediaKind::Document => UserContent::Document(Document {
            data: DocumentSourceKind::Raw(bytes),
            media_type: DocumentMediaType::from_mime_type(&a.mime_type),
            ..
        }),
    }
}
```

### 当前实现 (WIP)

当前 `message_convert.rs` 直接使用 `MessageAttachment.data` 内联数据，不经过 AttachmentManager：

```rust
// 当前 WIP: 直接用内联数据
let data = if !a.data.is_empty() {
    DocumentSourceKind::Raw(a.data.clone())
} else {
    DocumentSourceKind::Unknown
};
```

## 序列化与持久化

### ChatMessage

```rust
pub struct ChatMessage {
    // ...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<MessageAttachment>,
}
```

### Turn

```rust
pub struct Turn {
    // ...
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<MessageAttachment>,
}
```

目标架构下，序列化到 SQLite 的只是附件引用（ID + 元数据），不包含二进制数据，避免 JSON 膨胀。

## 历史消息恢复

从 DB 恢复 session 时：

1. Turn/ChatMessage 反序列化得到 `MessageAttachment`（含 `attachment_id`）
2. 构建 LLM 上下文时，通过 `AttachmentManager.get_base64(id)` 按需加载附件数据
3. 附件数据不驻留内存，只在发送给 LLM 时临时加载

## 存储后端

### Local (文件系统)

```
{workspace_dir}/attachments/
├── {uuid1}.jpg
├── {uuid2}.pdf
└── {uuid3}.mp3
```

### S3 / MinIO

```
s3://{bucket}/{prefix}/{uuid}.{ext}
```

存储配置在 Binding 级别指定，每个 Binding 可以有独立的存储后端。未配置时默认使用 `{workspace_dir}/attachments/`。

## 数据库 Schema

```sql
CREATE TABLE attachments (
    id           TEXT PRIMARY KEY NOT NULL,
    mime_type    TEXT NOT NULL,
    kind         TEXT NOT NULL,        -- 'image' | 'audio' | 'document'
    filename     TEXT,
    size_bytes   INTEGER,
    source_url   TEXT,                 -- 原始来源 URL（留档）
    created_at   TEXT NOT NULL
);
```

> Storage key 不单独存储：由 `{id}.{ext}` 派生（ext 从 `mime_type` 推断）。

## 实现状态

| 组件 | 状态 | 说明 |
|------|------|------|
| `IncomingAttachment` | ✅ 已完成 | Channel 层类型定义 |
| `MessageAttachment` | ⚠️ WIP | 当前内联 data, 目标改为 attachment_id 引用 |
| `AttachmentManager` | ⚠️ 桩代码 | create/get_base64/delete 均为 todo!() |
| Binding 层转换 | ⚠️ WIP | 当前直接克隆 data, 目标调用 AttachmentManager.create() |
| message_convert | ⚠️ WIP | 当前直接用内联 data, 目标调用 AttachmentManager.get_base64() |
| DB Schema | ❌ 未创建 | attachments 表迁移待执行 |
| 存储后端 | ❌ 未实现 | Local/S3 写入读取逻辑 |

## 实现优先级

**Phase 1（最小可用）：**
1. `AttachmentStorageConfig` 实现存储后端构建
2. `AttachmentManager` 核心逻辑（create / get_base64 / delete）
3. DB schema + `attachments` 表迁移
4. Binding 层集成：`process_user_input()` 调用 `create()` 持久化
5. `message_convert` 集成：发送前调用 `get_base64()` 获取数据
6. `MessageAttachment` 字段从 `data: Vec<u8>` 改为 `attachment_id: String`

**Phase 2（完善）：**
1. HTTP 下载重试机制
2. 下载超时与大小限制（30s 连接, 5min 下载, 50MB 上限）
3. S3 存储后端测试
4. 附件清理（定期删除无引用的附件）

**Phase 3（增强）：**
1. 缩略图生成（图片附件）
2. 文本提取（PDF/文档 OCR）
3. `extracted_text` 字段填充

## 关键设计决策

### 为什么持久化优先而非内联数据

- **Discord CDN URL 会过期**：必须在收到消息时立即下载并持久化
- **避免 JSON 膨胀**：一张图片 base64 后约 1.3x 大小，直接存入 SQLite JSON 字段会导致 session 数据过大
- **按需加载**：历史消息恢复时不需要把所有附件数据加载到内存，只在发送给 LLM 时临时读取
- **存储可扩展**：从本地文件到 S3 只需改配置，不影响上层逻辑

### 为什么存储配置放在 Binding 级别

- 不同 Binding 不同需求：生产用 S3，开发/测试用本地
- 与 `workspace_dir` 对齐：`workspace_dir` 已是 Binding 级别配置
- 默认行为合理：未配置时用 `{workspace_dir}/attachments/`

### 为什么不在 Channel 层下载

- Channel 层应保持轻量，只负责协议适配
- 下载/重试/存储涉及业务逻辑，不应侵入 Channel
- 不同 Channel 附件来源不同（URL vs 内联 data），统一在 AttachmentManager 处理

## 文件清单

| 文件 | 职责 |
|------|------|
| `src/channel/message/attachment.rs` | Channel 层附件类型（IncomingAttachment, AttachmentKind） |
| `src/message/attachment.rs` | Message 层附件类型（MessageAttachment, MediaKind） |
| `src/attachment/mod.rs` | AttachmentManager + AttachmentStorageConfig |
| `src/message/message.rs` | ChatMessage 定义，包含 attachments 字段 |
| `src/session/turn.rs` | Turn 定义，包含 attachments 字段 |
| `src/binding/mod.rs` | Channel → AttachmentManager.create() → MessageAttachment |
| `src/binding/message_convert.rs` | AttachmentManager.get_base64() → rig SDK UserContent |
| `src/storage/models.rs` | Attachment / NewAttachment DB 模型 |

## 相关文档

- [Channel 系统](channel-system.md) — IncomingAttachment 定义与消息流
- [会话持久化](storage.md) — Database 层设计与 Diesel 模型
- [Binding Loop](binding-loop.md) — 消息处理循环
- [Session 模块](session.md) — Turn 与附件的关联关系