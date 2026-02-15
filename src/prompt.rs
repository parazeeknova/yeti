pub const SYSTEM_PROMPT: &str = r#"You are Yeti, a fast git commit message generator powered by Cerebras.
Generate a conventional commit message based on the staged changes.

RULES:
- First line: type(scope): description (max 72 chars)
- Optional body: 1-3 lines explaining WHAT and WHY (not HOW)
- Use types: feat, fix, refactor, docs, test, chore, perf, ci, build, style, revert
- Infer scope from primary file path (e.g., src/auth/login.rs -> "auth")
- Be concise but descriptive
- NO markdown, NO code blocks, NO thinking tags
- NO explanations outside the commit message
- DO NOT include any prefix like "commit:" or "message:"
- Output ONLY the commit message, nothing else

OUTPUT FORMAT:
<type>(<scope>): <description>

[optional body line 1]
[optional body line 2]"#;

pub fn build_user_prompt(branch: &str, files: &[FileInfo]) -> String {
    let file_list = files
        .iter()
        .take(30)
        .map(|f| {
            let change_type = match (f.additions > 0, f.deletions > 0) {
                (true, true) => "modified",
                (true, false) => "added",
                (false, true) => "deleted",
                (false, false) => "changed",
            };
            format!(
                "- {} ({}: +{}/-{})",
                f.path, change_type, f.additions, f.deletions
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let extra = if files.len() > 30 {
        format!("\n... and {} more files", files.len() - 30)
    } else {
        String::new()
    };

    let diff_hint = if files.len() <= 5 {
        let diffs: Vec<String> = files
            .iter()
            .filter_map(|f| {
                if !f.diff.is_empty() {
                    Some(format!("--- {}\n{}", f.path, f.diff))
                } else {
                    None
                }
            })
            .collect();
        if !diffs.is_empty() {
            format!("\n\nDiffs:\n{}", diffs.join("\n\n"))
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    format!(
        "Branch: {}\n\nFiles changed ({}):\n{}{}\n\nGenerate a commit message.",
        branch,
        files.len(),
        file_list,
        extra
    ) + &diff_hint
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub additions: usize,
    pub deletions: usize,
    pub diff: String,
    pub status: FileStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

impl FileStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileStatus::Added => "A",
            FileStatus::Modified => "M",
            FileStatus::Deleted => "D",
            FileStatus::Renamed => "R",
        }
    }
}
