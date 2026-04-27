use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub dir: PathBuf,
}

pub struct LoadResult {
    pub prompt: String,
    pub tool_names: Vec<String>,
}

pub fn parse_frontmatter(content: &str) -> anyhow::Result<(SkillFrontmatter, String)> {
    let content = content.trim();
    if !content.starts_with("---") {
        anyhow::bail!("SKILL.md must start with YAML frontmatter (---)");
    }

    let after_first = &content[3..];
    let end = after_first.find("---")
        .ok_or_else(|| anyhow::anyhow!("Missing closing --- in frontmatter"))?;

    let yaml_str = &after_first[..end];
    let body = after_first[end + 3..].trim().to_string();

    let frontmatter: SkillFrontmatter = serde_yaml::from_str(yaml_str)?;
    Ok((frontmatter, body))
}