use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::tools::tool::Tool;

pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let tools = self.tools.read().await;
        tools.get(name).map(Arc::clone)
    }

    pub async fn tools(&self) -> HashMap<String, Arc<dyn Tool>> {
        let tools = self.tools.read().await;
        tools.clone()
    }
}
