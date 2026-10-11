//! Skill metadata prompt generation (progressive disclosure level 1) and
//! injection into system prompts.

use std::path::Path;

use wf_types::skill::SkillMetadata;

use crate::skill_fs::SkillFileLoader;

/// Placeholder replaced by [`inject_skill_metadata`]. Canonical text lives
/// in the shared template module so every injection stage references one spelling.
pub const SKILLS_METADATA_PLACEHOLDER: &str = wf_common::template::SKILLS_METADATA_PLACEHOLDER;

/// Generate the metadata prompt listing all enabled skills
/// (progressive disclosure level 1).
pub fn generate_skill_metadata_prompt(skills: &[SkillMetadata]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut lines = vec!["Available skills:".to_string()];
    for skill in skills {
        let mut line = format!("  - {}: {}", skill.name, skill.description);
        if let Some(version) = &skill.version {
            line.push_str(&format!(" (v{})", version));
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// Inject the skill metadata prompt into a system prompt: replaces the
/// `{SKILLS_METADATA}` placeholder when present, otherwise appends the
/// metadata at the end. Runs after template rendering. Returns the
/// (possibly unchanged) prompt.
pub fn inject_skill_metadata(system_prompt: &str, enabled_skills: &[SkillMetadata]) -> String {
    let metadata_prompt = generate_skill_metadata_prompt(enabled_skills);
    crate::tool_description_generator::inject_placeholder_block(
        system_prompt,
        SKILLS_METADATA_PLACEHOLDER,
        &metadata_prompt,
    )
}

pub(super) fn collect_relative_files(
    loader: &dyn SkillFileLoader,
    dir: &Path,
    root: &Path,
    out: &mut Vec<String>,
) {
    let Ok(entries) = loader.read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = dir.join(&entry.name);
        if entry.is_dir {
            collect_relative_files(loader, &path, root, out);
        } else {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().to_string());
            }
        }
    }
}
