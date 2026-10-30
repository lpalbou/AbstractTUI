#!/usr/bin/env python3
"""Aggregate the supported user documentation; source pages remain canonical."""
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
FILES = ["README.md", "docs/getting-started.md", "docs/architecture.md", "docs/api.md",
         "docs/theming.md", "docs/graphics-and-3d.md", "docs/graphs-and-diagrams.md",
         "docs/live-data.md", "docs/faq.md", "docs/troubleshooting.md",
         "skills/abstracttui/SKILL.md"]
header = "# AbstractTUI documentation\n\nExpanded companion to llms.txt. The source pages below are canonical.\n"
(ROOT / "llms-full.txt").write_text(header + "".join(
    f"\n===== FILE: {name} =====\n\n{(ROOT / name).read_text().rstrip()}\n" for name in FILES))
