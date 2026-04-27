use crate::errors::tool::ToolError;
use crate::skills::SkillManager;
use crate::tools::{Tool, ToolOutput};
use std::sync::Arc;

pub struct SkillLoadTool {
    skill_manager: Arc<SkillManager>,
}

impl SkillLoadTool {
    pub fn new(skill_manager: Arc<SkillManager>) -> Self {
        Self { skill_manager }
    }
}

#[async_trait]
impl Tool for SkillLoadTool {
    fn name(&self) -> &str {
        "skill_load"
    }

    fn description(&self) -> &str {
        "Load a skill by name. After loading, the skill's prompt and tools become available."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name of the skill to load"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> anyhow::Result<ToolOutput, ToolError> {
        let name = params.get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParameters("missing 'name' parameter".to_string()))?;

        let start = std::time::Instant::now();
        match self.skill_manager.load(name).await {
            Ok(result) => {
                let output = serde_json::json!({
                    "success": true,
                    "skill": name,
                    "prompt_loaded": !result.prompt.is_empty(),
                    "tools": result.tool_names,
                });
                Ok(ToolOutput::success(output, start.elapsed()))
            }
            Err(e) => {
                let output = serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                });
                Ok(ToolOutput::success(output, start.elapsed()))
            }
        }
    }
}