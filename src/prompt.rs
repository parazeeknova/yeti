pub const SYSTEM_PROMPT: &str = r#"Output ONLY a commit message. No markdown. No code blocks. No explanations. No preamble.

Generate a conventional commit message with this exact format:

<type>[SCOPE]: <summary>

<body paragraph>

Rules:
- type: feat, fix, refactor, docs, test, chore, perf, ci, build, style, or revert
- SCOPE: UPPERCASE module name from file paths (e.g., AUTH, API, DB, TUI, CORE)
- summary: imperative mood, max 50 chars, describe what changed (no period)
- body: single paragraph, explain WHAT and WHY, reference affected components

Examples:

feat[AUTH]: add OAuth2 login flow

Implement Google OAuth2 provider with JWT token generation and session management. Update auth middleware to validate tokens and handle refresh flows.

fix[API]: resolve null pointer in user handler

Add null check before accessing user preferences in profile endpoint. Prevents crash when user record exists but preferences not initialized."#;

pub fn build_user_prompt(branch: &str, files: &[FileInfo]) -> String {
    let file_list = files
        .iter()
        .take(30)
        .map(|f| {
            let change_type = match f.status {
                FileStatus::Added => "added",
                FileStatus::Deleted => "deleted",
                FileStatus::Renamed => "renamed",
                FileStatus::Modified => "modified",
            };
            let rename_suffix = match &f.old_path {
                Some(old) => format!(" (from {})", old),
                None => String::new(),
            };
            format!(
                "- {}{} ({}: +{}/-{})",
                f.path, rename_suffix, change_type, f.additions, f.deletions
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let change_tree = build_change_tree(files);

    let extra = if files.len() > 30 {
        format!("\n... and {} more files", files.len() - 30)
    } else {
        String::new()
    };

    let diff_hint = build_patch_context(files);

    format!(
        "Branch: {}\n\nFiles changed ({}):\n{}{}\n\nChange tree:\n{}\n\nUse this staged diff context (including renames/moves) to generate the exact commit message.\n\nGenerate a commit message.",
        branch,
        files.len(),
        file_list,
        extra,
        change_tree
    ) + &diff_hint
}

fn build_change_tree(files: &[FileInfo]) -> String {
    if files.is_empty() {
        return "(none)".to_string();
    }

    let mut sorted = files.to_vec();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let mut lines = Vec::new();
    let mut seen_dirs = std::collections::BTreeSet::new();

    for file in &sorted {
        let parts: Vec<&str> = file.path.split('/').collect();
        let mut current = String::new();
        for (depth, dir) in parts.iter().take(parts.len().saturating_sub(1)).enumerate() {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(dir);
            if seen_dirs.insert(current.clone()) {
                lines.push(format!("{}{}{}/", "  ".repeat(depth), "", dir));
            }
        }

        let depth = parts.len().saturating_sub(1);
        let status = match file.status {
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
            FileStatus::Modified => "M",
        };
        let rename_note = file
            .old_path
            .as_ref()
            .map(|old| format!(" <- {}", old))
            .unwrap_or_default();
        lines.push(format!(
            "{}- [{}] {}{}",
            "  ".repeat(depth),
            status,
            parts.last().unwrap_or(&file.path.as_str()),
            rename_note
        ));
    }

    lines.join("\n")
}

fn build_patch_context(files: &[FileInfo]) -> String {
    let mut used = 0usize;
    let mut patches = Vec::new();
    let max_total = 14_000usize;
    let max_file = 2_200usize;

    for file in files {
        if file.diff.is_empty() {
            continue;
        }

        let title = if let Some(old) = &file.old_path {
            format!("--- {} (renamed from {})\n", file.path, old)
        } else {
            format!("--- {}\n", file.path)
        };
        let mut body = file.diff.clone();
        if body.len() > max_file {
            body.truncate(max_file);
            body.push_str("\n...[truncated]");
        }

        let mut entry = format!("{}{}", title, body);
        if used + entry.len() > max_total {
            let remaining = max_total.saturating_sub(used);
            if remaining == 0 {
                break;
            }
            entry.truncate(remaining);
            entry.push_str("\n...[truncated]");
            patches.push(entry);
            break;
        }

        used += entry.len();
        patches.push(entry);
    }

    if patches.is_empty() {
        String::new()
    } else {
        format!("\n\nStaged patch excerpts:\n{}", patches.join("\n\n"))
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
    pub diff: String,
    pub status: FileStatus,
    pub old_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[cfg(test)]
mod tests {
    use super::{build_user_prompt, FileInfo, FileStatus};

    fn file(
        path: &str,
        status: FileStatus,
        additions: usize,
        deletions: usize,
        diff: &str,
        old_path: Option<&str>,
    ) -> FileInfo {
        FileInfo {
            path: path.to_string(),
            additions,
            deletions,
            diff: diff.to_string(),
            status,
            old_path: old_path.map(|s| s.to_string()),
        }
    }

    #[test]
    fn user_prompt_includes_files_changed_section_with_status_and_rename() {
        let files = vec![
            file(
                "src/new.rs",
                FileStatus::Added,
                8,
                0,
                "+fn new() {}\n",
                None,
            ),
            file(
                "src/current.rs",
                FileStatus::Renamed,
                2,
                2,
                "-old\n+new\n",
                Some("src/old.rs"),
            ),
            file(
                "src/obsolete.rs",
                FileStatus::Deleted,
                0,
                3,
                "-gone\n",
                None,
            ),
        ];

        let prompt = build_user_prompt("feature/refactor", &files);

        assert!(prompt.contains("Files changed (3):"));
        assert!(prompt.contains("- src/new.rs (added: +8/-0)"));
        assert!(prompt.contains("- src/current.rs (from src/old.rs) (renamed: +2/-2)"));
        assert!(prompt.contains("- src/obsolete.rs (deleted: +0/-3)"));
    }

    #[test]
    fn user_prompt_includes_change_tree_section() {
        let files = vec![
            file(
                "src/tui/app.rs",
                FileStatus::Modified,
                1,
                1,
                "-a\n+b\n",
                None,
            ),
            file("src/prompt.rs", FileStatus::Modified, 2, 0, "+c\n", None),
        ];

        let prompt = build_user_prompt("main", &files);

        assert!(prompt.contains("Change tree:"));
        assert!(prompt.contains("src/"));
        assert!(prompt.contains("  tui/"));
        assert!(prompt.contains("    - [M] app.rs"));
        assert!(prompt.contains("  - [M] prompt.rs"));
    }

    #[test]
    fn user_prompt_includes_staged_patch_excerpts_and_truncates_long_diff() {
        let long_diff = format!("+{}\n", "x".repeat(2500));
        let files = vec![file(
            "src/huge.rs",
            FileStatus::Modified,
            120,
            4,
            &long_diff,
            None,
        )];

        let prompt = build_user_prompt("main", &files);

        assert!(prompt.contains("Staged patch excerpts:"));
        assert!(prompt.contains("--- src/huge.rs"));
        assert!(prompt.contains("...[truncated]"));
    }
}
