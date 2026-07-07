//! Stage 1 exit-condition tests (docs/prd.md §5): a test harness creates a
//! repo, commits from two simulated agents, and reads a unified op log back —
//! entirely through the gRPC API, no shelling out to the `jj` binary. Plus
//! ticket 6's proof: a commit lands as a real git object (verified with gix).

use clotho_common::pb::vcs::v1::{
    changed_file::ChangeKind, vcs_client::VcsClient, CheckpointRequest, CommitRequest,
    DiffCommitsRequest, FileChange, GetHeadsRequest, InitRepoRequest, ListFilesRequest,
    QueryOpLogRequest, RestoreToRequest,
};
use clotho_vcs::{VcsEngine, VcsService};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};

async fn start_server(root: &std::path::Path) -> VcsClient<Channel> {
    serve(VcsEngine::new(root).unwrap()).await
}

async fn serve(engine: VcsEngine) -> VcsClient<Channel> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(
        Server::builder()
            .add_service(VcsService::new(engine).into_server())
            .serve_with_incoming(TcpListenerStream::new(listener)),
    );
    VcsClient::connect(format!("http://{addr}")).await.unwrap()
}

fn file(path: &str, content: &str) -> FileChange {
    FileChange {
        path: path.to_string(),
        content: content.as_bytes().to_vec(),
        executable: false,
    }
}

fn commit_req(repo: &str, agent: &str, message: &str, files: Vec<FileChange>) -> CommitRequest {
    CommitRequest {
        repo: repo.to_string(),
        parent_commit_ids: vec![],
        files,
        deleted_paths: vec![],
        message: message.to_string(),
        author_name: agent.to_string(),
        author_email: format!("{agent}@agents.clotho.internal"),
    }
}

#[tokio::test]
async fn commit_lands_as_real_git_object() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = start_server(dir.path()).await;

    client
        .init_repo(InitRepoRequest {
            name: "weave".into(),
        })
        .await
        .unwrap();

    let commit = client
        .commit(commit_req(
            "weave",
            "agent-a",
            "add readme",
            vec![file("README.md", "# weave\n")],
        ))
        .await
        .unwrap()
        .into_inner();

    // The commit must exist as an ordinary git commit in the internal bare
    // git repo — verified with gix, independent of jj.
    let git_repo = gix::open(dir.path().join("weave/store/git")).unwrap();
    let oid = gix::ObjectId::from_hex(commit.commit_id.as_bytes()).unwrap();
    let git_commit = git_repo.find_commit(oid).unwrap();
    assert_eq!(git_commit.message_raw_sloppy().to_string(), "add readme");
    let author = git_commit.author().unwrap();
    assert_eq!(author.name, "agent-a");

    // The file content must round-trip through the git tree.
    let tree = git_commit.tree().unwrap();
    let entry = tree.find_entry("README.md").expect("README.md in tree");
    let blob = git_repo.find_object(entry.oid()).unwrap();
    assert_eq!(blob.data.as_slice(), b"# weave\n");
}

