use crate::SkillsAction;
use anyhow::Result;
use std::path::PathBuf;

const SKILL_CONTENT: &str = include_str!("../../../../skills/skill.md");

pub async fn execute(action: Option<SkillsAction>) -> Result<()> {
    let skills_dir = get_skills_dir()?;

    match action {
        Some(SkillsAction::Install) | None => install_skills(&skills_dir),
    }
}

fn get_skills_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    Ok(home.join(".claude").join("skills").join("jb"))
}

fn install_skills(skills_dir: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(skills_dir)?;

    let skill_path = skills_dir.join("skill.md");
    std::fs::write(&skill_path, SKILL_CONTENT)?;

    println!("Installed skills to {}", skills_dir.display());
    Ok(())
}
