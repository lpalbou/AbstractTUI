#!/usr/bin/env python3
"""Compatibility entrypoint for AbstractAvatar's standalone artifact builder."""
from pathlib import Path
import argparse
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--check", action="store_true")
parser.add_argument("--ui-kit", type=Path, default=root.parent / "abstractuic/ui-kit")
args = parser.parse_args()
builder = root.parent / "abstractavatar/scripts/build_artifacts.py"
if not builder.is_file():
    parser.error("Clone AbstractAvatar beside AbstractTUI to build or verify shared artifacts.")
command = [sys.executable, str(builder), "--uic-dir", str(args.ui_kit / "src/avatars")]
if args.check:
    command.append("--check")
raise SystemExit(subprocess.call(command))
