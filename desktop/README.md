<p align="center">
  <img src="../assets/logo.png" alt="snout Logo" width="200" />
</p>

# snout desktop

A native Rust input-method workbench built with NSH 0.4.0. No webview, JavaScript
runtime, browser assets, or native dependencies are added to the root CLI package.
This directory is a separate Cargo workspace; it consumes `snout` through a path
dependency.

## GitHub authentication

Settings → Downloads & network → GitHub token accepts an optional explicit token.
When blank, the shared core tries `GH_TOKEN`, then `GITHUB_TOKEN`, then the existing
`gh auth token --hostname github.com` login. Without any credentials it uses anonymous
access; installing `gh` is optional. Environment/gh credentials stay in memory and
are never written into settings or forwarded to CNB. Apps launched from the desktop
menu may not inherit a terminal's environment; set the token in settings or launch
`snout-desktop` from that terminal. Rate-limit failures show reset/wait information.

## Branding

The original transparent artwork is preserved in `../assets/logo.png`. The sidebar
embeds its 256 px PNG at compile time, so it needs no external image files or network.
The desktop assets include 256/512 px PNGs for Linux, a multi-size ICNS for macOS,
and a multi-size ICO embedded into the Windows executable by `build.rs`.
Regenerate them with `uv run --with pillow python scripts/generate-desktop-icons.py`
from the repository root.

## Arch Linux Package

```sh
yay -S snout-desktop-bin
```

The binary package currently supports x86_64 and requires GTK 4.14+. It installs
`snout-desktop`, an application-menu entry, and the MIT license. It can coexist
with `snout-bin`, which remains the separate CLI/TUI package. Both share the same
configuration; do not run concurrent updates from different frontends.

The Release workflow publishes this package after uploading the desktop assets.
Maintainers can also run `Publish Desktop AUR` with the latest published stable
tag to publish an existing release without rebuilding or replacing its assets.
The workflow verifies the release checksum, builds the Arch package, and lets
`updpkgsums` generate real AUR checksums; the GitHub PKGBUILD keeps `SKIP` values.

## Run

```sh
cargo run --manifest-path desktop/Cargo.toml --locked
```

Linux builds require GTK 4.14 or newer, Cairo, Pango, GDK Pixbuf, Graphene, GLib,
their development packages, `pkg-config`, and a running graphical session. The
Linux executable dynamically links these native libraries; it is not the static
CLI artifact. Windows and macOS select their respective NSH native runners.

To inspect the app without opening your real configuration or integrating with
live engines, provide an **absolute** temporary preview directory:

```sh
preview=$(mktemp -d)
printf '%s\n' '{"language":"en","auto_update":false}' > "$preview/config.json"
SNOUT_DESKTOP_SANDBOX="$preview" cargo run --manifest-path desktop/Cargo.toml --locked
```

Preview configuration, Rime files, and cache stay under that directory. Engine
discovery, deployment, synchronization, Fcitx theme changes, and manual Rime
integration are disabled in preview. All effectful update actions are rejected
before shared-core dispatch: preview never downloads updates or executes hooks.
The only dispatch exception is a standalone dictionary request for a schema with
no standalone dictionary, which produces the core's side-effect-free unsupported
component report. This permits testing real JSON export without live effects.

## Features

The redesigned workbench uses a persistent icon sidebar, component cards,
wrapping scheme choices, a candidate-count preview, and grouped settings. A fixed
save/revert strip keeps drafts visible across pages. Confirmation dialogs support
Escape; Command/Ctrl+1–5 switches pages and Command/Ctrl+S reviews unsaved changes.
Control feedback and disclosure animations honor the system reduced-motion
preference. The app's semantic light/dark tokens and interaction standards live in
`src/theme.json` and `../docs/desktop-design.md`.

| Page | Shared CLI/TUI capabilities |
| --- | --- |
| Overview | Update all or scheme/dictionary/model separately; installed component versions; remote status check; engine diagnostics; Wanxiang diagnosis; paths; one-time Rime integration; cancellable startup countdown |
| Schemes | All 15 schemas (including Jidian Wubi86 and Smart ABC Mod); shared model-patch setting; save selection before explicit installation |
| Appearance | Candidate size 1–9 and confirmed reset; Squirrel/Weasel builtin multiselection and default using one atomic core write; Linux Fcitx local library and theme market; per-repository download results and multiple variants; light/dark selectors and one-click Apply; optional rounding where supported |
| Settings | Platform-appropriate `Config` fields; Chinese/English labels; terminal-theme preference; user-data policy; mirror/threads; configured and environment proxy status; masked token; automatic setup/update; engine synchronization/link strategy; hooks; exclusion add/edit/remove/reset |
| Activity | Progress and bounded session history; shared structured JSON reports, including failure/cancellation; credential-redacted display and atomic native-dialog export; real zsh/fish completion exports |

The desktop follows the operating system's light/dark appearance and system
fonts. Unsupported engine skin settings are hidden. Linux shows Fcitx5 themes only when supported; macOS/Windows show their own engine presets.
Fcitx optional rounding appears only for themes whose embedded template supports
it; fixed-rounding themes do not pretend to offer a toggle.

## Architecture And Safety

- `src/ui.rs` is the actual reactive NSH root. It keeps unsaved drafts, bilingual
  labels, visible focus feedback, busy/error states, and explicit confirmations.
