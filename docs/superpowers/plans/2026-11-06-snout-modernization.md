# snout Modernization Implementation Plan

> **For agentic workers:** Execute the tasks in order and request one fresh-context review before publication. Track verification evidence rather than treating a clean diff as proof.

**Goal:** Resolve the Windows TUI issue, improve the entire Rust codebase without breaking compatibility, and add a full-featured NSH desktop application sharing application services with CLI/TUI.

**Architecture:** Preserve CLI and TUI behavior while extracting their shared Rust application core. Keep native desktop dependencies in a separate crate so CLI builds do not require GTK, AppKit, or Win32 UI dependencies.

**Tech Stack:** Rust, Tokio, clap, ratatui, longbridge/nsh, GitHub Actions.

**Spec:** `docs/desktop-spec.md`, issue #5, PR #6, and the current CLI contract in `src/cli.rs`.

## Global Constraints

- Cover the entire Rust project, not just the files changed in PR #6.
- Preserve CLI flags, direct-action exit codes, JSON schema, configuration keys, existing workflows, and user data; correct failed initialization's false-success exit status.
- Use the existing Release workflow for GitHub Release creation and AUR publishing.
- Do not claim Windows responsiveness measurements without running on Windows.
- Keep application services independent from ratatui and NSH; frontend adapters own interaction only.

## Review Focus

- Rendering and idle polling must not create a Manager or launch engine-detection subprocesses.
- Terminal initialization failures and early returns must restore terminal state.
- Cancellation must remain cancellation, including when a mirror request fails and falls back through a proxy.
- Cross-platform tests must compile and must not mutate a user's real configuration.
- Desktop actions must share core logic and retain every existing CLI capability.

## Tasks

### 1. Review And Resolve PR #6

- [x] Read the complete PR and issue, freeze the base/head, and inspect existing CI.
- [x] Approve the external-contributor CI run after reviewing its scope.
- [x] Fix two redundant references that fail strict Clippy and push the repair to the existing PR branch.
- [x] Run formatting, all 127 tests, and strict Clippy on the repaired PR.
- [x] Wait for every CI build to pass, merge the PR, and synchronize local master.
- [x] Confirm the issue-reply receipt.

Evidence: merged commit `a3ac6fcf6a152124a1df983f4114c558b8606755`; PR CI run `42236541834` completed successfully.

### 2. Extract And Harden The Shared Application Core

- [x] Extract `src/lib.rs` and a small CLI entry point without changing parsing or output behavior.
- [x] Centralize the duplicated scheme/dictionary/model operation dispatch used by frontends.
- [x] Apply concrete findings from a whole-project Rust review: ownership, client reuse, panic-free errors, and useful API boundaries.
- [x] Fix the Windows test helper and ensure tests do not depend on mutable live configuration.
- [x] Add focused regression tests and verify each meaningful behavioral fix.
- [x] Add reliable terminal cleanup for setup failures and early returns.
- [x] Run the full verification suite after the refactor.

### 3. Implement Full-Featured Desktop

- [x] Confirm the desktop architecture in `docs/desktop-spec.md`.
- [x] Add the separate native NSH application crate and the shared-core bridge.
- [x] Implement every entry in the parity matrix, including platform-aware settings, export actions, progress, cancellation, and error presentation.
- [x] Add headless NSH interaction tests and native build coverage.
- [x] Verify the real Linux window in an isolated preview; inspect a window-only capture and close that window cleanly.

### 4. Review And Release

- [x] Obtain a fresh-context whole-change review and address its concrete findings.
- [x] Select and synchronize release 0.3.0 across both manifests and lockfiles after implementing the feature scope.
- [x] Verify local source, packages, and Linux native artifacts.
- [ ] Require successful cross-platform CI for the pushed source commit before tagging.
- [ ] Trigger the existing Release workflow and verify all binaries and AUR publication.
- [ ] Update the issue reply to name the actual published version and read it back.

## Decisions

- Use current repository files and live GitHub state as release evidence. The existing `.github/workflows/release.yml` handles GitHub Release creation and AUR publication.
- Use NSH 0.4.0 Rust-only native backends for Windows, macOS, and Linux in an independent desktop crate. The user explicitly requires shared services and separate TUI/GUI implementations.

## Verification Ledger

