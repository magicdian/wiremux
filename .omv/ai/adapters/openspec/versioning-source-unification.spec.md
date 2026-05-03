<!-- OMV-MANAGED-FILE source=.omv/ai/adapters/openspec/versioning-source-unification.spec.md contract=1 -->
# Spec: Versioning Source Unification

## Requirements

- The project MUST treat `.omv/state.toml` as version truth.
- Workflows MUST read current version via `omv current --json`.
- Workflows SHOULD preview target changes via `omv plan --json`.
- Workflows SHOULD gate drift via `omv sync --check --json` before manual edits or CI checks.
- Workflows SHOULD use `omv integrate status --json` and `omv integrate apply --json` for host integration provider/capability state where available.
- Workflows MUST update managed version via `omv bump --json`.
- Native manifests and runtime export files MUST be treated as derived outputs.
- Host adapter/spec files MUST be treated as derived projections of `.omv/ai/*`.