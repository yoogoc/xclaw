# Skill 系统设计

## 1. 概述

Skill 是 Agent 的可插拔能力单元，以**目录**为单位定义（参考 Claude Code 的 `.claude/skills/` 模式）。每个 Skill 目录包含一个 `SKILL.md` 主定义文件和可选的参考资料文件。

每个 Skill 可以同时提供：
- **Prompt 注入**：`SKILL.md` 正文作为行为指导注入 system prompt
- **参考资料**：`references/` 子目录中的文件提供额外上下文
- **Tool 声明**：frontmatter 中声明该 Skill 依赖的已注册 Tool

核心特点：
- **目录即 Skill**：一个目录就是一个 Skill，目录名即标识符
- **按需加载**：所有可用 Skill 的名称和描述在 system prompt 中注入，LLM 通过 `skill_load` 工具按需激活
- **Tool 保持不变**：Skill 不改变现有 Tool 系统，只声明对已注册 Tool 的引用关系
- **无需重编译**：新增/修改 Skill 只需编辑 `.md` 文件

## 2. 与 Tool 的区别

| 维度 | Tool | Skill |
|------|------|-------|
| 定义方式 | Rust 代码（实现 `Tool` trait） | 目录 + SKILL.md（Markdown + YAML frontmatter） |
| 粒度 | 单个函数 | 能力包（prompt + 参考资料 + tool 声明） |
| 生命周期 | 启动时注册，全程可用 | 按需加载，运行时动态激活 |
| 作用范围 | 仅提供可调用函数 | 同时影响 LLM 行为模式和能力 |
| 修改方式 | 需要重编译 | 编辑 .md 文件即可 |

## 3. 目录结构

```
workspace/skills/
├── code_review/
│   ├── SKILL.md              # 主定义文件（必须）
│   └── references/           # 参考资料（可选）
│       ├── style_guide.md
│       └── security_checklist.md
├── web_browse/
│   ├── SKILL.md
│   └── references/
│       └── url_patterns.md
├── task_management/
│   └── SKILL.md              # 最小 Skill：只有 SKILL.md
└── writing_style/
    └── SKILL.md
```

## 4. SKILL.md 格式

### YAML Frontmatter

```yaml
---
name: code_review
description: 代码审查能力，提供代码规范检查和改进建议
tools:                        # 声明依赖的已注册 Tool（可选）
  - file_read
  - file_write
---
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 唯一标识符，用于 `skill_load("name")` |
| `description` | string | 是 | 一行描述，注入 system prompt 供 LLM 判断何时加载 |
| `tools` | string[] | 否 | 依赖的 Tool 名称（必须是 ToolRegistry 中已注册的） |

### Markdown 正文

Frontmatter 之后的 Markdown 正文是 **prompt 注入内容**，激活 Skill 后追加到 system prompt。

### 完整示例

`workspace/skills/code_review/SKILL.md`:

```markdown
---
name: code_review
description: 代码审查能力，提供代码规范检查和改进建议
tools:
  - file_read
---

你现在具有代码审查能力。在审查代码时：

## 审查要点
- 关注安全漏洞、性能问题、可维护性
- 使用具体的行号引用
- 给出修改建议而非仅指出问题

## 工作流程
1. 使用 file_read 读取需要审查的文件
2. 逐文件分析代码质量
3. 汇总问题并给出改进建议
```

### references/ 目录

`references/` 中的 `.md` 文件在 Skill 激活时**一并加载**，追加到 prompt 中。例如：

`workspace/skills/code_review/references/style_guide.md`:
```markdown
# 代码风格规范

- 函数不超过 50 行
- 嵌套不超过 3 层
- 命名使用 snake_case
```

激活后 system prompt 中会包含：
1. `SKILL.md` 正文
2. 每个 reference 文件内容（以 `#### Reference: {filename}` 为标题）

## 5. 架构

```
┌─────────────────────────────────────────────────────┐
│                    Agent Loop                        │
│                                                     │
│  ┌──────────────────┐    ┌────────────────────────┐ │
│  │  SkillManager    │    │   ToolRegistry         │ │
│  │                  │    │   (所有 Tool 始终注册)  │ │
│  │  available:      │    │                        │ │
│  │   (从目录扫描)   │    │  + skill_load tool     │ │
│  │   [code_review,  │    │                        │ │
│  │    web_browse,   │    └────────────────────────┘ │
│  │    deploy, ...]  │                               │
│  │                  │                               │
│  │  active:         │                               │
│  │   {code_review}  │                               │
│  │        │         │                               │
│  │        ├── SKILL.md 正文 ──→ system_prompt       │
│  │        ├── references/* ───→ system_prompt       │
│  │        └── tools 声明 ─────→ tool 可见性过滤     │
│  └──────────────────┘                               │
│                                                     │
│  system_prompt 始终包含:                             │
│   - 所有可用 Skill 的 name + description            │
│   - 已激活 Skill 的完整 prompt + references         │
└─────────────────────────────────────────────────────┘
```

