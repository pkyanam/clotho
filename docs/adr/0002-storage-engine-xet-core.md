# ADR-0002: Build the Arachne storage engine on xet-core

- **Status:** Accepted
- **Date:** 2026-07-07
- **Deciders:** Clotho core

## Context

Clotho targets repositories that carry large binary artifacts — model weights,
datasets, design files, video — where git LFS's file-level dedup fails: a
one-byte edit re-uploads and re-stores the whole file.

Hugging Face's **Xet protocol** solves this with content-defined chunking and
is published as an open, implementation-agnostic spec (not an HF-internal
system):

- **Content-defined chunking** (GearHash rolling hash, ~64 KiB average,
  8–128 KiB bounds): inserts/deletes only affect nearby chunks, so dedup
  survives edits.
- **Xorbs**: chunks batched into ~64 MiB immutable containers, so a multi-GB
  file doesn't become tens of thousands of S3 objects/HTTP requests.
- **Permission-aware global dedup**: identical content across repos is stored
  once, with access enforced at the chunk level.
- The reference implementation (`xet-core`) is Apache/MIT-licensed Rust we can
  embed directly, rather than reimplementing chunking and xorb formats.

Alternatives considered:

- **git LFS as-is** — rejected: file-level dedup, no chunking; kept only as a
  pointer-format compatibility bridge at the edges.
- **Reimplement chunking from scratch** — rejected: no value over embedding a
  vetted implementation of a published spec; interop with the Xet ecosystem
  would be lost.

## Decision

`crates/clotho-storage` (the **Arachne engine**) embeds **`xet-core`** and
implements upload (chunk → xorb → S3 write) and download (reconstruct from
xorb + chunk ranges) against any S3-compatible backend (MinIO in dev).

Protocol-first: Clotho implements a compatible server without depending on
HF's infrastructure; longer-term interop with Xet-based storage is possible
rather than requiring migration. A git-LFS pointer bridge is kept at the edges
for import/migration compatibility.

## Consequences

- Dedup is **measured, not assumed**: the Stage 2 exit condition requires
  uploading a multi-GB file plus a near-duplicate and showing only changed
  chunks were newly written, with byte-identical reconstruction.
- `xet-core` is designed around HF's CAS service; our S3/MinIO backend needs a
  content-addressed-store shim. This is real, budgeted work even though the
  chunking/xorb logic is reusable as-is.
- Storage becomes a differentiator vs. GitHub+LFS for ML/data-heavy repos
  without users thinking about "which storage system is this file in."
