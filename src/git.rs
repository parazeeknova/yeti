use crate::error::{Result, YetiError};
use crate::prompt::{FileInfo, FileStatus};
use git2::{DiffFindOptions, DiffOptions, Repository};
use std::cell::RefCell;
use std::collections::HashMap;

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

        let mut diff = if let Some(tree) = &head_tree {
            self.repo
                .diff_tree_to_index(Some(tree), None, Some(&mut opts))?
        } else {
            self.repo.diff_tree_to_index(None, None, Some(&mut opts))?
        };

        let mut find_opts = DiffFindOptions::new();
        find_opts.renames(true);
        diff.find_similar(Some(&mut find_opts))?;

        let files: RefCell<Vec<FileInfo>> = RefCell::new(Vec::new());
        let file_index: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());

        diff.foreach(
            &mut |delta, _| {
                let path = delta_path(&delta);
                if let Some(path) = path {
                    let status = match delta.status() {
                        git2::Delta::Added => FileStatus::Added,
                        git2::Delta::Deleted => FileStatus::Deleted,
                        git2::Delta::Renamed => FileStatus::Renamed,
                        _ => FileStatus::Modified,
                    };
                    let old_path = match status {
                        FileStatus::Renamed => delta
                            .old_file()
                            .path()
                            .map(|p| p.to_string_lossy().to_string()),
                        _ => None,
                    };

                    let mut files_mut = files.borrow_mut();
                    let index = files_mut.len();
                    file_index.borrow_mut().insert(path.clone(), index);
                    files_mut.push(FileInfo {
                        path,
                        additions: 0,
                        deletions: 0,
                        diff: String::new(),
                        status,
                        old_path,
                    });
                }
                true
            },
            None,
            None,
            Some(&mut |delta, _hunk, line| {
                let Some(path) = delta_path(&delta) else {
                    return true;
                };
                let index = {
                    let file_index_ref = file_index.borrow();
                    file_index_ref.get(&path).copied()
                };
                let Some(index) = index else {
                    return true;
                };

                let mut files_mut = files.borrow_mut();
                match line.origin() {
                    '+' => files_mut[index].additions += 1,
                    '-' => files_mut[index].deletions += 1,
                    _ => {}
                }

                if files_mut[index].diff.len() < 3000
                    && let Ok(text) = std::str::from_utf8(line.content())
                {
                    let prefix = line.origin();
                    files_mut[index]
                        .diff
                        .push_str(&format!("{}{}", prefix, text));
                }
                true
            }),
        )?;

        Ok(files.into_inner())
    }

    pub fn stage_all(&self) -> Result<()> {
        let mut index = self.repo.index()?;
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
        index.write()?;
        Ok(())
    }
}

fn delta_path(delta: &git2::DiffDelta<'_>) -> Option<String> {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().to_string())
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

pub fn unstage_all_with_git_cli() -> Result<()> {
    let output = std::process::Command::new("git")
        .arg("reset")
        .arg("--mixed")
        .arg("--quiet")
        .output()
        .map_err(|e| YetiError::CommitFailed(format!("Failed to run git reset: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let msg = if !stderr.is_empty() {
            stderr.to_string()
        } else if !stdout.is_empty() {
            stdout.to_string()
        } else {
            "Git reset failed".to_string()
        };
        return Err(YetiError::CommitFailed(msg));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{GitRepo, Result};
    use crate::prompt::FileStatus;
    use git2::{Repository, Signature};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn staged_summary_detects_rename_and_tracks_old_path() -> Result<()> {
        let temp_dir = create_temp_repo_dir("rename");
        let repo = init_repo_with_initial_commit(&temp_dir)?;

        let old_path = temp_dir.join("src/file.txt");
        let new_path = temp_dir.join("src/file_renamed.txt");
        fs::rename(&old_path, &new_path)?;
        write_file(&new_path, "one\ntwo\n")?;

        {
            let mut index = repo.index()?;
            index.remove_path(Path::new("src/file.txt"))?;
            index.add_path(Path::new("src/file_renamed.txt"))?;
            index.write()?;
        }

        write_file(&new_path, "one\ntwo\nunstaged-extra\n")?;

        let git_repo = GitRepo { repo };
        let summary = git_repo.get_staged_summary()?;
        let renamed = summary
            .files
            .iter()
            .find(|f| f.path == "src/file_renamed.txt")
            .expect("renamed file not found");

        assert_eq!(renamed.status, FileStatus::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("src/file.txt"));
        assert_eq!(renamed.additions, 0);
        assert_eq!(renamed.deletions, 0);
        assert!(!renamed.diff.contains("unstaged-extra"));

        drop(git_repo);
        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    #[test]
    fn staged_summary_uses_index_not_working_tree_for_patch() -> Result<()> {
        let temp_dir = create_temp_repo_dir("staged-only");
        let repo = init_repo_with_initial_commit(&temp_dir)?;
        let file_path = temp_dir.join("src/file.txt");

        write_file(&file_path, "one\ntwo\nstaged-only\n")?;
        {
            let mut index = repo.index()?;
            index.add_path(Path::new("src/file.txt"))?;
            index.write()?;
        }

        write_file(&file_path, "one\ntwo\nstaged-only\nunstaged-only\n")?;

        let git_repo = GitRepo { repo };
        let summary = git_repo.get_staged_summary()?;
        let changed = summary
            .files
            .iter()
            .find(|f| f.path == "src/file.txt")
            .expect("staged file not found");

        assert_eq!(changed.status, FileStatus::Modified);
        assert!(changed.diff.contains("staged-only"));
        assert!(!changed.diff.contains("unstaged-only"));

        drop(git_repo);
        let _ = fs::remove_dir_all(&temp_dir);
        Ok(())
    }

    fn create_temp_repo_dir(suffix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "yeti-git-tests-{suffix}-{}-{timestamp}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("failed to create temp directory");
        dir
    }

    fn init_repo_with_initial_commit(path: &Path) -> Result<Repository> {
        let repo = Repository::init(path)?;
        let file_path = path.join("src/file.txt");
        write_file(&file_path, "one\ntwo\n")?;

        {
            let mut index = repo.index()?;
            index.add_path(Path::new("src/file.txt"))?;
            index.write()?;
        }

        let tree_id = repo.index()?.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let sig = Signature::now("yeti-tests", "yeti-tests@example.com")?;
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])?;
        drop(tree);

        Ok(repo)
    }

    fn write_file(path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }
}
