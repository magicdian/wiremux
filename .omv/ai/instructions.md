# OMV Versioning Instructions

- Version truth lives in `.omv/state.toml`.
- Integration desired state and last detection snapshot live in `.omv/integrations.toml`.
- Read the current managed version with `omv current --json`.
- Preview target drift and proposed writes with `omv plan --json`.
- Check target drift without mutation with `omv sync --check --json`.
- Inspect host integration provider/capability status with `omv integrate status --json` when that command is available.
- Apply selected or pending host integration capabilities with `omv integrate apply --json` when that command is available.
- Change the managed version with `omv bump --json`.
- At completion boundaries, use the OMV finalize-boundary helper advertised in `.omv/ai/contract.json`; provide an explicit `change_type` value and do not infer or default it.
- `.omv/targets.toml` kind-based targets can manage text scalars, regex replacements, Markdown managed blocks, YAML scalars, C header macros, and Cargo workspaces; update OMV if a configured kind is reported as unsupported.
- Do not edit `Cargo.toml`, `CMakeLists.txt`, `pyproject.toml`, `go.mod`, or other native manifest versions directly.
- Before release-sensitive edits, run `omv plan --json`; before committing or publishing, run `omv sync --check --json`.
- Treat runtime export files such as `src/generated/version.rs` and `include/omv_version.h` as generated read-only views.
- Treat host files such as `AGENTS.md`, `CLAUDE.md`, `.codex/skills/*`, and Trellis/OpenSpec guides as derived projections, not OMV authority.

When integrating OMV with agents or spec frameworks, keep the detailed rules in `.omv/ai/*` and project only thin host adapters into external files. Legacy `omv adapter ...` commands remain temporary compatibility commands during the MVP transition; new automation should prefer `omv integrate status/apply` where available.