- `src/controller.rs` owns a dedicated worker thread, Tokio runtime, request/event
  channels, cancellation, cached appearance capabilities, and configuration
  snapshots. Network access, hooks, file operations, and deployment stay off the
  UI thread. Explicit refresh preserves the user's selected schema.
- Update dispatch, settings validation/rollback, model-patch synchronization,
  appearance routing, remote status, candidate validation, exclusion parsing,
  JSON schema, completions, and atomic writes come from the root library.
- Opening/navigating screens does not install, overwrite, deploy, or run hooks.
  An enabled startup update shows a cancellable countdown. Hooks and discard
  require review; settings must be saved or reverted before another mutation.
- Invalid drafts remain editable. Unreadable optional records/theme patches
  produce visible warnings without locking users out of settings. Conflicting
  commands are rejected while busy. Window unmount/controller drop requests
  cancellation without blocking the running native event loop. A process-level
  owner retains the worker's single join handle and waits after the native event
  loop returns, even if NSH has not dropped the root/controller yet. Explicit
  controller shutdown uses that same handle; there are no detached cleanup
  threads. Standalone controllers without a process owner join on drop.
- Shutdown is cooperative, not an abrupt abort of a copy/extraction/install step.
  The window can close before the process exits: an in-progress external hook
  or synchronous filesystem operation must return before the worker can reach a
  cancellation checkpoint. There is no timeout that abandons such work. A hung
  hook can therefore keep the process alive; hooks should not launch detached
  background jobs whose lifetime they do not wait for.
- NSH 0.4.0's published native runner sources return process ownership normally:
  Linux returns from GTK's application run; Windows joins its native UI/message
  loop thread; macOS intercepts `applicationShouldTerminate`, returns
  `TerminateCancel`, stops/wakes AppKit's event loop, and returns from `run_app`.
  The default macOS window-close path also stops the loop. The desktop does not
  rely on AppKit's process-exiting default `terminate:` implementation.
- Reports exclude configuration and redact configured token/proxy credentials.
  Export replaces files atomically through the shared helper and preserves
  existing symlinks. Native file dialogs determine the destination.

## Application Updates

- Official portable desktop packages check the stable GitHub Release's `latest.json`
  once on launch. Settings → Application updates also offers a manual check.
  A newer release shows its notes; **Update & restart** requires confirmation.
  This is separate from input-method/scheme auto-updates and never changes Rime data.
- The dedicated worker downloads over HTTPS, checks app identity, SemVer, archive
  size and SHA-256 with the standalone `nsh-updater` crate, then replaces the portable
  directory. Installation is serialized with other operations; cancellation interrupts
  checks/downloads, not the final atomic replacement. Restart waits for the native
  event loop and existing worker shutdown. Failed verification leaves the app intact.
- **AUR/pacman packages never check, download, install or restart themselves.**
  The PKGBUILD installs `/usr/share/snout-desktop/package-managed`, omits portable
  metadata, and installs under `/usr`, which is blocked even when running as root.
  Update `snout-desktop-bin` with your AUR helper instead. Other protected package
  locations, unknown/unpackaged binaries, read-only installs and previews are disabled too.
- Portable packages retain `nsh/app.json` alongside their executable directory
  (`Contents/Resources/nsh/app.json` on macOS). Keep the entire extracted directory;
  copying just the executable intentionally disables self-updates. Windows ZIPs now
  contain a single portable directory, not a loose executable.
- Future stable tag releases generate `latest.json` from all four desktop and five terminal payloads
  before `SHA256SUMS`; CI uploads the update manifest last. No manual manifest editing
  is needed. GitHub HTTPS is the trust root; these manifests are not independently signed.
  macOS packaging applies ad-hoc signing so installer validation can run; Developer ID
  signing/notarization is still needed for trusted distribution to other Macs.
- The UI/engine remains on NSH 0.4.0. The independent `nsh-updater` 0.5.0 has no
  UI/engine dependency and requires Rust 1.97 or newer to build the desktop.

## Verification

```sh
cargo fmt --manifest-path desktop/Cargo.toml --all -- --check
cargo clippy --manifest-path desktop/Cargo.toml --locked --all-targets --all-features -- -D warnings
cargo test --manifest-path desktop/Cargo.toml --locked
cargo test --manifest-path desktop/Cargo.toml --locked --no-default-features
cargo doc --manifest-path desktop/Cargo.toml --locked --no-deps
```

Controller tests and **actual NSH root** headless interaction tests use temporary
configurations and Rime directories, never `~/.config` or live Fcitx configuration.
They cover navigation/all schemas, confirmation/cancel, settings persistence and
invalid drafts, exclusion CRUD/error recovery, candidate apply/reset, countdown
and hook review, actual shared failure JSON export, completion export, credential
redaction, corrupt optional records, cancellation, and worker shutdown. A Linux
subprocess regression exercises the production controller with a real blocking
hook under a temporary HOME and isolated PATH, checks nonblocking UI close, and
proves the hook is finished and reaped before its parent process exits. It covers
controller drop before loop return, NSH retaining the root past loop return,
explicit shutdown followed by drop, and the process owner's fallback drop.

NSH's system-clock `Headless::press()` is not a render barrier. Interaction
tests wait for both the selected navigation item and mounted page content before
editing controls; a channel-gated regression covers delayed event dispatch.

Linux native compilation and headless tests can be verified on Linux. Windows
and macOS native compilation, platform-specific deployment, and live engine
updates require separate platform/session testing; headless tests are not a
substitute for those acceptance checks.
