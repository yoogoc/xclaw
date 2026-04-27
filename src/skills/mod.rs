pub mod skill;
pub mod skill_load_tool;

pub use skill::*;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tokio::sync::RwLock;

pub struct SkillManager {
    available: HashMap<String, SkillDefinition>,
    active: RwLock<HashMap<String, String>>,
    skills_dir: PathBuf,
}

impl SkillManager {
    pub fn new(skills_dir: PathBuf) -> Self {
        Self {
            available: HashMap::new(),
            active: RwLock::new(HashMap::new()),
            skills_dir,
        }
    }

    pub fn scan(&mut self) {
        if !self.skills_dir.exists() {
            info!("Skills directory does not exist: {}", self.skills_dir.display());
            return;
        }

        let entries = match std::fs::read_dir(&self.skills_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read skills directory: {}", e);
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            match std::fs::read_to_string(&skill_md) {
                Ok(content) => {
                    match skill::parse_frontmatter(&content) {
                        Ok((fm, _body)) => {
                            info!("Registered skill: {} - {}", fm.name, fm.description);
                            self.available.insert(fm.name.clone(), SkillDefinition {
                                name: fm.name,
                                description: fm.description,
                                tools: fm.tools,
                                dir: path.clone(),
                            });
                        }
                        Err(e) => {
                            warn!("Failed to parse {}: {}", skill_md.display(), e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read {}: {}", skill_md.display(), e);
                }
            }
        }

        info!("Scanned {} skill(s)", self.available.len());
    }

    pub async fn load(&self, name: &str) -> anyhow::Result<LoadResult> {
        let def = self.available.get(name)
            .ok_or_else(|| anyhow::anyhow!("Skill '{}' not found", name))?;

        {
            let active = self.active.read().await;
            if active.contains_key(name) {
                return Ok(LoadResult {
                    prompt: active.get(name).cloned().unwrap_or_default(),
                    tool_names: def.tools.clone(),
                });
            }
        }

        let content = tokio::fs::read_to_string(def.dir.join("SKILL.md")).await?;
        let (_fm, body) = skill::parse_frontmatter(&content)?;

        let mut full_prompt = body;

        let refs_dir = def.dir.join("references");
        if refs_dir.exists() {
            if let Ok(mut entries) = tokio::fs::read_dir(&refs_dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    if path.extension().map_or(false, |e| e == "md") {
                        if let Ok(ref_content) = tokio::fs::read_to_string(&path).await {
                            let filename = entry.file_name().to_string_lossy().to_string();
                            full_prompt.push_str(&format!("\n\n#### Reference: {}\n{}", filename, ref_content));
                        }
                    }
                }
            }
        }

        self.active.write().await.insert(name.to_string(), full_prompt.clone());

        info!("Loaded skill: {}", name);
        Ok(LoadResult {
            prompt: full_prompt,
            tool_names: def.tools.clone(),
        })
    }

    pub async fn unload(&self, name: &str) -> anyhow::Result<()> {
        self.active.write().await.remove(name);
        Ok(())
    }

    pub fn available_catalog(&self) -> String {
        if self.available.is_empty() {
            return String::new();
        }
        self.available.values()
            .map(|s| format!("- **{}**: {}", s.name, s.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub async fn active_prompts(&self) -> Vec<(String, String)> {
        let active = self.active.read().await;
        active.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    pub async fn active_tool_names(&self) -> HashSet<String> {
        let active = self.active.read().await;
        let mut names = HashSet::new();
        for name in active.keys() {
            if let Some(def) = self.available.get(name) {
                for tool in &def.tools {
                    names.insert(tool.clone());
                }
            }
        }
        names
    }

    pub async fn is_active(&self, name: &str) -> bool {
        self.active.read().await.contains_key(name)
    }

    pub fn is_empty(&self) -> bool {
        self.available.is_empty()
    }
}