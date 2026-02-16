use crate::error::{Result, YetiError};
use crate::prompt::{FileInfo, FileStatus};
use git2::{DiffOptions, Repository};

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
        let head_tree = self
            .repo
            .revparse_single("HEAD")
            .ok()
            .and_then(|o| o.peel_to_tree().ok());

        let mut opts = DiffOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true);

        let diff = if let Some(tree) = &head_tree {
            self.repo
                .diff_tree_to_index(Some(tree), None, Some(&mut opts))?
        } else {
            self.repo.diff_tree_to_index(None, None, Some(&mut opts))?
        };

        let mut files = Vec::new();

        diff.foreach(
            &mut |delta, _| {
                let path = delta
                    .new_file()
                    .path()
                    .map(|p| p.to_string_lossy().to_string());
                if let Some(path) = path {
                    let status = match delta.status() {
                        git2::Delta::Added => FileStatus::Added,
                        git2::Delta::Deleted => FileStatus::Deleted,
                        git2::Delta::Renamed => FileStatus::Renamed,
                        _ => FileStatus::Modified,
                    };

                    files.push(FileInfo {
                        path,
                        additions: 0,
                        deletions: 0,
                        diff: String::new(),
                        status,
                    });
                }
                true
            },
            None,
            None,
            None,
        )?;

        for file in &mut files {
            let (add, del, diff_text) = self.get_file_stats(&file.path, file.status)?;
            file.additions = add;
            file.deletions = del;
            file.diff = diff_text;
        }

        Ok(files)
    }

    fn get_file_stats(&self, path: &str, status: FileStatus) -> Result<(usize, usize, String)> {
        let head_tree = self
            .repo
            .revparse_single("HEAD")
            .ok()
            .and_then(|o| o.peel_to_tree().ok());

        let mut opts = DiffOptions::new();
        opts.pathspec(path);

        let diff = match status {
            FileStatus::Added => {
                let mut opts = DiffOptions::new();
                opts.pathspec(path);
                opts.include_untracked(true);
                opts.recurse_untracked_dirs(true);

                if let Some(tree) = &head_tree {
                    self.repo
                        .diff_tree_to_workdir(Some(tree), Some(&mut opts))?
                } else {
                    self.repo.diff_tree_to_workdir(None, Some(&mut opts))?
                }
            }
            _ => {
                if let Some(tree) = &head_tree {
                    self.repo
                        .diff_tree_to_workdir(Some(tree), Some(&mut opts))?
                } else {
                    self.repo.diff_tree_to_workdir(None, Some(&mut opts))?
                }
            }
        };

        let mut additions = 0;
        let mut deletions = 0;
        let mut diff_text = String::new();

        diff.print(git2::DiffFormat::Patch, |_delta, _, line| {
            match line.origin() {
                '+' => additions += 1,
                '-' => deletions += 1,
                _ => {}
            }

            if diff_text.len() < 2000
                && let Ok(text) = std::str::from_utf8(line.content())
            {
                let prefix = line.origin();
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
    cmd.arg("commit").arg("-m").arg(title).arg("--no-verify");

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