### 数据流

```
1. Agent 启动
   └── SkillManager 扫描 workspace/skills/*/SKILL.md
       ├── 解析每个 SKILL.md 的 YAML frontmatter
       └── 注册为可用 Skill（只读 name + description，不加载正文）
   └── 注册 skill_load Tool 到 ToolRegistry

2. 每次 LLM 调用前（build_system_prompt）
   └── 注入所有可用 Skill 的 name + description
   └── 注入已激活 Skill 的完整内容（SKILL.md 正文 + references/*）

3. LLM 判断需要某种能力
   └── LLM 调用 skill_load("code_review")
       ├── 读取 workspace/skills/code_review/SKILL.md 正文
       ├── 读取 workspace/skills/code_review/references/*.md
       └── 记录 frontmatter.tools 声明

4. 后续 LLM 调用
   └── system prompt 包含 code_review 的完整 prompt 内容
   └── tool 列表中 code_review 声明的 tool 变为可见
```

## 6. Tool 可见性

**Tool 系统保持不变**。所有 Tool 在 ToolRegistry 中始终注册、始终可执行。Skill 的 `tools` 声明只影响 LLM 的 tool 列表可见性：

- **Base tools**（file_read、file_write、web_search 等）：始终对 LLM 可见
- **Skill 声明的 tools**：只有对应 Skill 激活后才对 LLM 可见

`execute_tools()` 不受限制 — 所有注册的 Tool 都可执行。

> **Phase 1 简化**：可以让所有 Tool 始终可见，不做过滤。Tool 可见性控制留到 Phase 2。

## 7. 类型定义

### SkillDefinition

```rust
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,        // 声明依赖的 Tool 名称
    pub dir: PathBuf,              // Skill 目录路径
}
```

### SkillManager

```rust
pub struct SkillManager {
    /// 可用 Skill（从 workspace/skills/*/ 扫描）
    available: HashMap<String, SkillDefinition>,
    /// 已激活 Skill：name → 加载的完整 prompt 内容
    active: RwLock<HashMap<String, String>>,
    /// Skills 根目录
    skills_dir: PathBuf,
}
```

**API**:

