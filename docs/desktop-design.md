# Desktop design system

## Product and platforms

Snout is a native Rime maintenance workbench for Linux, macOS and Windows. The
main task is understanding the installed scheme and updating its components.
Keep the NSH 0.4.0 Rust backend, system window chrome, fonts, scrolling, text
input and file dialogs. Desktop work does not change the CLI/TUI presentation.

## Visual language

- A persistent 208 point sidebar groups the five existing destinations. Selection
  uses a filled rounded surface, matching text and vector icon, and the accessible
  selected state. No floating navigation or custom window chrome.
- The page header identifies the current destination and keeps Update all in a
  stable position. Overview emphasizes the current scheme, then installed
  components, then engine integration. Diagnostic details are disclosed on demand.
- `desktop/src/theme.json` owns every light/dark color. Rust reads `theme()`;
  component part classes use the same semantic tokens. Blue denotes actions and
  selection; destructive actions and errors use the destructive role.
- System typography: 28 point scheme identity, 22 page title, 20 section heading,
  16 section label, 14 control/body, 12–13 supporting text. Spacing uses the NSH
  4 point scale: 8 within controls, 12 between peers, 16 within sections, 24
  between regions. Corners use 6–8 for controls and 10–12 for containing surfaces.
- Wrap component and scheme cards as the window narrows. The content scrolls;
  navigation, title, unsaved changes and operation status stay available.

## Controls and interaction

- Use NSH semantic buttons, inputs, selects, switches, progress and modal dialog.
  App helpers style their parts, disabled, focus, active and selected states.
- Keep drafts separate from persisted configuration. Show a fixed save/revert
  strip across destinations when the draft changes. Revert restores the loaded
  configuration. Saving uses the existing validation and explicit confirmation.
- Confirm consequential actions in a modal layer, with Escape/cancel dismissal.
  Preserve data policy, hook warnings and the explicit destructive alternative.
- Candidate preview illustrates only count/layout. It does not claim to reproduce
  the installed input engine's skin or deploy changes before Apply.
- Keyboard: Tab/Shift+Tab and Enter/Space through native controls, Mod+1…5 for
  destinations, Mod+S to review/save. Mod is Command on macOS and Control elsewhere.

## Motion

Use 120–150 ms color/opacity transitions for control states, selection and switch
thumb changes. Reveal draft and diagnostic content with a 140 ms opacity change.
No route choreography, bouncing, artificial waits, sounds or ambient animation.
Native NSH transitions and `Motion::play` honor the OS reduced motion setting;
the final state is always understandable without motion. Input stays enabled
through transitions, subject only to the real busy/confirmation state.

## Verification

Exercise real NSH headless behavior, both languages, theme changes, reduced motion,
keyboard paths, narrower windows, draft persistence and confirmation cancellation.
Inspect screenshots from the native Linux renderer. Cross-platform CI proves
compilation and headless behavior on Windows/macOS; it does not substitute for
real platform visual or input-method testing.

## Platform theme catalog

- Hide engine skin panels and the patch-key setting when the current environment
  has no corresponding appearance target.
- Fcitx5 uses one filterable virtual list. Built-in themes and downloaded variants
  carry source tags; online repositories show both topic tags when applicable.
- Merge topic results by GitHub repository identity. Fetch/import does not apply
  a theme. Apply the chosen light/dark pair explicitly, with visible selected
  labels, optional rounding only where supported, and standard confirmation.