- Shared core: 174 unit tests passed after extraction; strict all-features Clippy and library documentation passed. Later source changes require a fresh final run.
- CLI compatibility: a clean archive of merged HEAD produces byte-identical help, zsh/fish completions, JSON parse errors, and unsupported-dictionary results; exit codes match (0/0/0/2/1).
- Candidate size validation: regression fails without the core guard and passes with it; invalid sizes leave existing YAML unchanged.
- Additional shared services cover settings validation/rollback, atomic configuration persistence, appearance capability discovery, and component status checks.
- Pre-hardening shared-core run: 189 unit tests and three CLI contract integration tests passed; later security and lifecycle changes require the final release validation below.
- PTY smoke: both normal quit and a forced setup write failure restored canonical input and echo; normal quit left the alternate screen.
- Linux native desktop: optimized build and actual GNU archive inspection passed. Native/headless suites passed eight controller and ten actual-root interaction tests before final preview-isolation hardening.
- Packaging: nine stdlib tests passed, covering Linux executable/desktop entry, both macOS bundle plists, Windows archive, version/tag agreement, and known SHA256 digests.
- Review finding: temporary Manager paths do not sandbox global deployment or hooks. Preview now rejects effectful updates before shared-core dispatch; a fresh regression and final review are required before publication.
- Review finding: inverse model-patch rollback deleted custom grammar values and comments. Two regressions failed before replacing inverse operations with exact byte/existence snapshots; all 11 application tests then passed.
- Ruling: correct failed `--init` to exit unsuccessfully, rather than preserve the old false-success bug. Direct-action CLI compatibility remains required, and the initialization behavior correction is called out in release notes.
- A hook-path regression failed with shell interpolation and passed after passing the script path as one `sh` argument; all four hook tests passed.
- Archive/merge regressions failed before staging, normalization, and no-follow preflight. The final scoped host run passed 32 tests across extraction, base updater, and Mint.
- Ice/Frost cache regressions failed when required installed files were removed despite matching metadata. Wanxiang adoption regressions failed when checksum-valid files did not repair missing/stale version records; scoped tests passed after both fixes.
- Native process-close subprocess regressions now cover controller drop, retained NSH root, explicit shutdown, and process-owner drop. The UI close returns promptly while process ownership waits for the real hook to finish and be reaped.
- Fcitx5 theme actions now use shared application dispatch and JSON reports. All-failed reports are errors; only mixed success/failure is partial failure. Native/headless desktop suites passed 23 tests each before the final release rebuild.
- The sibling sweep extended installation-presence guards to Mint and all ten Wanxiang dictionary variants. Mint reproduced two failing regressions and passed all 11 scoped tests; Wanxiang reproduced three failing regressions and passed all 17 scoped tests.
- The independent full headless run exposed a test scheduling flaw: NSH `Headless::press` can return before a queued navigation event is rendered. Tests now wait for both selected navigation and mounted page content; a channel-gated regression proves this without sleeps, retries, or serialization. Thirty headless and ten native repeated runs passed; production UI behavior did not need changing.

## Final Local Release Gate

- The frozen shared core passes 235 unit tests and three CLI contract integration tests, formatting, all-targets/all-features strict Clippy, documentation, and an optimized 0.3.0 build.
- Native and no-default-features desktop suites each pass 24 tests: 12 controller, 11 actual NSH root, and one subprocess regression covering four shutdown variants. Desktop formatting, strict Clippy, documentation, and optimized Linux native build pass.
- All nine packaging tests, workflow actionlint/ShellCheck, and diff whitespace checks pass. The Linux GNU archive contains the actual optimized executable with executable permissions and the matching desktop entry.
- The actual Linux GTK window opens in isolated preview and accepts `WM_DELETE_WINDOW`, with process exit 0 and no Rime installation effects. The window is identified by its own PID, title, and close protocol, not by the GTK helper window.
- Independent review closes every known P1/P2 finding. Windows/macOS runtime behavior and real input-method updates are not claimed as locally tested.
- The first pushed candidate CI exposed non-Linux dead code and HTTP test-server portability bugs; that run was cancelled after its native packages completed and is not a passing release gate. The correction removes unused platform stubs, avoids real Windows input-method shutdown in unit tests, and makes mock HTTP servers consume requests and serve concurrent ranges with bounded waits. No production download logic changes.
- The corrected candidate repeats every local gate successfully: 235 shared-core unit tests, three CLI integration tests, both 24-test desktop modes, strict Clippy, formatting, documentation, optimized builds, and workflow checks. The actual four-platform native CI archives from the first candidate also pass architecture, version, bundle, and executable-permission inspection; corrected-source CI is still required before tagging.
- This document records release-candidate preparation. Cross-platform CI, tag publication, uploaded-asset checksums, AUR publication, and the final issue reply are follow-through gates recorded in GitHub Actions and the release receipts.
