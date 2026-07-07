//! Stage 5 exit-condition tests (docs/prd.md §5): two simulated agents
//! committing concurrently to the same repo end up reconciled into one graph
//! without a human intervening (non-conflicting case); the conflicting case
//! produces a clearly surfaced, first-class conflict commit — and the queue
//! never blocks.

use clotho_common::pb::mergequeue::v1::{
    merge_queue_client::MergeQueueClient, SubmitChangeRequest,
};
use clotho_common::pb::vcs::v1::{
    vcs_client::VcsClient, CommitRequest, FileChange, GetHeadsRequest, InitRepoRequest,
    ListFilesRequest, QueryOpLogRequest,
};
use clotho_merge_queue::MergeQueueService;
use clotho_vcs::{VcsEngine, VcsService};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Server};

/// In-process stack: a real vcs gRPC server and a real merge-queue gRPC
/// server wired to it — the same processes the dev stack runs.
async fn start_stack(root: &std::path::Path) -> (VcsClient<Channel>, MergeQueueClient<Channel>) {
    let vcs_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let vcs_addr = vcs_listener.local_addr().unwrap();
    tokio::spawn(
        Server::builder()
            .add_service(VcsService::new(VcsEngine::new(root).unwrap()).into_server())
            .serve_with_incoming(TcpListenerStream::new(vcs_listener)),
    );

    let queue_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let queue_addr = queue_listener.local_addr().unwrap();
    tokio::spawn(
        Server::builder()
            .add_service(
                MergeQueueService::new(&format!("http://{vcs_addr}"))
                    .unwrap()
                    .into_server(),
            )
            .serve_with_incoming(TcpListenerStream::new(queue_listener)),
    );

    (
        VcsClient::connect(format!("http://{vcs_addr}"))
            .await
            .unwrap(),
        MergeQueueClient::connect(format!("http://{queue_addr}"))
            .await
            .unwrap(),
    )
}

fn commit_on(
    repo: &str,
    parent: &str,
    agent: &str,
    message: &str,
    path: &str,
    content: &str,
) -> CommitRequest {
    CommitRequest {
        repo: repo.into(),
        // Empty parent means "commit on the current head(s)" (base commit).
        parent_commit_ids: if parent.is_empty() {
            vec![]
        } else {
            vec![parent.into()]
        },
        files: vec![FileChange {
            path: path.into(),
            content: content.as_bytes().to_vec(),
            executable: false,
        }],
        deleted_paths: vec![],
        message: message.into(),
        author_name: agent.into(),
        author_email: format!("{agent}@agents.clotho.internal"),
    }
}

