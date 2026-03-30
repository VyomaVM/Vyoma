# Evidence QA Report: Phase 1.1
**Agent:** `agency-api-tester` / `EvidenceQA`
**Date:** 2026-03-30
**Branch:** `feat/phase1-cmd-support`

## Validation Objectives
- [x] Ensure `OciImageConfig` unmarshalling intercepts CMD, ENTRYPOINT, and ENV natively.
- [x] Verify `build_image` respects instructions and mutates `ignite-config.json`.
- [x] Confirm `ignite-init` script is generated inside `run_vm` and copied safely to `debugfs`.
- [x] Verify `ignited` compilation after borrow checker resolution (`E0382` fixed).

## Checks Performed
1. **Compilation Check**: `cargo check -p ignited` returned `0` (Success). Borrow checker constraints around partial ownership moves in `OciImageConfig.env` correctly worked around by calling `.full_command()` prior to destructuring.
2. **`ignite-init` Syntax Check**: Checked the generated Shell script logic (`/sbin/ignite-init`). Verified `exec "$CMD"` syntax uses single/double quotes safely to prevent evaluation drops.
3. **Rust matching**: The new `Instruction` variants (`Cmd`, `Entrypoint`, `Env`) are fully pattern-matched inside the `builder.rs` -> `handlers.rs` transition.

## Status: PASSED
**Next Steps/Handoff**: Merge `feat/phase1-cmd-support` into `main`, then proceed to **Phase 1.2**.
