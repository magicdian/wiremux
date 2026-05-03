<!-- OMV-MANAGED-FILE source=.omv/ai/adapters/codex/AGENTS.md contract=1 -->
# OMV Codex Adapter

Read `./.omv/ai/instructions.md` before touching project versions.

- Use `omv current --json` to inspect the managed version.
- Use `omv plan --json` before editing version-sensitive surfaces.
- Use `omv sync --check --json` to verify target drift without writing.
- Use `omv integrate status --json` and `omv integrate apply --json` for host integration provider/capability status where available.
- At finalize boundaries, call the OMV finalize-boundary helper from `.omv/ai/contract.json` only after tests pass and only with an explicit `change_type`.
- Use `omv bump --json` to advance the managed version.
- Do not edit native manifest versions directly.
- Treat this host file as a derived projection; `.omv/*` and `.omv/ai/*` remain authoritative.