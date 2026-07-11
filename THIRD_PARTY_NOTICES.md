# Third-party notices

Clotho's own source is licensed under Apache-2.0. Its Cargo and pnpm lockfiles
identify third-party packages, which remain under their respective licenses.
Official release artifacts must include a generated dependency license report
and SBOM; this file does not replace those upstream license texts.

`collab/forgejo` is a pinned Git submodule containing unmodified Forgejo,
licensed separately under GPL-3.0-or-later. Forgejo is built and distributed as
a separate internal provider process. Clotho does not relicense or copy its
source into Clotho services, and public clients do not depend on Forgejo APIs,
URLs, database schemas, UI, or terminology.

Optional provider services and SDK bridges remain separate dependencies behind
Clotho-owned interfaces. Their availability does not imply endorsement or
change their upstream license, terms, or credential requirements.