#[tokio::test]
async fn concurrent_non_conflicting_commits_reconcile_into_one_graph() {
    let dir = tempfile::tempdir().unwrap();
    let (mut vcs, queue) = start_stack(dir.path()).await;

    vcs.init_repo(InitRepoRequest {
        name: "loom".into(),
    })
    .await
    .unwrap();
    let base = vcs
        .commit(commit_on(
            "loom",
            "",
            "setup",
            "base",
            "README.md",
            "# loom\n",
        ))
        .await
        .unwrap()
        .into_inner();

    // Two agents commit concurrently from the same base — two heads, no
    // blocking, no locks at write time. Side commits leave main at base.
    let a = vcs
        .commit(commit_on(
            "loom",
            &base.commit_id,
            "agent-a",
            "agent a: spin",
            "a.txt",
            "thread a\n",
        ))
        .await
        .unwrap()
        .into_inner();
    let b = vcs
        .commit(commit_on(
            "loom",
            &base.commit_id,
            "agent-b",
            "agent b: weave",
            "b.txt",
            "thread b\n",
        ))
        .await
        .unwrap()
        .into_inner();
    let heads = vcs
        .get_heads(GetHeadsRequest {
            repo: "loom".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(heads.heads.len(), 2, "two concurrent agent heads");
    // Agent a's commit extended main (fast-forward); agent b's sibling did
    // not move it — landing b is the queue's job.
    assert_eq!(
        heads.main_commit_id, a.commit_id,
        "sibling commits don't move main"
    );

    // Both agents submit to the queue at the same time; the queue
    // serializes and rebases — no human in the loop.
    let mut qa = queue.clone();
    let mut qb = queue.clone();
    let (ra, rb) = tokio::join!(
        qa.submit_change(SubmitChangeRequest {
            repo: "loom".into(),
            commit_id: a.commit_id.clone(),
        }),
        qb.submit_change(SubmitChangeRequest {
            repo: "loom".into(),
            commit_id: b.commit_id.clone(),
        }),
    );
    let (ra, rb) = (ra.unwrap().into_inner(), rb.unwrap().into_inner());
    assert!(!ra.conflicted && !rb.conflicted);
    // One fast-forwarded, the other was rebased on top of it.
    assert_eq!(
        [ra.fast_forwarded, rb.fast_forwarded]
            .iter()
            .filter(|f| **f)
            .count(),
        1,
        "exactly one submission fast-forwards"
    );

    // One reconciled graph: main holds both agents' files, single head.
    let heads = vcs
        .get_heads(GetHeadsRequest {
            repo: "loom".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(heads.heads.len(), 1, "reconciled into one head");
    assert_eq!(heads.main_commit_id, heads.heads[0].commit_id);
    let files = vcs
        .list_files(ListFilesRequest {
            repo: "loom".into(),
            commit_id: String::new(),
        })
        .await
        .unwrap()
        .into_inner();
    let paths: Vec<_> = files.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["README.md", "a.txt", "b.txt"]);

    // The op log shows both integrations.
    let log = vcs
        .query_op_log(QueryOpLogRequest {
            repo: "loom".into(),
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner();
    let integrations = log
        .entries
        .iter()
        .filter(|e| e.description.starts_with("integrate commit"))
        .count();
    assert_eq!(integrations, 2);
}

#[tokio::test]
async fn conflicting_commits_surface_a_first_class_conflict_without_blocking() {
    let dir = tempfile::tempdir().unwrap();
    let (mut vcs, queue) = start_stack(dir.path()).await;

    vcs.init_repo(InitRepoRequest {
        name: "fates".into(),
    })
    .await
    .unwrap();
    let base = vcs
        .commit(commit_on(
            "fates",
            "",
            "setup",
            "base",
            "config.toml",
            "threads = 1\n",
        ))
        .await
        .unwrap()
        .into_inner();

    // Both agents rewrite the same line of the same file.
    let a = vcs
        .commit(commit_on(
            "fates",
            &base.commit_id,
            "agent-a",
            "agent a: more threads",
            "config.toml",
            "threads = 8\n",
        ))
        .await
        .unwrap()
        .into_inner();
    let b = vcs
        .commit(commit_on(
            "fates",
            &base.commit_id,
            "agent-b",
            "agent b: fewer threads",
            "config.toml",
            "threads = 0\n",
        ))
        .await
        .unwrap()
        .into_inner();

    let mut q = queue.clone();
    let first = q
        .submit_change(SubmitChangeRequest {
            repo: "fates".into(),
            commit_id: a.commit_id.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(first.fast_forwarded && !first.conflicted);

    // The second submission conflicts — and still lands, clearly marked,
    // with main advanced. Nothing deadlocks, nobody waits on a human.
    let second = q
        .submit_change(SubmitChangeRequest {
            repo: "fates".into(),
            commit_id: b.commit_id.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(
        !second.fast_forwarded,
        "same-file divergence can't fast-forward"
    );
    assert!(second.conflicted, "conflict is surfaced, not hidden");
    assert_eq!(second.conflicted_paths, vec!["config.toml"]);
    assert_eq!(second.change_id, b.change_id, "same change, rebased");
    assert_ne!(second.commit_id, b.commit_id);

    let heads = vcs
        .get_heads(GetHeadsRequest {
            repo: "fates".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        heads.main_commit_id, second.commit_id,
        "main advanced through the conflict"
    );
    assert_eq!(heads.heads.len(), 1);

    // The op log records the conflicted integration in plain words.
    let log = vcs
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
        .any(|e| e.description.contains("(conflicted)")));

    // The queue is not blocked: a follow-up commit on the conflicted main
    // integrates cleanly on top.
    let fix = vcs
        .commit(commit_on(
            "fates",
            &second.commit_id,
            "agent-a",
            "resolve config threads",
            "config.toml",
            "threads = 4\n",
        ))
        .await
        .unwrap()
        .into_inner();
    let landed = q
        .submit_change(SubmitChangeRequest {
            repo: "fates".into(),
            commit_id: fix.commit_id.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(landed.fast_forwarded && !landed.conflicted);
}
