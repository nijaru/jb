use crate::SkillsAction;
use anyhow::Result;
use std::path::PathBuf;

const SKILL_CONTENT: &str = include_str!("../../skills/SKILL.md");

pub async fn execute(action: Option<SkillsAction>) -> Result<()> {
    match action {
        Some(SkillsAction::Install { path }) => {
            let skills_dir = path.unwrap_or_else(get_default_skills_dir);
            install_skills(&skills_dir)
        }
        Some(SkillsAction::Show) => {
            print!("{SKILL_CONTENT}");
            Ok(())
        }
        None => {
            let skills_dir = get_default_skills_dir();
            install_skills(&skills_dir)
        }
    }
}

fn get_default_skills_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("skills")
        .join("jb")
}

fn install_skills(skills_dir: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(skills_dir)?;

    let skill_path = skills_dir.join("SKILL.md");
    std::fs::write(&skill_path, SKILL_CONTENT)?;

    let display_path = if skill_path.starts_with(dirs::home_dir().unwrap_or_default()) {
        skill_path.to_string_lossy().replacen(
            &dirs::home_dir().unwrap().to_string_lossy().to_string(),
            "~",
            1,
        )
    } else {
        skill_path.to_string_lossy().to_string()
    };

    println!("Installed jb skill to {display_path}");
    println!();
    println!("For proactive usage, add to ~/.claude/CLAUDE.md:");
    println!();
    println!("  **Background Jobs:** `jb run \"cmd\"` for long-running commands.");
    println!();
    println!("For other agents: jb skills show > /path/to/config");

    Ok(())
}
