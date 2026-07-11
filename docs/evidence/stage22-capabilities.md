# Stage 22 capability-discovery evidence

**Date:** July 11, 2026

The versioned source document is `docs/capabilities.json`. The canonical REST
edge serves it at `GET /api/v1/capabilities`; the MCP gateway serves the exact
same embedded bytes as JSON at `GET /capabilities`. OpenAPI and the JavaScript
SDK include the REST operation. The contract declares the public surfaces,
repository kinds, agent trust boundary, pagination/idempotency limits, provider
families, stability, and canonical REST ownership without exposing internal
topology or credentials.
