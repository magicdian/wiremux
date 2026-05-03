<!-- OMV-MANAGED-FILE source=.omv/ai/adapters/openspec/project.md contract=1 -->
# OMV Version Governance

This project uses `omv` as the authoritative version source.

- Version truth: `.omv/state.toml`
- Read current version: `omv current --json`
- Preview sync plan: `omv plan --json`
- Check drift without writes: `omv sync --check --json`
- Check host integration status: `omv integrate status --json` where available
- Apply selected host integration capabilities: `omv integrate apply --json` where available
- Update version truth: `omv bump --json`
- Native manifests are synchronized outputs, not authority
- Host adapter/spec files are derived projections, not authority

See `./.omv/ai/instructions.md` for the canonical workflow.