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
use jj_lib::config::{ConfigLayer, ConfigSource, StackedConfig};
use jj_lib::git_backend::GitBackend;
use jj_lib::merge::Merge;
use jj_lib::merged_tree_builder::MergedTreeBuilder;
use jj_lib::object_id::ObjectId as _;
use jj_lib::op_store::{OperationId, RefTarget};
use jj_lib::op_walk;
use jj_lib::operation::Operation;
use jj_lib::ref_name::RefName;
use jj_lib::repo::{ReadonlyRepo, Repo as _, RepoLoader, StoreFactories};
use jj_lib::repo_path::RepoPathBuf;
use jj_lib::settings::UserSettings;
use jj_lib::signing::Signer;

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
        loader.load_at_head().await.map_err(EngineError::other)
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
        // Advance the `main` bookmark to the new commit and mirror it into
        // the backing git repo as `refs/heads/main`, so plain-git consumers
        // (Forgejo) see every engine-written commit on an ordinary branch.
        tx.repo_mut().set_local_bookmark_target(
            RefName::new(MAIN_BOOKMARK),
            RefTarget::normal(commit.id().clone()),
        );
        let repo = tx
            .commit(format!("commit: {}", first_line(&params.message)))
            .await
            .map_err(EngineError::other)?;
        mirror_main_ref(&self.git_backend_path(name)?, Some(commit.id()))?;

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
        // point in time — mirror it back into the git repo too.
        let main_target = tx
            .repo_mut()
            .view()
            .get_local_bookmark(RefName::new(MAIN_BOOKMARK))
            .as_normal()
            .cloned();
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

/// Mirror the engine's `main` bookmark into the backing bare git repo:
/// `refs/heads/main` tracks `target` (absent when `None`), and HEAD stays a
/// symref to it so plain-git consumers treat `main` as the default branch.
/// clotho-vcs is the only writer of these refs (docs/adr/0003), so
/// unconditional updates are safe.
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
