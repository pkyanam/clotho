# Clotho StorageSDK bridge

Internal adapter bridge for the storage layer of Clotho's Provider Fabric.
It uses the open-source [StorageSDK](https://storagesdk.dev/) interface so
Clotho can probe and operate S3, MinIO, R2, and local development storage
without teaching the product API vendor-specific verbs.

The bridge also exposes StorageSDK snapshots and forks. These map naturally
to agent workflows: freeze a repo artifact namespace, fork it for an agent
run, then discard or retain the fork independently of git history.

The default Clotho stack does not require this sidecar or any credentials;
Arachne continues to use managed MinIO. Start it with the
`storage-bridge` Compose profile when connecting an external provider.
