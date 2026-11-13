#!/usr/bin/env python3
"""Vendor Material Icon Theme (MIT, https://github.com/material-extensions/vscode-material-icon-theme)
into the app's bundled assets for the Project Explorer's file and folder icons.

Usage:
    npm pack material-icon-theme@<version>      # in an empty scratch directory
    tar -xzf material-icon-theme-<version>.tgz
    python3 -I script/update_material_icons.py <scratch>/package

Writes:
    app/assets/bundled/svg/material/*.svg   only the icons the manifest references
    app/assets/bundled/svg/material/LICENSE the theme's MIT license (required to redistribute)
    app/assets/bundled/material_icons.json  the name -> icon stem mappings the app looks up
"""

import json
import shutil
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
ICON_DIR = REPO / "app/assets/bundled/svg/material"
MANIFEST_OUT = REPO / "app/assets/bundled/material_icons.json"
MAPPING_KEYS = ["fileExtensions", "fileNames", "folderNames", "folderNamesExpanded"]


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    package = Path(sys.argv[1])
    manifest = json.loads((package / "dist/material-icons.json").read_text())
    version = json.loads((package / "package.json").read_text())["version"]

    # Icon definition name -> SVG file stem (several definitions can share a file).
    stems = {
        name: Path(definition["iconPath"]).stem
        for name, definition in manifest["iconDefinitions"].items()
    }

    def resolve(mapping: dict) -> dict:
        return {key.lower(): stems[icon] for key, icon in mapping.items() if icon in stems}

    out = {
        "version": version,
        "file": stems[manifest["file"]],
        "folder": stems[manifest["folder"]],
        "folderExpanded": stems[manifest["folderExpanded"]],
        **{key: resolve(manifest[key]) for key in MAPPING_KEYS},
        "light": {key: resolve(manifest["light"].get(key, {})) for key in MAPPING_KEYS},
    }

    used = {out["file"], out["folder"], out["folderExpanded"]}
    for mapping in [*(out[k] for k in MAPPING_KEYS), *out["light"].values()]:
        used.update(mapping.values())

    if ICON_DIR.exists():
        shutil.rmtree(ICON_DIR)
    ICON_DIR.mkdir(parents=True)
    for stem in sorted(used):
        shutil.copyfile(package / "icons" / f"{stem}.svg", ICON_DIR / f"{stem}.svg")
    shutil.copyfile(package / "LICENSE", ICON_DIR / "LICENSE")
    MANIFEST_OUT.write_text(json.dumps(out, separators=(",", ":"), sort_keys=True))
    print(f"Material Icon Theme {version}: {len(used)} icons, mapping {MANIFEST_OUT.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
