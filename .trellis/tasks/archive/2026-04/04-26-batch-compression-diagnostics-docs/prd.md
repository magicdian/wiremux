# PR4: Batch Compression Diagnostics and Docs

## Goal

Expose enough diagnostics to compare heatshrink and LZ4 on real ESP hardware and
document the new channel policy behavior.

## Requirements

* Emit or expose per-codec metrics: `raw_bytes`, `encoded_bytes`, `ratio`,
  `encode_us`, `decode_ok`, `fallback_count`, and `heap_peak`.
* Update docs for current defaults and policy examples.
* Update specs if protocol or API contracts change.

## Acceptance Criteria

* [ ] Host can display batch/compression metadata.
* [ ] ESP demo has a path to generate representative compressed batch traffic.
* [ ] Documentation explains console/log/telemetry default policies.

## Technical Notes

* Final hardware validation is deferred to unified user acceptance after all 4 PRs.
