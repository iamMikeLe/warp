# Warp UI tweaks

Small quality-of-life changes to [Warp](https://github.com/warpdotdev/warp), kept on their own
branch (`ui-tweaks`, based directly on upstream) so each could be proposed upstream separately.
This is an unofficial fork, not affiliated with Warp.

- **Alternating block backgrounds:** every other terminal block gets a subtle tint, so it's
  easy to see where each command or session starts and ends (Settings → Appearance → Blocks).
- **Easier pane resizing:** split-pane dividers have a wider grab area and highlight on hover;
  side panel edges highlight too.
- **Colored file and folder icons** from
  [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme)
  (MIT; license in `app/assets/bundled/svg/material/LICENSE`) in the Project Explorer, file
  search, tabs and the Command Palette. Turn them off in Settings → Appearance. Regenerate them
  with `script/update_material_icons.py`.
- **Rainbow indent:** code editors and diffs tint each indentation level in a different faint
  color, following the file's indent unit (Settings → Code, or "rainbow indent" in the Command
  Palette).

Each change has a setting and a Command Palette toggle, and is on by default.
