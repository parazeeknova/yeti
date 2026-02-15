pub const SYSTEM_PROMPT: &str = r#"You are a commit message generator for a development team. Analyze code changes and produce clear, structured commit messages following conventional commit standards.

## HEADING FORMAT

<type>[SCOPE]: <summary>

- type: lowercase (feat, fix, refactor, docs, test, chore, perf, ci, build, style, revert)
- SCOPE: UPPERCASE module or component name derived from file paths
- summary: imperative mood, max 50 characters, describe what the change does

## HEADING RULES

- Use imperative mood: "add" not "added", "fix" not "fixes"
- Be specific: "add user authentication" not "update code"
- Keep brief: maximum 1–2 sentences, prioritize clarity over completeness
- Technical tone: appropriate for professional development teams
- No period at the end of the heading

## BODY FORMAT

Always include a body. Write a single paragraph describing the technical changes across files. No bullet points. No fluff. Straight technical summary.

## BODY RULES

- Explain WHAT changed and WHY, not HOW
- Reference affected modules, functions, or components
- Include relevant technical details: API changes, database migrations, configuration updates
- Keep under 100 characters per line
- Skip obvious details; focus on meaningful changes

## EXAMPLES

feat[AUTH]: add OAuth2 login flow

Implement Google OAuth2 provider with JWT token generation and session management. Update auth middleware to validate tokens and handle refresh flows.

fix[API]: resolve null pointer in user handler

Add null check before accessing user preferences in profile endpoint. Prevents crash when user record exists but preferences not initialized.

refactor[DB]: consolidate connection pooling logic

Merge duplicate connection pool configurations into shared module. Reduce memory overhead and simplify database connection management.

## OUTPUT

Output ONLY the commit message. No markdown. No code blocks. No explanations."#;

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
