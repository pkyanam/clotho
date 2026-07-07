//! Stage 1 exit-condition tests (docs/prd.md §5): a test harness creates a
//! repo, commits from two simulated agents, and reads a unified op log back —
//! entirely through the gRPC API, no shelling out to the `jj` binary. Plus
//! ticket 6's proof: a commit lands as a real git object (verified with gix).

use clotho_common::pb::vcs::v1::{
    vcs_client::VcsClient, CheckpointRequest, CommitRequest, FileChange, InitRepoRequest,
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
