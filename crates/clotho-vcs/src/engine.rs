//! The jj-backed repository engine.
//!
//! Server-side design notes:
//! - Repos are **workspace-less**: commits are built directly as trees via the
//!   store, so there is no working copy, no staging area, and no single-writer
//!   working-copy lock to fight over. Each agent's edits arrive as explicit
//!   file contents and become real commits (real git objects, via the internal
//!   git backend at `<repo>/store/git`).
//! - The jj **operation log** is the recovery primitive: `checkpoint` records
//!   a named op-log entry, `restore_to` moves the view back to any operation
//!   (itself recorded as a new operation — history is never erased).
//! - Nothing here shells out to the `jj` or `git` binaries.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt as _;
use jj_lib::backend::{CommitId, CopyId, Signature, Timestamp, TreeValue};
use jj_lib::commit::Commit;
use jj_lib::config::{ConfigLayer, ConfigSource, StackedConfig};
use jj_lib::git_backend::GitBackend;
use jj_lib::matchers::EverythingMatcher;
use jj_lib::merge::Merge;
use jj_lib::merge::MergedTreeValue;
use jj_lib::merged_tree_builder::MergedTreeBuilder;
use jj_lib::object_id::ObjectId as _;
use jj_lib::op_store::{OperationId, RefTarget};
use jj_lib::op_walk;
use jj_lib::operation::Operation;
use jj_lib::ref_name::RefName;
use jj_lib::repo::{ReadonlyRepo, Repo as _, RepoLoader, StoreFactories};
use jj_lib::repo_path::{RepoPath, RepoPathBuf};
use jj_lib::rewrite::rebase_commit;
use jj_lib::settings::UserSettings;
use jj_lib::signing::Signer;
use jj_lib::store::Store;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("invalid repo name {0:?}: must match [a-z0-9][a-z0-9-_]* and be at most 100 chars")]
    InvalidRepoName(String),
    #[error("repo {0:?} already exists")]
    RepoExists(String),
    #[error("repo {0:?} not found")]
    RepoNotFound(String),
    #[error("invalid id {0:?}")]
    InvalidId(String),
    #[error("invalid path {0:?}: {1}")]
    InvalidPath(String, String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl EngineError {
    fn other(err: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Other(anyhow::Error::new(err))
    }
}

pub struct FileChange {
    pub path: String,
    pub content: Vec<u8>,
    pub executable: bool,
}

pub struct CommitParams {
    pub parent_commit_ids: Vec<String>,
    pub files: Vec<FileChange>,
    pub deleted_paths: Vec<String>,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
}

pub struct CommitOutcome {
    pub commit_id: String,
    pub change_id: String,
    pub operation_id: String,
}

pub struct OpLogEntry {
    pub operation_id: String,
    pub description: String,
    pub start_time_millis: i64,
    pub end_time_millis: i64,
    pub parent_operation_ids: Vec<String>,
}

pub struct CommitSummary {
    pub commit_id: String,
    pub change_id: String,
    pub description: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp_millis: i64,
    pub parent_commit_ids: Vec<String>,
}

pub struct Heads {
    pub heads: Vec<CommitSummary>,
    /// Commit the `main` bookmark points at; `None` while main is unborn.
    pub main_commit_id: Option<String>,
}

pub struct RepoArchive {
    /// Uncompressed tar of the bare git repo directory (top-level `repo.git`).
    pub tar: Vec<u8>,
    /// `main` bookmark commit id at export time; empty when main is unborn.
    pub main_commit_id: String,
}

pub struct FileEntry {
    pub path: String,
    pub size_bytes: u64,
    pub executable: bool,
    pub conflicted: bool,
}

pub struct FileList {
    pub commit_id: String,
    pub files: Vec<FileEntry>,
}

pub struct FileContent {
    pub commit_id: String,
    pub path: String,
    pub content: Vec<u8>,
    pub executable: bool,
    pub conflicted: bool,
}

pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

pub struct ChangedFile {
    pub path: String,
    pub kind: ChangeKind,
    pub old_content: Vec<u8>,
    pub new_content: Vec<u8>,
    /// The new side is an unresolved jj conflict; `new_content` holds its
    /// materialization (conflict-marker text).
    pub conflicted: bool,
}

pub struct CommitsDiff {
    pub from_commit_id: String,
    pub to_commit_id: String,
    pub files: Vec<ChangedFile>,
}

pub struct IntegrationOutcome {
    /// The commit now at `main`: the input commit when fast-forwarded, or
    /// its rebased successor (same change id, new commit id).
    pub commit_id: String,
    pub change_id: String,
    pub operation_id: String,
    pub fast_forwarded: bool,
    /// A rebase through a conflict does not stop (jj-style): the commit
    /// lands marked conflicted, main advances, resolution happens later.
    pub conflicted: bool,
    pub conflicted_paths: Vec<String>,
}

/// The bookmark the engine keeps pointing at the latest commit, exported to
/// the backing git repo as `refs/heads/main` so plain-git consumers (Forgejo)
/// see an ordinary branch.
const MAIN_BOOKMARK: &str = "main";

/// Manages all repositories under a single root directory.
///
/// jj-lib's futures are not `Send`, so engine methods cannot run directly on
/// a multi-threaded executor — call them via [`VcsEngine::run`], which
/// executes on a blocking thread with a local executor.
#[derive(Clone)]
pub struct VcsEngine {
    root: PathBuf,
    /// When set, each repo's backing bare git repository is created here as
    /// `<git_root>/<name>.git` (jj's "external" git backend) instead of
    /// inside the jj repo at `<repo>/store/git`. This is how the collaboration
    /// shell (Forgejo) sees Clotho repos: its repository root is this same
    /// directory, shared as a volume (docs/adr/0003).
    git_root: Option<PathBuf>,
    settings: UserSettings,
}

impl VcsEngine {
    /// Run an engine operation to completion on a blocking thread with a
    /// local executor. jj-lib futures are `!Send`; this keeps the gRPC layer's
    /// `Send` requirements satisfied without spreading that constraint.
    pub async fn run<T, F, Fut>(&self, f: F) -> Result<T, EngineError>
    where
        F: FnOnce(VcsEngine) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<T, EngineError>>,
        T: Send + 'static,
    {
        let engine = self.clone();
        tokio::task::spawn_blocking(move || futures::executor::block_on(f(engine)))
            .await
            .map_err(|e| EngineError::Other(anyhow::anyhow!("engine task join error: {e}")))?
    }

    pub fn new(root: impl Into<PathBuf>) -> Result<Self, EngineError> {
        Self::with_git_root(root, None::<PathBuf>)
    }

    /// Like [`VcsEngine::new`], but backing git repos are created under
    /// `git_root` as `<name>.git` (see the `git_root` field docs).
    pub fn with_git_root(
        root: impl Into<PathBuf>,
        git_root: Option<impl Into<PathBuf>>,
    ) -> Result<Self, EngineError> {
        let root = root.into();
        let git_root = git_root.map(Into::into);
        std::fs::create_dir_all(&root).map_err(EngineError::other)?;
        if let Some(git_root) = &git_root {
            std::fs::create_dir_all(git_root).map_err(EngineError::other)?;
        }

        let mut config = StackedConfig::with_defaults();
        // The service-level identity; per-commit authors are set explicitly on
        // each CommitBuilder from the request.
        let layer = ConfigLayer::parse(
            ConfigSource::EnvBase,
            r#"
            user.name = "clotho-vcs"
            user.email = "vcs@clotho.internal"
            operation.hostname = "clotho-vcs"
            operation.username = "clotho"
            "#,
        )
        .map_err(EngineError::other)?;
        config.add_layer(layer);
        let settings = UserSettings::from_config(config).map_err(EngineError::other)?;

        Ok(Self {
            root,
            git_root,
            settings,
        })
    }

    fn validate_name(name: &str) -> Result<(), EngineError> {
        let valid_start = name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit());
        let valid_rest = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if name.is_empty() || name.len() > 100 || !valid_start || !valid_rest {
            return Err(EngineError::InvalidRepoName(name.to_string()));
        }
        Ok(())
    }

    fn repo_path(&self, name: &str) -> Result<PathBuf, EngineError> {
        Self::validate_name(name)?;
        Ok(self.root.join(name))
    }

    /// Create a new repository backed by a bare git repo — internal
    /// (`<repo>/store/git`) by default, or external (`<git_root>/<name>.git`)
    /// when the engine has a git root. Returns the id of the root operation.
    pub async fn init_repo(&self, name: &str) -> Result<String, EngineError> {
        let path = self.repo_path(name)?;
        if path.exists() {
            return Err(EngineError::RepoExists(name.to_string()));
        }
        let external_git_dir = match &self.git_root {
            Some(git_root) => {
                let git_dir = git_root.join(format!("{name}.git"));
                if git_dir.exists() {
                    return Err(EngineError::RepoExists(name.to_string()));
                }
                gix::init_bare(&git_dir).map_err(EngineError::other)?;
                Some(git_dir)
            }
            None => None,
        };
        std::fs::create_dir_all(&path).map_err(EngineError::other)?;

        let signer = Signer::from_settings(&self.settings).map_err(EngineError::other)?;
        let repo = ReadonlyRepo::init(
            &self.settings,
            &path,
            &|settings, store_path| match &external_git_dir {
                Some(git_dir) => Ok(Box::new(GitBackend::init_external(
                    settings, store_path, git_dir,
                )?)),
                None => Ok(Box::new(GitBackend::init_internal(settings, store_path)?)),
            },
            signer,
            ReadonlyRepo::default_op_store_initializer(),
            ReadonlyRepo::default_op_heads_store_initializer(),
            ReadonlyRepo::default_index_store_initializer(),
            ReadonlyRepo::default_submodule_store_initializer(),
        )
        .await
        .map_err(EngineError::other)?;

        // Plain-git consumers (Forgejo) take the default branch from HEAD.
        mirror_main_ref(&self.git_backend_path(name)?, None)?;

        Ok(repo.operation().id().hex())
    }

    /// Absolute path to the real bare git repository backing `name` — every
    /// commit the engine writes is an ordinary git object in here.
    pub fn git_backend_path(&self, name: &str) -> Result<PathBuf, EngineError> {
        match &self.git_root {
            Some(git_root) => Ok(git_root.join(format!("{name}.git"))),
            None => Ok(self.repo_path(name)?.join("store").join("git")),
        }
    }

    async fn load_repo(&self, name: &str) -> Result<Arc<ReadonlyRepo>, EngineError> {
        let path = self.repo_path(name)?;
        if !path.exists() {
            return Err(EngineError::RepoNotFound(name.to_string()));
        }
        let loader =
            RepoLoader::init_from_file_system(&self.settings, &path, &StoreFactories::default())
                .map_err(EngineError::other)?;
        let repo = loader.load_at_head().await.map_err(EngineError::other)?;

        // Absorb git-side ref changes made behind jj's back (a Forgejo UI
        // merge or push moving refs/heads/main) before any engine operation
        // reads or moves main. When nothing moved this is a cheap diff and
        // no operation is recorded; when something did, the import lands in
        // the op log like any other operation (closes the ADR-0003
        // "Forgejo writes bypass the op log" gap).
        let mut tx = repo.start_transaction();
        let import_options = jj_lib::git::GitImportOptions {
            // Never garbage-collect on import: a moved ref must not abandon
            // commits agents may still be building on.
            abandon_unreachable_commits: false,
            record_synthetic_predecessors: false,
            remote_auto_track_bookmarks: std::collections::HashMap::new(),
        };
        jj_lib::git::import_refs(tx.repo_mut(), &import_options)
            .await
            .map_err(EngineError::other)?;
        if !tx.repo_mut().has_changes() {
            return Ok(repo);
        }
        tx.repo_mut()
            .rebase_descendants()
            .await
            .map_err(EngineError::other)?;
        tx.commit("import git refs")
            .await
            .map_err(EngineError::other)
    }

    /// Write a commit built directly from file contents (no working copy).
    pub async fn commit(
        &self,
        name: &str,
        params: CommitParams,
    ) -> Result<CommitOutcome, EngineError> {
        let repo = self.load_repo(name).await?;
        let store = repo.store();

        // Resolve parents: explicit ids, or the repo's current head(s).
        let parent_ids: Vec<CommitId> = if params.parent_commit_ids.is_empty() {
            let mut heads: Vec<CommitId> = repo.view().heads().iter().cloned().collect();
            heads.sort();
            if heads.is_empty() {
                vec![store.root_commit_id().clone()]
            } else {
                heads
            }
        } else {
            params
                .parent_commit_ids
                .iter()
                .map(|hex| {
                    CommitId::try_from_hex(hex).ok_or_else(|| EngineError::InvalidId(hex.clone()))
                })
                .collect::<Result<_, _>>()?
        };

        // Base tree: first parent's tree (empty tree for a root commit).
        let first_parent = store
            .get_commit_async(&parent_ids[0])
            .await
            .map_err(EngineError::other)?;
        let base_tree = first_parent.tree();

        let mut tree_builder = MergedTreeBuilder::new(base_tree);
        for file in &params.files {
            let repo_path = RepoPathBuf::from_internal_string(file.path.clone())
                .map_err(|e| EngineError::InvalidPath(file.path.clone(), e.to_string()))?;
            let file_id = store
                .write_file(&repo_path, &mut file.content.as_slice())
                .await
                .map_err(EngineError::other)?;
            tree_builder.set_or_remove(
                repo_path,
                Merge::normal(TreeValue::File {
                    id: file_id,
                    executable: file.executable,
                    copy_id: CopyId::placeholder(),
                }),
            );
        }
        for path in &params.deleted_paths {
            let repo_path = RepoPathBuf::from_internal_string(path.clone())
                .map_err(|e| EngineError::InvalidPath(path.clone(), e.to_string()))?;
            tree_builder.set_or_remove(repo_path, Merge::absent());
        }
        let tree = tree_builder
            .write_tree()
            .await
            .map_err(EngineError::other)?;

        let author = Signature {
            name: params.author_name,
            email: params.author_email,
            timestamp: Timestamp::now(),
        };

        let mut tx = repo.start_transaction();
        let commit = tx
            .repo_mut()
            .new_commit(parent_ids, tree)
            .set_description(params.message.clone())
            .set_author(author.clone())
            .set_committer(author)
            .write()
            .await
            .map_err(EngineError::other)?;
        // Advance the `main` bookmark only when the new commit extends the
        // current main history (fast-forward), and mirror it into the backing
        // git repo as `refs/heads/main` for plain-git consumers (Forgejo).
        // Side commits (an agent branching off an older parent) leave main
        // alone — landing those is the merge-queue's job (`integrate_commit`).
        let main_target = tx
            .repo_mut()
            .view()
            .get_local_bookmark(RefName::new(MAIN_BOOKMARK))
            .as_normal()
            .cloned();
        let advance_main = match &main_target {
            None => true,
            Some(target) => tx
                .repo_mut()
                .index()
                .is_ancestor(target, commit.id())
                .map_err(EngineError::other)?,
        };
        if advance_main {
            tx.repo_mut().set_local_bookmark_target(
                RefName::new(MAIN_BOOKMARK),
                RefTarget::normal(commit.id().clone()),
            );
            record_main_git_ref(tx.repo_mut(), Some(commit.id()));
        }
        let repo = tx
            .commit(format!("commit: {}", first_line(&params.message)))
            .await
            .map_err(EngineError::other)?;
        if advance_main {
            mirror_main_ref(&self.git_backend_path(name)?, Some(commit.id()))?;
        }

        Ok(CommitOutcome {
            commit_id: commit.id().hex(),
            change_id: commit.change_id().hex(),
            operation_id: repo.operation().id().hex(),
        })
    }

    /// Record a named checkpoint operation and return its id.
    pub async fn checkpoint(&self, name: &str, label: &str) -> Result<String, EngineError> {
        let repo = self.load_repo(name).await?;
        let tx = repo.start_transaction();
        let repo = tx
            .commit(format!("checkpoint: {label}"))
            .await
            .map_err(EngineError::other)?;
        Ok(repo.operation().id().hex())
    }

    /// Restore the repo view to the state as of `operation_id`. Recorded as a
    /// new operation; the op log keeps everything.
    pub async fn restore_to(&self, name: &str, operation_id: &str) -> Result<String, EngineError> {
        let repo = self.load_repo(name).await?;
        let target = self.resolve_operation(&repo, operation_id).await?;
        let target_view = target.view().await.map_err(EngineError::other)?;

        let mut tx = repo.start_transaction();
        tx.repo_mut().set_view(target_view.store_view().clone());
        // The restored view carries the `main` bookmark position from that
        // point in time — mirror it back into the git repo too. Restoring
        // the view also restored a stale record of what git holds; re-record
        // it so ref imports/exports keep diffing against reality.
        let main_target = tx
            .repo_mut()
            .view()
            .get_local_bookmark(RefName::new(MAIN_BOOKMARK))
            .as_normal()
            .cloned();
        record_main_git_ref(tx.repo_mut(), main_target.as_ref());
        let repo = tx
            .commit(format!("restore to operation {}", short(operation_id)))
            .await
            .map_err(EngineError::other)?;
        mirror_main_ref(&self.git_backend_path(name)?, main_target.as_ref())?;
        Ok(repo.operation().id().hex())
    }

    /// Operation log, newest first. `limit == 0` means unbounded.
    pub async fn query_op_log(
        &self,
        name: &str,
        limit: u32,
    ) -> Result<Vec<OpLogEntry>, EngineError> {
        let repo = self.load_repo(name).await?;
        let head_op = repo.operation().clone();

        let mut entries = Vec::new();
        let mut stream = std::pin::pin!(op_walk::walk_ancestors(std::slice::from_ref(&head_op)));
        while let Some(op) = stream.next().await {
            let op = op.map_err(EngineError::other)?;
            let metadata = &op.store_operation().metadata;
            entries.push(OpLogEntry {
                operation_id: op.id().hex(),
                description: metadata.description.clone(),
                start_time_millis: metadata.time.start.timestamp.0,
                end_time_millis: metadata.time.end.timestamp.0,
                parent_operation_ids: op
                    .store_operation()
                    .parents
                    .iter()
                    .map(|id| id.hex())
                    .collect(),
            });
            if limit != 0 && entries.len() as u32 >= limit {
                break;
            }
        }
        Ok(entries)
    }

    /// Current head commits plus the `main` bookmark target — the "where am
    /// I" half of an agent's situational awareness (`orient_repo`).
    pub async fn get_heads(&self, name: &str) -> Result<Heads, EngineError> {
        let repo = self.load_repo(name).await?;
        let store = repo.store();
        let mut head_ids: Vec<CommitId> = repo.view().heads().iter().cloned().collect();
        head_ids.sort();
        let mut heads = Vec::with_capacity(head_ids.len());
        for id in &head_ids {
            let commit = store
                .get_commit_async(id)
                .await
                .map_err(EngineError::other)?;
            heads.push(summarize(&commit));
        }
        let main_commit_id = repo
            .view()
            .get_local_bookmark(RefName::new(MAIN_BOOKMARK))
            .as_normal()
            .map(|id| id.hex());
        Ok(Heads {
            heads,
            main_commit_id,
        })
    }

    /// Export the backing bare git repository as an uncompressed tar of its
    /// directory — the real git object database, delivered so external CI
    /// compute (Stage 7) can `git clone` it inside a sandbox that has no route
    /// back to the stack (docs/adr/0008). Loads the repo first so any external
    /// ref moves are imported and `refs/heads/main` is current; the archive
    /// itself is a plain filesystem tar (never a jj/git shell-out).
    pub async fn export_repo_archive(&self, name: &str) -> Result<RepoArchive, EngineError> {
        let repo = self.load_repo(name).await?;
        let main_commit_id = repo
            .view()
            .get_local_bookmark(RefName::new(MAIN_BOOKMARK))
            .as_normal()
            .map(|id| id.hex())
            .unwrap_or_default();
        let git_dir = self.git_backend_path(name)?;

        let mut builder = tar::Builder::new(Vec::new());
        // Archive the bare repo under a stable top-level name so the sandbox
        // clones a predictable path.
        builder
            .append_dir_all("repo.git", &git_dir)
            .map_err(EngineError::other)?;
        let tar = builder.into_inner().map_err(EngineError::other)?;

        Ok(RepoArchive {
            tar,
            main_commit_id,
        })
    }

    /// List the files in `commit_id`'s tree (default: the `main` bookmark
    /// target). Sizes are real byte counts read from the store.
    pub async fn list_files(
        &self,
        name: &str,
        commit_id: Option<&str>,
    ) -> Result<FileList, EngineError> {
        let repo = self.load_repo(name).await?;
        let store = repo.store();
        let commit_id = match commit_id {
            Some(hex) => CommitId::try_from_hex(hex)
                .ok_or_else(|| EngineError::InvalidId(hex.to_string()))?,
            None => repo
                .view()
                .get_local_bookmark(RefName::new(MAIN_BOOKMARK))
                .as_normal()
                .cloned()
                .unwrap_or_else(|| store.root_commit_id().clone()),
        };
        let commit = store
            .get_commit_async(&commit_id)
            .await
            .map_err(EngineError::other)?;
        let mut files = Vec::new();
        for (path, value) in commit.tree().entries() {
            let value = value.map_err(EngineError::other)?;
            // Conflicted entries are included, flagged, with the size of
            // their materialized conflict text — the browser must never hide
            // first-class conflicts (Stage 5/6).
            if let Some(read) = self.read_tree_value(store, &path, value).await? {
                files.push(FileEntry {
                    path: path.as_internal_file_string().to_string(),
                    size_bytes: read.content.len() as u64,
                    executable: read.executable,
                    conflicted: read.conflicted,
                });
            }
        }
        Ok(FileList {
            commit_id: commit_id.hex(),
            files,
        })
    }

    /// Read one file from `commit_id`'s tree (default: the `main` bookmark
    /// target). Unresolved conflicts come back materialized and flagged.
    pub async fn get_file(
        &self,
        name: &str,
        commit_id: Option<&str>,
        path: &str,
    ) -> Result<FileContent, EngineError> {
        let repo = self.load_repo(name).await?;
        let store = repo.store();
        let commit_id = match commit_id {
            Some(hex) => CommitId::try_from_hex(hex)
                .ok_or_else(|| EngineError::InvalidId(hex.to_string()))?,
            None => repo
                .view()
                .get_local_bookmark(RefName::new(MAIN_BOOKMARK))
                .as_normal()
                .cloned()
                .unwrap_or_else(|| store.root_commit_id().clone()),
        };
        let commit = store
            .get_commit_async(&commit_id)
            .await
            .map_err(EngineError::other)?;
        let repo_path = RepoPathBuf::from_internal_string(path.to_string())
            .map_err(|e| EngineError::InvalidPath(path.to_string(), e.to_string()))?;
        let value = commit
            .tree()
            .path_value(&repo_path)
            .await
            .map_err(EngineError::other)?;
        let read = self
            .read_tree_value(store, &repo_path, value)
            .await?
            .ok_or_else(|| {
                EngineError::InvalidPath(path.to_string(), "no such file at this commit".into())
            })?;
        Ok(FileContent {
            commit_id: commit_id.hex(),
            path: path.to_string(),
            content: read.content,
            executable: read.executable,
            conflicted: read.conflicted,
        })
    }

    /// Commit history walking ancestors of `from` (default: the `main`
    /// bookmark target), newest first. `limit == 0` means unbounded.
    pub async fn log_commits(
        &self,
        name: &str,
        from_commit_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<CommitSummary>, EngineError> {
        let repo = self.load_repo(name).await?;
        let store = repo.store();
        let start = match from_commit_id {
            Some(hex) => Some(
                CommitId::try_from_hex(hex)
                    .ok_or_else(|| EngineError::InvalidId(hex.to_string()))?,
            ),
            None => repo
                .view()
                .get_local_bookmark(RefName::new(MAIN_BOOKMARK))
                .as_normal()
                .cloned(),
        };
        let Some(start) = start else {
            return Ok(Vec::new()); // main is unborn: no history yet
        };

        // Walk ancestors newest-committer-time-first (a plain priority-queue
        // walk over parent edges — good enough for a prototype log view; the
        // root commit itself is elided as jj's synthetic empty root).
        let root_id = store.root_commit_id().clone();
        let mut seen = std::collections::HashSet::from([start.clone()]);
        let mut queue = std::collections::BinaryHeap::new();
        let mut commits = Vec::new();
        let start_commit = store
            .get_commit_async(&start)
            .await
            .map_err(EngineError::other)?;
        queue.push((start_commit.committer().timestamp.timestamp.0, start));
        while let Some((_, id)) = queue.pop() {
            if id == root_id {
                continue;
            }
            let commit = store
                .get_commit_async(&id)
                .await
                .map_err(EngineError::other)?;
            commits.push(summarize(&commit));
            if limit != 0 && commits.len() as u32 >= limit {
                break;
            }
            for parent_id in commit.parent_ids() {
                if seen.insert(parent_id.clone()) {
                    let parent = store
                        .get_commit_async(parent_id)
                        .await
                        .map_err(EngineError::other)?;
                    queue.push((parent.committer().timestamp.timestamp.0, parent_id.clone()));
                }
            }
        }
        Ok(commits)
    }

    /// Changed files between two commits, with full before/after contents.
    /// `from` defaults to the first parent of `to`.
    pub async fn diff_commits(
        &self,
        name: &str,
        from_commit_id: Option<&str>,
        to_commit_id: &str,
    ) -> Result<CommitsDiff, EngineError> {
        let repo = self.load_repo(name).await?;
        let store = repo.store();
        let to_id = CommitId::try_from_hex(to_commit_id)
            .ok_or_else(|| EngineError::InvalidId(to_commit_id.to_string()))?;
        let to_commit = store
            .get_commit_async(&to_id)
            .await
            .map_err(EngineError::other)?;
        let from_id = match from_commit_id {
            Some(hex) => CommitId::try_from_hex(hex)
                .ok_or_else(|| EngineError::InvalidId(hex.to_string()))?,
            None => to_commit
                .parent_ids()
                .first()
                .cloned()
                .unwrap_or_else(|| store.root_commit_id().clone()),
        };
        let from_commit = store
            .get_commit_async(&from_id)
            .await
            .map_err(EngineError::other)?;

        let from_tree = from_commit.tree();
        let to_tree = to_commit.tree();
        let mut files = Vec::new();
        let mut stream = from_tree.diff_stream(&to_tree, &EverythingMatcher);
        while let Some(entry) = stream.next().await {
            let values = entry.values.map_err(EngineError::other)?;
            let old = self
                .read_tree_value(store, &entry.path, values.before)
                .await?;
            let new = self
                .read_tree_value(store, &entry.path, values.after)
                .await?;
            let old_content = old.map(|r| r.content).unwrap_or_default();
            let (new_content, conflicted) =
                new.map(|r| (r.content, r.conflicted)).unwrap_or_default();
            if old_content == new_content {
                continue;
            }
            let kind = if old_content.is_empty() {
                ChangeKind::Added
            } else if new_content.is_empty() {
                ChangeKind::Deleted
            } else {
                ChangeKind::Modified
            };
            files.push(ChangedFile {
                path: entry.path.as_internal_file_string().to_string(),
                kind,
                old_content,
                new_content,
                conflicted,
            });
        }
        Ok(CommitsDiff {
            from_commit_id: from_id.hex(),
            to_commit_id: to_id.hex(),
            files,
        })
    }

    /// Land `commit_id` on `main`: fast-forward when it already descends
    /// from the current main target, otherwise rebase it on top. Conflicts
    /// are first-class — the rebased commit lands marked conflicted and main
    /// still advances (the vision spec's non-blocking conflict model). The
    /// merge-queue serializes calls to this per repo; the engine itself
    /// stays policy-free.
    pub async fn integrate_commit(
        &self,
        name: &str,
        commit_id: &str,
    ) -> Result<IntegrationOutcome, EngineError> {
        let repo = self.load_repo(name).await?;
        let store = repo.store();
        let commit_id = CommitId::try_from_hex(commit_id)
            .ok_or_else(|| EngineError::InvalidId(commit_id.to_string()))?;
        let commit = store
            .get_commit_async(&commit_id)
            .await
            .map_err(EngineError::other)?;
        let main_target = repo
            .view()
            .get_local_bookmark(RefName::new(MAIN_BOOKMARK))
            .as_normal()
            .cloned();

        let mut tx = repo.start_transaction();
        let fast_forward = match &main_target {
            None => true,
            Some(target) => repo
                .index()
                .is_ancestor(target, &commit_id)
                .map_err(EngineError::other)?,
        };
        let landed = if fast_forward {
            commit
        } else {
            let target = main_target.expect("rebase implies main exists");
            let rebased = rebase_commit(tx.repo_mut(), commit, vec![target])
                .await
                .map_err(EngineError::other)?;
            // Keep any descendants of the submitted commit consistent with
            // its rewrite (usually a no-op — submissions are heads).
            tx.repo_mut()
                .rebase_descendants()
                .await
                .map_err(EngineError::other)?;
            rebased
        };

        let conflicted = landed.has_conflict();
        let mut conflicted_paths = Vec::new();
        if conflicted {
            for (path, _) in landed.tree().conflicts() {
                conflicted_paths.push(path.as_internal_file_string().to_string());
            }
        }

        tx.repo_mut().set_local_bookmark_target(
            RefName::new(MAIN_BOOKMARK),
            RefTarget::normal(landed.id().clone()),
        );
        record_main_git_ref(tx.repo_mut(), Some(landed.id()));
        let repo = tx
            .commit(format!(
                "integrate commit {} into main{}",
                short(&landed.id().hex()),
                if conflicted { " (conflicted)" } else { "" }
            ))
            .await
            .map_err(EngineError::other)?;
        mirror_main_ref(&self.git_backend_path(name)?, Some(landed.id()))?;

        Ok(IntegrationOutcome {
            commit_id: landed.id().hex(),
            change_id: landed.change_id().hex(),
            operation_id: repo.operation().id().hex(),
            fast_forwarded: fast_forward,
            conflicted,
            conflicted_paths,
        })
    }

    /// One tree entry in readable form. Unresolved file conflicts come back
    /// as jj's materialized conflict-marker text — the same rendering the
    /// backing git tree holds (ADR-0006) — flagged `conflicted`. `None` for
    /// entries with no file contents to expose (absent, symlinks,
    /// submodules, exotic non-file conflicts).
    async fn read_tree_value(
        &self,
        store: &Arc<Store>,
        path: &RepoPath,
        value: MergedTreeValue,
    ) -> Result<Option<ReadFile>, EngineError> {
        use jj_lib::conflict_labels::ConflictLabels;
        use jj_lib::conflicts::{
            materialize_merge_result_to_bytes, materialize_tree_value, ConflictMarkerStyle,
            ConflictMaterializeOptions, MaterializedTreeValue,
        };
        use jj_lib::tree_merge::MergeOptions;

        match materialize_tree_value(store, path, value, &ConflictLabels::unlabeled())
            .await
            .map_err(EngineError::other)?
        {
            MaterializedTreeValue::File(mut file) => Ok(Some(ReadFile {
                content: file.read_all(path).await.map_err(EngineError::other)?,
                executable: file.executable,
                conflicted: false,
            })),
            MaterializedTreeValue::FileConflict(conflict) => {
                let options = ConflictMaterializeOptions {
                    // jj's default marker style, matching what the working
                    // copy (and therefore human muscle memory) shows.
                    marker_style: ConflictMarkerStyle::Diff,
                    marker_len: None,
                    merge: MergeOptions::from_settings(&self.settings)
                        .map_err(EngineError::other)?,
                };
                let content = materialize_merge_result_to_bytes(
                    &conflict.contents,
                    &conflict.labels,
                    &options,
                );
                Ok(Some(ReadFile {
                    content: content.into(),
                    executable: conflict.executable.unwrap_or(false),
                    conflicted: true,
                }))
            }
            _ => Ok(None),
        }
    }

    async fn resolve_operation(
        &self,
        repo: &Arc<ReadonlyRepo>,
        operation_id: &str,
    ) -> Result<Operation, EngineError> {
        let id = OperationId::try_from_hex(operation_id)
            .ok_or_else(|| EngineError::InvalidId(operation_id.to_string()))?;
        let op_store = repo.op_store();
        let data = op_store
            .read_operation(&id)
            .await
            .map_err(EngineError::other)?;
        Ok(Operation::new(op_store.clone(), id, data))
    }
}

fn summarize(commit: &Commit) -> CommitSummary {
    CommitSummary {
        commit_id: commit.id().hex(),
        change_id: commit.change_id().hex(),
        description: commit.description().to_string(),
        author_name: commit.author().name.clone(),
        author_email: commit.author().email.clone(),
        timestamp_millis: commit.author().timestamp.timestamp.0,
        parent_commit_ids: commit.parent_ids().iter().map(|id| id.hex()).collect(),
    }
}

/// A tree entry read through [`VcsEngine::read_tree_value`].
struct ReadFile {
    content: Vec<u8>,
    executable: bool,
    conflicted: bool,
}

/// Record in the jj view what `refs/heads/main` in the backing git repo is
/// about to hold (see [`mirror_main_ref`], which performs the actual git
/// write after the transaction commits): both the raw git-ref record and
/// the `main@git` remote-tracking ref that `import_refs` diffs against.
/// Keeping this bookkeeping in sync with our own writes means the import in
/// `load_repo` only fires — and only merges — for *external* changes, e.g.
/// Forgejo moving main behind jj's back.
fn record_main_git_ref(mut_repo: &mut jj_lib::repo::MutableRepo, target: Option<&CommitId>) {
    use jj_lib::git::REMOTE_NAME_FOR_LOCAL_GIT_REPO;
    use jj_lib::op_store::{RemoteRef, RemoteRefState};
    use jj_lib::ref_name::GitRefName;

    let target = match target {
        Some(id) => RefTarget::normal(id.clone()),
        None => RefTarget::absent(),
    };
    mut_repo.set_git_ref_target(GitRefName::new("refs/heads/main"), target.clone());
    mut_repo.set_remote_bookmark(
        RefName::new(MAIN_BOOKMARK).to_remote_symbol(REMOTE_NAME_FOR_LOCAL_GIT_REPO),
        RemoteRef {
            target,
            state: RemoteRefState::Tracked,
        },
    );
}

/// Mirror the engine's `main` bookmark into the backing bare git repo:
/// `refs/heads/main` tracks `target` (absent when `None`), and HEAD stays a
/// symref to it so plain-git consumers treat `main` as the default branch.
/// External writers (Forgejo) are absorbed by the ref import in `load_repo`
/// before every engine operation, so by the time we write here the engine's
/// view already accounts for them; a write racing into that small window
/// would be clobbered — an accepted prototype tradeoff (docs/adr/0003).
fn mirror_main_ref(git_dir: &Path, target: Option<&CommitId>) -> Result<(), EngineError> {
    use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
    use gix::refs::{FullName, Target};

    let repo = gix::open(git_dir).map_err(EngineError::other)?;
    let main_ref: FullName = format!("refs/heads/{MAIN_BOOKMARK}")
        .try_into()
        .map_err(EngineError::other)?;
    let log = |message: &str| LogChange {
        mode: RefLog::AndReference,
        force_create_reflog: false,
        message: message.into(),
    };

    let mut edits = vec![RefEdit {
        change: Change::Update {
            log: log("clotho-vcs: point HEAD at main"),
            expected: PreviousValue::Any,
            new: Target::Symbolic(main_ref.clone()),
        },
        name: "HEAD".try_into().map_err(EngineError::other)?,
        deref: false,
    }];
    match target {
        Some(id) => {
            let oid = gix::ObjectId::try_from(id.as_bytes()).map_err(EngineError::other)?;
            edits.push(RefEdit {
                change: Change::Update {
                    log: log("clotho-vcs: advance main"),
                    expected: PreviousValue::Any,
                    new: Target::Object(oid),
                },
                name: main_ref,
                deref: false,
            });
        }
        // Absent bookmark (fresh repo, or restore to before it existed):
        // remove the branch ref if it exists so git agrees `main` is unborn.
        None => {
            if repo.try_find_reference(&main_ref).ok().flatten().is_some() {
                edits.push(RefEdit {
                    change: Change::Delete {
                        expected: PreviousValue::Any,
                        log: RefLog::AndReference,
                    },
                    name: main_ref,
                    deref: false,
                });
            }
        }
    }
    repo.edit_references(edits).map_err(EngineError::other)?;
    Ok(())
}

fn first_line(message: &str) -> &str {
    message.lines().next().unwrap_or_default()
}

fn short(id: &str) -> &str {
    &id[..id.len().min(12)]
}
