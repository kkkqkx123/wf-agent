#!/usr/bin/env python3
"""
Clean build artifacts (.turbo, node_modules, dist) from the project,
excluding .pnpm-store to preserve download cache.

Usage:
    python scripts/clean-build-artifacts.py [--dry-run]
"""

import os
import shutil
import sys
from pathlib import Path

EXCLUDED_DIRS = {".pnpm-store"}
TARGET_DIRS = {".turbo", "node_modules", "dist"}

def find_and_clean(root: Path, dry_run: bool = False):
    removed_count = 0
    removed_size = 0

    for dirpath, dirnames, _ in os.walk(root):
        # Skip excluded directories entirely
        dirnames[:] = [d for d in dirnames if d not in EXCLUDED_DIRS]

        current = Path(dirpath)
        for d in list(dirnames):
            if d in TARGET_DIRS:
                target = current / d
                if target.is_dir():
                    # Calculate size
                    size = 0
                    for subdirpath, subdirnames, filenames in os.walk(target):
                        for f in filenames:
                            fp = os.path.join(subdirpath, f)
                            try:
                                size += os.path.getsize(fp)
                            except OSError:
                                pass

                    if dry_run:
                        print(f"[DRY-RUN] Would remove: {target} ({_format_size(size)})")
                    else:
                        print(f"Removing: {target} ({_format_size(size)})")
                        shutil.rmtree(target, ignore_errors=True)

                    removed_count += 1
                    removed_size += size
                    dirnames.remove(d)  # don't recurse into deleted dirs

    return removed_count, removed_size


def _format_size(bytes_sz: int) -> str:
    if bytes_sz >= 1024 ** 3:
        return f"{bytes_sz / 1024 ** 3:.2f} GB"
    elif bytes_sz >= 1024 ** 2:
        return f"{bytes_sz / 1024 ** 2:.2f} MB"
    elif bytes_sz >= 1024:
        return f"{bytes_sz / 1024:.1f} KB"
    return f"{bytes_sz} B"


if __name__ == "__main__":
    dry_run = "--dry-run" in sys.argv
    root = Path(__file__).resolve().parent.parent  # project root

    print(f"Scanning {root} for {', '.join(TARGET_DIRS)} directories...")
    print(f"Excluding: {', '.join(EXCLUDED_DIRS)}")
    if dry_run:
        print("Mode: DRY RUN (no files will be deleted)\n")
    else:
        print("")

    count, size = find_and_clean(root, dry_run)

    print(f"\n{'Would remove' if dry_run else 'Removed'} {count} directories, "
          f"freed {_format_size(size)}")