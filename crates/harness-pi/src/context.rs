//! Context-file autoload: walks from CWD upward collecting `AGENTS.md` / `CLAUDE.md`
//! files and the user's `~/.pi/SYSTEM.md`, concatenating them into a system prompt.

use std::path::{Path, PathBuf};

const FILENAMES: &[&str] = &["AGENTS.md", "CLAUDE.md"];

pub fn load_system_prompt() -> anyhow::Result<String> {
    let mut parts = Vec::new();

    if let Some(home) = std::env::var_os("HOME") {
        let user_sys = PathBuf::from(home).join(".pi").join("SYSTEM.md");
        if user_sys.exists() {
            parts.push(std::fs::read_to_string(&user_sys)?);
        }
    }

    let cwd = std::env::current_dir()?;
    let mut ancestors: Vec<&Path> = cwd.ancestors().collect();
    ancestors.reverse(); // root-first so nearer files win when duplicated
    for dir in ancestors {
        for name in FILENAMES {
            let p = dir.join(name);
            if p.exists() {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    parts.push(s);
                }
            }
        }
    }

    Ok(parts.join("\n\n"))
}
