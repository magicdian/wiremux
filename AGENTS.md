<!-- TRELLIS:START -->
# Trellis Instructions

These instructions are for AI assistants working in this project.

This project is managed by Trellis. The working knowledge you need lives under `.trellis/`:

- `.trellis/workflow.md` — development phases, when to create tasks, skill routing
- `.trellis/spec/` — package- and layer-scoped coding guidelines (read before writing code in a given layer)
- `.trellis/workspace/` — per-developer journals and session traces
- `.trellis/tasks/` — active and archived tasks (PRDs, research, jsonl context)

If a Trellis command is available on your platform (e.g. `/trellis:finish-work`, `/trellis:continue`), prefer it over manual steps. Not every platform exposes every command.

If you're using Codex or another agent-capable tool, additional project-scoped helpers may live in:
- `.agents/skills/` — reusable Trellis skills
- `.codex/agents/` — optional custom subagents

Managed by Trellis. Edits outside this block are preserved; edits inside may be overwritten by a future `trellis update`.

<!-- TRELLIS:END -->

<!-- OMV-MANAGED-BEGIN:integration-codex-project-instructions -->
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
<!-- OMV-MANAGED-END:integration-codex-project-instructions -->
