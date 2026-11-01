# =============================================================================
# .wf/ directory — Project-level configuration
# =============================================================================
#
# This directory contains project-specific configuration that overrides
# shared defaults. Files here are loaded on top of the explicit global
# settings directory (if the host provides one) and the versioned
# `baselines/` directory.
#
# File layout:
#
#   .wf/
#   ├── mcp.json         # MCP server connections (project-specific, tracked)
#   ├── skills.json      # Skill paths (project-specific, tracked)
#   ├── config.toml      # Host knobs: storage, log level, tool approval (tracked)
#   ├── keymap-settings.toml  # Key bindings (tracked)
#   ├── meta/            # Volatile machine-local metadata (untracked, unscanned)
#   │   └── .gitignore   # auto-maintained: ignores everything inside
#   ├── tmp/             # Volatile spill output (untracked, unscanned)
#   │   └── .gitignore   # auto-maintained: ignores everything inside
#   └── README.md        # this file
#
# Only `.wf/meta/` and `.wf/tmp/` are volatile: they are excluded from
# version control (via their internal `.gitignore` files, never by editing
# the outer workspace `.gitignore`) and from file-checkpoint scans, commits
# and restores. Everything else directly under `.wf/` is synced project
# configuration and participates in checkpoint sampling like any other
# workspace file. The internal ignore files are created on demand and never
# force-overwritten once customized.
#
# Priority (lower number = higher priority):
#   1. CLI flags / SDK Options (incl. programmatic ConfigOverrides)
#   2. WF_* environment variables
#   3. .wf/ files (project-level) + explicit settings dir
#      (ambient workstation state joins only with an explicit opt-in;
#      services never read it)
#   4. baselines/ fixed files (project-owned deployment baselines)
#   5. Built-in code defaults
#
# =============================================================================