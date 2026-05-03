<!-- OMV-MANAGED-FILE source=.omv/ai/adapters/trellis/guide.md contract=1 -->
# OMV Versioning Guide

- `.omv/state.toml` is the version source of truth.
- Use `omv current --json` for reads.
- Use `omv plan --json` to preview target changes.
- Use `omv sync --check --json` to verify drift without mutation.
- Use `omv integrate status --json` and `omv integrate apply --json` for host integration provider/capability state where available.
- If a Trellis finalize-boundary capability is installed, call the OMV helper advertised in `.omv/ai/contract.json` after `/trellis:finish-work` succeeds; supply an explicit `change_type`.
- Use `omv bump --json` for writes.
- Do not trust manifest versions as authority.
- Do not treat this guide or other host files as OMV authority.

Canonical reference: `./.omv/ai/instructions.md`