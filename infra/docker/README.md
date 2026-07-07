# infra/docker/

Dockerfiles for Clotho's services, consumed by the root
`docker-compose.dev.yml`.

`rust-services.Dockerfile` is a single multi-stage build for the whole Cargo
workspace; each compose service selects its binary via the `SERVICE` build arg.
