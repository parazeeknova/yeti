use crate::error::{Result, YetiError};
use crate::prompt::{FileInfo, FileStatus};
use git2::{DiffOptions, Repository, Status, StatusOptions};

pub struct GitRepo {
    repo: Repository,
}

#[derive(Debug, Clone)]
pub struct StagedSummary {
    pub branch: String,
    pub files: Vec<FileInfo>,
}

impl GitRepo {
    pub fn discover() -> Result<Self> {
        let repo = Repository::discover(".").map_err(|_| YetiError::NotAGitRepo)?;
        Ok(Self { repo })
    }

    pub fn branch(&self) -> String {
        self.repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()))
            .unwrap_or_else(|| "HEAD".to_string())
    }

    pub fn get_staged_summary(&self) -> Result<StagedSummary> {
        let branch = self.branch();
        let files = self.get_staged_files()?;

        if files.is_empty() {
            return Err(YetiError::NoChangesToCommit);
        }

        Ok(StagedSummary { branch, files })
    }

    fn get_staged_files(&self) -> Result<Vec<FileInfo>> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false)
            .include_unmodified(false);

        let statuses = self.repo.statuses(Some(&mut opts))?;

        let mut files = Vec::new();

        for entry in statuses.iter() {
            let path = match entry.path() {
                Some(p) => p.to_string(),
                None => continue,
            };

            let status = entry.status();

            let file_status = if status.contains(Status::INDEX_NEW)
                || status.contains(Status::WT_NEW)
            {
                FileStatus::Added
            } else if status.contains(Status::INDEX_DELETED) || status.contains(Status::WT_DELETED)
            {
                FileStatus::Deleted
            } else if status.contains(Status::INDEX_RENAMED) || status.contains(Status::WT_RENAMED)
            {
                FileStatus::Renamed
            } else {
                FileStatus::Modified
            };

            let (additions, deletions, diff) = self.get_file_diff(&path, file_status)?;

            files.push(FileInfo {
                path,
                additions,
                deletions,
                diff,
                status: file_status,
            });
        }

        Ok(files)
    }

    fn get_file_diff(&self, path: &str, status: FileStatus) -> Result<(usize, usize, String)> {
        let diff = match status {
            FileStatus::Added => {
                let obj = self.repo.revparse_single("HEAD").ok();
                let old_tree = obj.and_then(|o| o.peel_to_tree().ok());

                let mut opts = DiffOptions::new();
                opts.pathspec(path);
                opts.include_untracked(true);
                opts.recurse_untracked_dirs(true);

                self.repo
                    .diff_tree_to_workdir(old_tree.as_ref(), Some(&mut opts))?
            }
            _ => {
                let mut opts = DiffOptions::new();
                opts.pathspec(path);

                self.repo.diff_index_to_workdir(None, Some(&mut opts))?
            }
        };

        let mut additions = 0;
        let mut deletions = 0;
        let mut diff_text = String::new();

        diff.print(git2::DiffFormat::Patch, |_delta, _, line| {
            let prefix = match line.origin() {
                '+' => {
                    additions += 1;
                    '+'
                }
                '-' => {
                    deletions += 1;
                    '-'
                }
                _ => ' ',
            };

            if diff_text.len() < 2000
                && let Ok(text) = std::str::from_utf8(line.content())
            {
                diff_text.push_str(&format!("{}{}", prefix, text));
            }

            true
        })?;

        Ok((additions, deletions, diff_text))
    }

    pub fn stage_all(&self) -> Result<()> {
        let mut index = self.repo.index()?;

        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;

        index.write()?;
        Ok(())
    }
}

pub fn commit_with_git_cli(title: &str, body: Option<&str>) -> Result<()> {
    let mut cmd = std::process::Command::new("git");
    cmd.arg("commit")
        .arg("-m")
        .arg(title)
        .arg("--no-verify");

    if let Some(b) = body
        && !b.is_empty()
    {
        cmd.arg("-m").arg(b);
    }

    let output = cmd
        .output()
        .map_err(|e| YetiError::CommitFailed(format!("Failed to run git commit: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let msg = if !stderr.is_empty() {
            stderr.to_string()
        } else if !stdout.is_empty() {
            stdout.to_string()
        } else {
            "Git commit failed".to_string()
        };
        return Err(YetiError::CommitFailed(msg));
    }

    Ok(())
}