#[tokio::test]
async fn two_agents_commit_into_one_graph_with_unified_op_log() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = start_server(dir.path()).await;

    client
        .init_repo(InitRepoRequest {
            name: "loom".into(),
        })
        .await
        .unwrap();

    // Two simulated agent sessions committing through the same engine.
    let a = client
        .commit(commit_req(
            "loom",
            "agent-a",
            "agent a: spin thread",
            vec![file("a.txt", "thread a\n")],
        ))
        .await
        .unwrap()
        .into_inner();
    let b = client
        .commit(commit_req(
            "loom",
            "agent-b",
            "agent b: weave cloth",
            vec![file("b.txt", "thread b\n")],
        ))
        .await
        .unwrap()
        .into_inner();
    assert_ne!(a.commit_id, b.commit_id);

    // Both commits are real git objects in one object store, and b descends
    // from a (b committed on the current head), i.e. one commit graph.
    let git_repo = gix::open(dir.path().join("loom/store/git")).unwrap();
    let oid_a = gix::ObjectId::from_hex(a.commit_id.as_bytes()).unwrap();
    let oid_b = gix::ObjectId::from_hex(b.commit_id.as_bytes()).unwrap();
    git_repo.find_commit(oid_a).unwrap();
    let commit_b = git_repo.find_commit(oid_b).unwrap();
    let parents: Vec<_> = commit_b.parent_ids().map(|id| id.detach()).collect();
    assert!(parents.contains(&oid_a), "b must descend from a");

    // The unified op log shows the repo creation and both agents' commits.
    let log = client
        .query_op_log(QueryOpLogRequest {
            repo: "loom".into(),
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner();
    let descriptions: Vec<_> = log.entries.iter().map(|e| e.description.as_str()).collect();
    assert!(descriptions.contains(&"commit: agent a: spin thread"));
    assert!(descriptions.contains(&"commit: agent b: weave cloth"));
    // Newest first.
    assert_eq!(descriptions.first(), Some(&"commit: agent b: weave cloth"));
}

/// Stage 3 (docs/prd.md §5, docs/adr/0003): with an external git root, the
/// backing bare git repo lands at `<git_root>/<name>.git`, `refs/heads/main`
/// tracks every engine-written commit, and HEAD stays a symref to main — so a
/// plain-git collaboration shell (Forgejo) sees an ordinary branch, and
/// restores move the branch with the jj view.
#[tokio::test]
async fn external_git_root_mirrors_main_branch() {
    let dir = tempfile::tempdir().unwrap();
    let git_root = dir.path().join("git-repos");
    let engine = VcsEngine::with_git_root(dir.path().join("jj"), Some(&git_root)).unwrap();
    let mut client = serve(engine).await;

    client
        .init_repo(InitRepoRequest {
            name: "spindle".into(),
        })
        .await
        .unwrap();

    // A fresh repo is a bare git repo with HEAD pointing at (unborn) main.
    let git_repo = gix::open(git_root.join("spindle.git")).unwrap();
    let head = git_repo.head_name().unwrap().expect("HEAD is symbolic");
    assert_eq!(head.as_bstr(), "refs/heads/main");

    let first = client
        .commit(commit_req(
            "spindle",
            "agent-a",
            "first",
            vec![file("a.txt", "a\n")],
        ))
        .await
        .unwrap()
        .into_inner();
    let checkpoint = client
        .checkpoint(CheckpointRequest {
            repo: "spindle".into(),
            label: "after first".into(),
        })
        .await
        .unwrap()
        .into_inner();
    let second = client
        .commit(commit_req(
            "spindle",
            "agent-b",
            "second",
            vec![file("b.txt", "b\n")],
        ))
        .await
        .unwrap()
        .into_inner();

    // refs/heads/main tracks the newest commit; HEAD still points at main.
    let git_repo = gix::open(git_root.join("spindle.git")).unwrap();
    let main = git_repo.find_reference("refs/heads/main").unwrap();
    assert_eq!(
        main.id().to_string(),
        second.commit_id,
        "main must track the latest commit"
    );
    let head = git_repo.head_name().unwrap().expect("HEAD is symbolic");
    assert_eq!(head.as_bstr(), "refs/heads/main");

    // Restoring to the checkpoint moves main back with the jj view.
    client
        .restore_to(RestoreToRequest {
            repo: "spindle".into(),
            operation_id: checkpoint.operation_id,
        })
        .await
        .unwrap();
    let git_repo = gix::open(git_root.join("spindle.git")).unwrap();
    let main = git_repo.find_reference("refs/heads/main").unwrap();
    assert_eq!(main.id().to_string(), first.commit_id);
}

#[tokio::test]
async fn checkpoint_and_restore_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = start_server(dir.path()).await;

    client
        .init_repo(InitRepoRequest {
            name: "fates".into(),
        })
        .await
        .unwrap();

    let good = client
        .commit(commit_req(
            "fates",
            "agent-a",
            "known good state",
            vec![file("main.rs", "fn main() {}\n")],
        ))
        .await
        .unwrap()
        .into_inner();

    let checkpoint = client
        .checkpoint(CheckpointRequest {
            repo: "fates".into(),
            label: "before risky refactor".into(),
        })
        .await
        .unwrap()
        .into_inner();

    // The agent "breaks something".
    let broken = client
        .commit(commit_req(
            "fates",
            "agent-a",
            "risky refactor (broken)",
            vec![file("main.rs", "fn main() { compile_error!() }\n")],
        ))
        .await
        .unwrap()
        .into_inner();
    assert_ne!(broken.commit_id, good.commit_id);

    // Restore to the checkpoint; the next commit builds on the good state.
    client
        .restore_to(RestoreToRequest {
            repo: "fates".into(),
            operation_id: checkpoint.operation_id.clone(),
        })
        .await
        .unwrap();

    let after = client
        .commit(commit_req(
            "fates",
            "agent-a",
            "continue from checkpoint",
            vec![file("lib.rs", "pub fn ok() {}\n")],
        ))
        .await
        .unwrap()
        .into_inner();

    // The new commit's parent must be the known-good commit, not the broken one.
    let git_repo = gix::open(dir.path().join("fates/store/git")).unwrap();
    let oid_after = gix::ObjectId::from_hex(after.commit_id.as_bytes()).unwrap();
    let commit_after = git_repo.find_commit(oid_after).unwrap();
    let parents: Vec<_> = commit_after.parent_ids().map(|id| id.detach()).collect();
    let oid_good = gix::ObjectId::from_hex(good.commit_id.as_bytes()).unwrap();
    let oid_broken = gix::ObjectId::from_hex(broken.commit_id.as_bytes()).unwrap();
    assert!(parents.contains(&oid_good));
    assert!(!parents.contains(&oid_broken));

    // Nothing was erased: the restore itself is an op-log entry.
    let log = client
        .query_op_log(QueryOpLogRequest {
            repo: "fates".into(),
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(log
        .entries
        .iter()
        .any(|e| e.description.starts_with("restore to operation")));
    assert!(log
        .entries
        .iter()
        .any(|e| e.description == "checkpoint: before risky refactor"));
}

/// Stage 5 (docs/adr/0003 consequence, resolved): git-side ref writes made
/// behind jj's back — a Forgejo UI merge or push moving `refs/heads/main` —
/// are imported into the jj view and op log before every engine operation,
/// instead of staying invisible.
#[tokio::test]
async fn external_git_ref_moves_flow_back_into_the_op_log() {
    let dir = tempfile::tempdir().unwrap();
    let git_root = dir.path().join("git-repos");
    let engine = VcsEngine::with_git_root(dir.path().join("jj"), Some(&git_root)).unwrap();
    let mut client = serve(engine).await;

    client
        .init_repo(InitRepoRequest {
            name: "chrome".into(),
        })
        .await
        .unwrap();
    let base = client
        .commit(commit_req(
            "chrome",
            "agent-a",
            "base",
            vec![file("README.md", "# chrome\n")],
        ))
        .await
        .unwrap()
        .into_inner();
    // A second commit whose git object exists — targets for the external
    // ref moves below.
    let side = client
        .commit(CommitRequest {
            repo: "chrome".into(),
            parent_commit_ids: vec![base.commit_id.clone()],
            files: vec![file("side.txt", "woven elsewhere\n")],
            deleted_paths: vec![],
            message: "merged through the collaboration shell".into(),
            author_name: "human".into(),
            author_email: "human@clotho.internal".into(),
        })
        .await
        .unwrap()
        .into_inner();
    // Simulate Forgejo moving refs/heads/main behind jj's back (its UI
    // merges and pushes write git refs directly, never through the engine).
    let git_repo = gix::open(git_root.join("chrome.git")).unwrap();
    let side_oid = gix::ObjectId::from_hex(side.commit_id.as_bytes()).unwrap();
    let base_oid = gix::ObjectId::from_hex(base.commit_id.as_bytes()).unwrap();
    // External actor resets main to base (e.g. a force-push through Forgejo).
    git_repo
        .reference(
            "refs/heads/main",
            base_oid,
            gix::refs::transaction::PreviousValue::Any,
            "forgejo: external move",
        )
        .unwrap();

    // The next engine operation absorbs the external move: main now reads
    // as base, and the op log records the import.
    let heads = client
        .get_heads(GetHeadsRequest {
            repo: "chrome".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        heads.main_commit_id, base.commit_id,
        "external move imported"
    );
    let log = client
        .query_op_log(QueryOpLogRequest {
            repo: "chrome".into(),
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(
        log.entries
            .iter()
            .any(|e| e.description == "import git refs"),
        "import recorded in the op log: {:?}",
        log.entries
            .iter()
            .map(|e| e.description.as_str())
            .collect::<Vec<_>>()
    );

    // And forward moves work too: external actor advances main to `side`.
    git_repo
        .reference(
            "refs/heads/main",
            side_oid,
            gix::refs::transaction::PreviousValue::Any,
            "forgejo: merge",
        )
        .unwrap();
    let heads = client
        .get_heads(GetHeadsRequest {
            repo: "chrome".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(heads.main_commit_id, side.commit_id);
}

/// Stage 4 additions (docs/prd.md §5): the orientation RPCs backing the MCP
/// `orient_repo` tool (heads + tree summary) and the raw-material RPC backing
/// `diff_symbol` (changed files with full before/after contents).
#[tokio::test]
async fn heads_files_and_commit_diffs_are_queryable() {
    let dir = tempfile::tempdir().unwrap();
    let mut client = start_server(dir.path()).await;

    client
        .init_repo(InitRepoRequest {
            name: "orient".into(),
        })
        .await
        .unwrap();

    let first = client
        .commit(commit_req(
            "orient",
            "agent-a",
            "add lib and readme",
            vec![
                file("src/lib.rs", "pub fn spin() -> u32 { 1 }\n"),
                file("README.md", "# orient\n"),
            ],
        ))
        .await
        .unwrap()
        .into_inner();
    let second = client
        .commit(CommitRequest {
            repo: "orient".into(),
            parent_commit_ids: vec![],
            files: vec![file(
                "src/lib.rs",
                "pub fn spin() -> u32 { 2 }\npub fn weave() {}\n",
            )],
            deleted_paths: vec!["README.md".into()],
            message: "rework lib, drop readme".into(),
            author_name: "agent-b".into(),
            author_email: "agent-b@agents.clotho.internal".into(),
        })
        .await
        .unwrap()
        .into_inner();

    // Heads: one head (linear history), main tracking it, real metadata.
    let heads = client
        .get_heads(GetHeadsRequest {
            repo: "orient".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(heads.main_commit_id, second.commit_id);
    assert_eq!(heads.heads.len(), 1);
    let head = &heads.heads[0];
    assert_eq!(head.commit_id, second.commit_id);
    assert_eq!(head.description, "rework lib, drop readme");
    assert_eq!(head.author_name, "agent-b");
    assert_eq!(head.parent_commit_ids, vec![first.commit_id.clone()]);

    // Tree summary: default commit is main's target; sizes are real.
    let list = client
        .list_files(ListFilesRequest {
            repo: "orient".into(),
            commit_id: String::new(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(list.commit_id, second.commit_id);
    assert_eq!(list.files.len(), 1);
    assert_eq!(list.files[0].path, "src/lib.rs");
    assert_eq!(
        list.files[0].size_bytes,
        "pub fn spin() -> u32 { 2 }\npub fn weave() {}\n".len() as u64
    );

    // An explicit commit id lists that tree instead.
    let list_first = client
        .list_files(ListFilesRequest {
            repo: "orient".into(),
            commit_id: first.commit_id.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    let paths: Vec<_> = list_first.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["README.md", "src/lib.rs"]);

    // Diff: from defaults to the first parent; contents come back whole.
    let diff = client
        .diff_commits(DiffCommitsRequest {
            repo: "orient".into(),
            from_commit_id: String::new(),
            to_commit_id: second.commit_id.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(diff.from_commit_id, first.commit_id);
    assert_eq!(diff.files.len(), 2);
    let readme = diff.files.iter().find(|f| f.path == "README.md").unwrap();
    assert_eq!(readme.kind(), ChangeKind::Deleted);
    assert_eq!(readme.old_content, b"# orient\n");
    assert!(readme.new_content.is_empty());
    let lib = diff.files.iter().find(|f| f.path == "src/lib.rs").unwrap();
    assert_eq!(lib.kind(), ChangeKind::Modified);
    assert_eq!(lib.old_content, b"pub fn spin() -> u32 { 1 }\n");
    assert_eq!(
        lib.new_content,
        b"pub fn spin() -> u32 { 2 }\npub fn weave() {}\n"
    );
}
