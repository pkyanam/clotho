# Stage 22 API contract evidence

**Baseline:** `ceb5a54`

**Audited:** July 11, 2026

## Deterministic inventory

`pnpm test:contract` verifies:

- 108 OpenAPI operations equal 108 Axum method/path registrations;
- all internal OpenAPI references resolve;
- every operation has a unique ID, summary, mutation request schema, success
  response/schema, declared path parameters, and effective alpha auth/error
  metadata;
- 92 direct SDK REST calls cover every canonical operation, with the binary
  release GET/HEAD pair mapped explicitly to `downloadReleaseFile`;
- 70 SDK interfaces are inspected; shared OpenAPI schemas match property names,
  required versus optional fields, and base types.

Run `pnpm test:contract -- --json` for the sorted machine-readable inventory.
That output is deterministic and can be compared between release refs.

## Diff from the baseline

| Class                             | Result                                                                                            |
| --------------------------------- | ------------------------------------------------------------------------------------------------- |
| Added/removed HTTP operations     | None                                                                                              |
| Runtime URL-shape change          | None                                                                                              |
| Corrected runtime parameter names | Org/repo secret detail routes now use OpenAPI's `secretName`; URL matching is unchanged           |
| Response schemas                  | Added missing Hugging Face compatibility, binary-download, and webhook schemas                    |
| Requiredness                      | OpenAPI component requirements now match exported SDK interfaces                                  |
| Stability/auth/error metadata     | Added inherited public-alpha contract metadata plus public health/spec and HMAC-webhook overrides |
| SDK runtime behavior              | Unchanged                                                                                         |

This is a documentation/verification correction rather than a public runtime
behavior change. Tightened OpenAPI `required` arrays may reveal invalid mocks
that omitted fields Clotho and the SDK already treat as present; that is an
intentional alpha-contract correction.
