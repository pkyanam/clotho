fn main() -> Result<(), Box<dyn std::error::Error>> {
    // tonic-prost-build only emits rerun-if-changed for the listed files via
    // protoc; make proto edits reliably regenerate even on stale caches.
    println!("cargo:rerun-if-changed=../../proto");
    tonic_prost_build::configure().compile_protos(
        &[
            "../../proto/clotho/health/v1/health.proto",
            "../../proto/clotho/vcs/v1/vcs.proto",
            "../../proto/clotho/storage/v1/storage.proto",
            "../../proto/clotho/diff/v1/diff.proto",
            "../../proto/clotho/mergequeue/v1/merge_queue.proto",
        ],
        &["../../proto"],
    )?;
    Ok(())
}
