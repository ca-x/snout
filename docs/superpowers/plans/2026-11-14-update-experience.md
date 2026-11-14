# Update Experience Implementation Plan

> Execute inline with the executing-plans workflow, one verified task at a time.

**Goal:** Make update decisions obvious in desktop and terminal, and replace log-first installation feedback with a truthful step timeline.

**Architecture:** The shared application layer owns typed component freshness and operation steps. Desktop NSH and terminal adapters render the same states using their existing semantic tokens. Logs remain available as secondary diagnostics.

**Tech Stack:** Rust, existing NSH 0.4.0 native desktop, Ratatui, existing updater events.

**Spec:** User requests current/latest/update-needed states in desktop and terminal, step/timeline progress with loading animation, then a tested release. Existing `docs/desktop-design.md` remains normative.

## Constraints
- Preserve AUR self-update prohibition, preview side-effect isolation, and the independent CLI workspace.
- Never label unchecked/offline/unsupported components as current. Compare the updater's actual identity/hash/time rules, not version strings alone.
- No synthetic percentage or animation outside active work; respect reduced motion. Keep keyboard cancellation available.
- Publish the next available 0.5.x only after validation. Do not move existing tags.

## Tasks
- [x] Add typed component status and regression tests for equal tag with changed hash/time, absent files, unsupported components, and failed checks.
- [x] Show shared status, current/latest versions and relevant actions in desktop component cards, TUI menu and CLI status command; check automatically on desktop startup and invalidate stale checks after mutations.
- [x] Add shared event-driven workflow timeline, tests for completion/failure/cancellation, desktop step UI with active-work loading animation, and terminal timeline rendering. Move raw diagnostics behind disclosure.
- [ ] Exercise headless interactions, both languages, reduced motion and light/dark native Linux screenshots without invoking real input-method updates. Headless scenarios pass; native fixture launches but the current Xpra display returns a blank root capture / BadMatch for the window. Native visual acceptance remains unverified.
- [ ] Run both workspaces' tests/fmt/Clippy/docs and packaging checks, update release notes/version, then tag/release and verify GitHub assets and both AUR packages.

## Local verification

- Root: 273 unit tests + 6 CLI contract tests; fmt, all-target/all-feature Clippy and docs pass.
- Desktop: 34 unit tests + 22 UI tests + 1 shutdown test pass in native and headless builds; 3 manual visual fixtures ignored. Native/headless Clippy, fmt and docs pass.
- Packaging: 15 Python tests, both PKGBUILD shell syntax checks and diff whitespace check pass.
- All UI operations use isolated fixtures; no real Rime update was performed.