| 方法 | 说明 |
|------|------|
| `new(skills_dir) -> Self` | 创建 manager |
| `scan(&mut self)` | 扫描 skills_dir/*/SKILL.md，解析 frontmatter |
| `load(&self, name) -> Result<LoadResult>` | 激活 Skill：读取正文 + references |
| `unload(&self, name) -> Result<()>` | 停用 Skill |
| `available_catalog() -> String` | 生成可用 Skill 目录文本 |
| `active_prompts() -> Vec<(String, String)>` | 已激活 Skill 的 (name, prompt) |
| `active_tool_names() -> HashSet<String>` | 已激活 Skill 声明的 Tool 名称 |
| `is_active(&self, name) -> bool` | 查询是否已激活 |

### LoadResult

```rust
pub struct LoadResult {
    pub prompt: String,            // SKILL.md 正文 + references 内容
    pub tool_names: Vec<String>,   // 声明的 Tool 名称
}
```

### load() 流程

```rust
fn load(&self, name: &str) -> Result<LoadResult> {
    let def = self.available.get(name)?;

    // 1. 读取 SKILL.md 正文（跳过 frontmatter）
    let skill_md = fs::read_to_string(def.dir.join("SKILL.md"))?;
    let body = strip_frontmatter(&skill_md);

    // 2. 读取 references/*.md
    let mut full_prompt = body;
    let refs_dir = def.dir.join("references");
    if refs_dir.exists() {
        for entry in fs::read_dir(&refs_dir)? {
            if entry.path().extension() == Some("md") {
                let content = fs::read_to_string(entry.path())?;
                let filename = entry.file_name();
                full_prompt += &format!("\n\n#### Reference: {}\n{}", filename, content);
            }
        }
    }

    // 3. 记录到 active
    self.active.write().insert(name.to_string(), full_prompt.clone());

    Ok(LoadResult {
        prompt: full_prompt,
        tool_names: def.tools.clone(),
    })
}
```

## 8. 专用 Tool: skill_load

只有一个专用 Tool。所有可用 Skill 的名称和描述已在 system prompt 中注入，LLM 直接判断何时加载。

```
名称: skill_load
描述: 按需加载一个 Skill，激活其 prompt 注入和 tool 声明
参数: { name: string }
返回: { success: bool, prompt_loaded: bool, tools: [string], references_loaded: int }
```

幂等：已激活的 Skill 重复调用不会重复注入。

## 9. 集成点

### 9.1 system prompt

`build_system_prompt()` 追加：

```rust
// 1. 可用 Skill 目录（始终注入）
let catalog = self.skill_manager.available_catalog();
if !catalog.is_empty() {
    parts.push(format!(
        "## Available Skills\n以下 Skill 可通过 skill_load 工具按需激活：\n{}",
        catalog
    ));
}

// 2. 已激活 Skill 的 prompt 内容
for (name, prompt) in self.skill_manager.active_prompts() {
    parts.push(format!("## Skill: {}\n{}", name, prompt));
}
```

`available_catalog()` 生成格式：
```
- **code_review**: 代码审查能力，提供代码规范检查和改进建议
- **web_browse**: 网页浏览能力，可以访问和解析网页内容
```

### 9.2 Agent 构建

```rust
let skills_dir = workspace_dir.join("skills");
let mut skill_manager = SkillManager::new(skills_dir);
skill_manager.scan();

let skill_load_tool = SkillLoadTool::new(Arc::clone(&skill_manager));
tool_registry.register(Arc::new(skill_load_tool));
```

## 10. Skill 示例

### 最小 Skill（纯 Prompt）

`workspace/skills/writing_style/SKILL.md`:
```markdown
---
name: writing_style
description: 专业写作风格，适用于撰写文档和技术文章
---

你现在使用专业写作风格：
- 简洁清晰，避免冗余
- 重要结论放在段落开头
- 使用恰当的过渡词
```

### 完整 Skill（Prompt + References + Tool 声明）

```
workspace/skills/code_review/
├── SKILL.md
└── references/
    ├── style_guide.md
    └── security_checklist.md
```

### Tool-only 声明（无 Prompt 正文）

`workspace/skills/advanced_tools/SKILL.md`:
```markdown
---
name: advanced_tools
description: 启用高级搜索和分析工具
tools:
  - regex_search
  - semantic_search
---
```

## 11. 配置

```yaml
agents:
  - name: main
    llm: claude
    default_skills: true    # 扫描 workspace/skills/ 目录
    skills: []              # 预留：未来可支持远程 Skill
```

`default_skills: true` → 扫描 `workspace/skills/*/SKILL.md`

目录存在即为"可用"，不会自动"激活"。激活需要 LLM 调用 `skill_load`。

## 12. 文件结构

```
src/skills/
├── mod.rs              # SkillManager, SkillDefinition, scan/load
└── skill_load_tool.rs  # skill_load Tool 实现

workspace/skills/       # Skill 定义目录（用户可编辑）
├── code_review/
│   ├── SKILL.md
│   └── references/
└── writing_style/
    └── SKILL.md
```

## 13. 实现优先级

**Phase 1（框架）：**
1. `SkillDefinition` 结构和 YAML frontmatter 解析
2. `SkillManager` 核心（scan 目录、load 正文+references、catalog 生成）
3. `skill_load` Tool
4. 集成到 `build_system_prompt()`（注入可用目录 + 已激活 prompt）
5. 注入到 Binding

**Phase 2（优化）：**
1. Tool 可见性过滤（只暴露已激活 Skill 声明的 tool）
2. 热重载（目录变更后自动更新可用列表）
3. 编写默认 Skill

## 14. 关键设计决策

### 为什么目录而非单文件

- **可扩展**：一个 Skill 可以包含多个参考资料文件
- **与 Claude Code 一致**：参考 `.claude/skills/{name}/SKILL.md` 模式
- **清晰的组织**：目录名即 Skill 名，结构一目了然
- **未来可扩展**：目录中可以放脚本、配置、模板等

### Tool 保持不变

- Skill 的 `tools` 字段是**声明性引用**，不定义新 Tool
- Tool 的实现（Rust `Tool` trait）完全独立于 Skill
- 多个 Skill 可以声明同一个 Tool，不会冲突

### system prompt 注入目录 + 只用一个 skill_load

- LLM 从 prompt 中直接看到所有可用 Skill，不需要额外 tool call
- 只有一个 `skill_load` Tool，最小化 tool 列表噪音
- 加载通过 Tool Call 记录，可审计

### 延迟加载正文

- scan 只读 frontmatter（name + description），不读正文
- 正文 + references 在 `skill_load` 时才读取
- 避免启动时加载所有 Skill 内容到内存

## 15. 相关文档

- [工具系统](tools.md) — Tool trait 和 ToolRegistry 设计
- [Binding Loop](binding-loop.md) — Agent 循环和 Tool 执行流程
- [Agent 定义](agent-definition.md) — SOUL.md / TOOL.md 配置驱动模式
- [配置](config.md) — YAML 配置结构