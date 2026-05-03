---
name: omv-versioning
description: "Use OMV as the version source of truth for this project."
---

<!-- OMV-MANAGED-FILE source=.omv/ai/adapters/codex/SKILL.md contract=1 -->

1. Read `./.omv/ai/instructions.md`.
2. Use `omv current --json` to inspect current version truth.
3. Use `omv plan --json` or `omv sync --check --json` before changing version-sensitive files.
4. Use `omv integrate status --json` to inspect host integration state and `omv integrate apply --json` to apply selected capabilities where available.
5. At completion boundaries, call the OMV finalize-boundary helper from `./.omv/ai/contract.json` only with an explicit `change_type`; ask the user when the value is missing.
6. Use `omv bump --json` to mutate version truth.
7. Do not hand-edit manifest versions or treat host adapter files as authority.