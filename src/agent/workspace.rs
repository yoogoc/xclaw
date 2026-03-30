use std::path::PathBuf;

pub struct Workspace {
    pub name: String,
    pub path: String,
}

impl Workspace {
    pub fn new(path: PathBuf) -> Self {
        Self {
            name: path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("workspace")
                .to_string(),
            path: path.to_string_lossy().to_string(),
        }
    }
}
