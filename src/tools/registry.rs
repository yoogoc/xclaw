use crate::tools::file_read_tool::FileRead;
use crate::tools::tool::Tool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let registry = ToolRegistry {
            tools: RwLock::new(HashMap::new()),
        };

        registry.register_sync(Arc::new(FileRead::new()));

        registry
    }

    pub fn register_sync(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        if let Ok(mut tools) = self.tools.try_write() {
            tools.insert(name.clone(), tool);
        }
    }

    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let tools = self.tools.read().await;
        tools.get(name).map(Arc::clone)
    }

    pub async fn tools(&self) -> HashMap<String, Arc<dyn Tool>> {
        let tools = self.tools.read().await;
        tools.clone()
    }

    pub async fn all_tools(&self) -> Vec<Arc<dyn Tool>> {
        let tools = self.tools.read().await;
        tools.clone().values().cloned().collect()
    }
}
