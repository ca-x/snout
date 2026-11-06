# snout Desktop Specification

Status: implementation scope established by the user's NSH, full-feature parity, and design-guide requirements.

## Objective

Add a native desktop application built with longbridge/nsh without removing or reducing the existing CLI and TUI. The desktop application must expose every existing CLI capability, reuse the same business logic, and preserve existing configuration and user data.

## Architecture

- A shared Rust application crate owns configuration, downloads, updates, deployment, model patches, themes, and structured results.
- A separate Rust NSH desktop crate owns native screens and interaction on Windows, macOS, and Linux.
- The CLI and TUI remain independently buildable without native desktop dependencies.
- Network, file, hook, and deployment work runs outside the UI thread and reports progress, completion, and cancellation through messages.
- The desktop application does not execute a shell command assembled from user input to implement core operations.

## Feature Parity

| Existing capability | Desktop entry |
| --- | --- |
| Interactive initialization | Setup screen with scheme selection and explicit installation confirmation |
| Update all, scheme, dictionary, model | Separate actions with progress, results, and cancellation |
| All 13 scheme variants | Scheme selector shared with the CLI |
| Enable/disable model patch | Setting that synchronizes the preference and the patch on disk |
| Mirror and download threads | Download settings with the same validation and semantics |
| Proxy enable/disable, type, address | Network settings, including environment-proxy visibility |
| GitHub token | Masked credential field; never include the token in exported results |
| Chinese/English | Language setting for all desktop screens |
| TUI auto/light/dark theme | Existing TUI preference remains editable; desktop follows system appearance |
| User-data prompt/preserve/discard | Explicit policy selector and confirmation before discarding data |
| Engine sync and link/copy strategy | Platform-aware settings and status |
| Pre-update and post-update hooks | Explicitly editable script file paths with a warning before execution; requires `sh` on `PATH` |
| Auto update and countdown | Configuration and the same startup behavior, with visible cancellation |
| Exclusion patterns | Add, edit, remove, reset, and validate patterns |
| Skin patch key and built-in skins | Skin screen including selection, default skin, and rounding |
| Fcitx5 light/dark themes and rounding | Linux-only theme selection with unsupported-platform messaging |
| Candidate page size and reset | Validated 1–9 selector and explicit reset |
| JSON results | Export the same structured report rather than expose raw console output |
| zsh/fish completions | Generate and export completion files |
| Existing TUI diagnostics | Wanxiang diagnosis, installed engines, component versions, and Rime setup status |

## Visual And Interaction Direction

- Follow `ui-ux-pro-max`, `apple-design`, `frontend-design`, and `refactoring-ui` as requested.
- Build a quiet input-method workbench, not a marketing dashboard or a wall of CLI flags.
- Use native window chrome, system fonts, a neutral fog/graphite surface, and one restrained amber accent referencing the existing snout identity.
- Separate Overview, Schemes, Appearance, Settings, and Activity with a compact sidebar. Give updates one clear primary action; put advanced network and hook options in meaningful settings groups.
- Use 12/14/16/20/28-point type roles and deliberate 4/8/12/16/24/32-point spacing, with stronger separation between groups than within groups.
- Use real native controls with visible labels, tab navigation, focus feedback, masked secrets, inline validation, and meaningful disabled/busy states.
- Use restrained action feedback only, respect reduced motion, and do not delay user input for transitions. Native desktop conventions override web-only cursor or marketing-layout suggestions.
- Every empty, loading, progress, success, partial-failure, cancellation, and invalid-input state must be designed explicitly.

## Safety And Compatibility

- Existing CLI flags, direct-action exit codes, JSON schema, and configuration keys remain compatible. Failed initialization now returns a nonzero status instead of false success.
- No download, overwrite, deployment, hook, or destructive user-data action happens merely by opening a screen.
- A running operation disables conflicting actions; closing the window requests cancellation.
- Failed operations and invalid settings remain visible and do not report success.
- Tests use temporary configuration and Rime directories, never the user's live configuration.
- Custom data roots do not isolate updater platform effects. Desktop preview explicitly rejects effectful updates, hooks, deployment, and engine integration before calling the shared core.
- CLI distribution remains static where currently supported; Linux desktop distribution declares its native runtime dependencies.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Desktop acceptance additionally requires native builds on Windows, macOS, and Linux; headless interaction tests of the actual NSH root; a manual desktop smoke test when a display is available; and a checked feature-parity matrix. Release publication follows the existing Release workflow and its AUR job, with published artifacts read back and verified.
