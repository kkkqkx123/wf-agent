#!/usr/bin/env python3
"""Sync the simvec submodule package version with the root workspace version.

The root ``Cargo.toml`` workspace version is the single source of truth.
The simvec submodule is an independent repository and cannot inherit the
workspace version, so its package version is a separate field that drifts.
This script reads the root workspace version, compares it with the version
in ``crates/simvec/Cargo.toml``, and (unless ``--check``) rewrites the
submodule version to match, reporting the change.

Only the version field is touched: dependencies and lockfiles are never
modified and versions are never bumped automatically — promoting a version
remains a human decision; this script only enforces consistency after that
decision is made.

Usage:
    scripts/sync_simvec_version.py            # sync (rewrite if drifted)
    scripts/sync_simvec_version.py --check    # check only, exit 1 on drift

Vendorized third-party submodules (e.g. tantivy) are skipped: their version
belongs to the upstream project and is intentionally not synchronized.
"""

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ROOT_MANIFEST = ROOT / "Cargo.toml"
SIMVEC_MANIFEST = ROOT / "crates" / "simvec" / "Cargo.toml"


def read_workspace_version() -> str:
    text = ROOT_MANIFEST.read_text(encoding="utf-8")
    section = re.search(r"\[workspace\.package\](.*?)(?:\n\[|\Z)", text, re.S)
    if not section:
        sys.exit(f"error: no [workspace.package] section in {ROOT_MANIFEST}")
    match = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M)
    if not match:
        sys.exit(f"error: no version field in [workspace.package]")
    return match.group(1)


def read_simvec_version() -> str:
    text = SIMVEC_MANIFEST.read_text(encoding="utf-8")
    section = re.search(r"\[package\](.*?)(?:\n\[|\Z)", text, re.S)
    if not section:
        sys.exit(f"error: no [package] section in {SIMVEC_MANIFEST}")
    match = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1), re.M)
    if not match:
        sys.exit(f"error: no version field in [package]")
    return match.group(1)


def write_simvec_version(version: str) -> None:
    text = SIMVEC_MANIFEST.read_text(encoding="utf-8")
    section = re.search(r"\[package\](.*?)(?:\n\[|\Z)", text, re.S)
    if not section:
        sys.exit(f"error: no [package] section in {SIMVEC_MANIFEST}")
    match = re.search(r'(?P<prefix>version\s*=\s*")(?P<value>[^"]+)(?=")',
                      section.group(1))
    if not match:
        sys.exit(f"error: no version field in [package]")
    new_section = section.group(1)[:match.start("value")] + version + \
        section.group(1)[match.end("value"):]
    new_text = text[:section.start(1)] + new_section + text[section.end(1):]
    SIMVEC_MANIFEST.write_text(new_text, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="only verify consistency; exit 1 on drift without rewriting",
    )
    args = parser.parse_args()

    workspace_version = read_workspace_version()
    simvec_version = read_simvec_version()

    if workspace_version == simvec_version:
        print(f"ok: simvec {simvec_version} matches workspace {workspace_version}")
        return 0

    if args.check:
        print(
            f"error: simvec version {simvec_version} != workspace version "
            f"{workspace_version}"
        )
        return 1

    write_simvec_version(workspace_version)
    print(f"updated simvec version: {simvec_version} -> {workspace_version}")
    print("note: the submodule pointer now needs committing in the main repo")
    return 0


if __name__ == "__main__":
    sys.exit(main())

