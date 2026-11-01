# =============================================================================
# .wf/ directory — Project-level configuration
# =============================================================================
#
# This directory contains project-specific configuration that overrides
# shared defaults. Files here are loaded on top of the explicit global
# settings directory (if the host provides one). There is no third
# versioned baseline directory: anything not set here or in the settings
# directory falls back to built-in code defaults.
#
# Production location: the `.wf/` directory of the business repository the
# server is started against (the `--config` project root). It travels with
# the business project, never with the release artifact.
#
# File layout:
#
#   .wf/
#   ├── mcp.json         # MCP server connections (project-specific, tracked)
#   ├── skills.json      # Skill paths (project-specific, tracked)
#   ├── config.toml      # Host knobs: storage, log level, tool approval (tracked)
#   ├── keymap-settings.toml  # Key bindings (tracked)
#   ├── infrastructure/  # Per-domain runtime overrides, one file per domain
#   │                    # (storage, metrics, output, sandbox, file-checkpoint,
#   │                    #  tool-approval, presets, tools, limits; tracked)
#   ├── server/          # Server transport + middleware overrides
#   │                    # (server, auth, cors, rate-limit; tracked)
#   ├── auth.json        # Credential store placed here by tenant-isolated
#   │                    # services (untracked); workstation CLIs keep the
#   │                    # home-directory default instead
#   ├── meta/            # Volatile machine-local metadata (untracked, unscanned)
#   │   └── .gitignore   # auto-maintained: ignores everything inside
#   ├── tmp/             # Volatile spill output (untracked, unscanned)
#   │   └── .gitignore   # auto-maintained: ignores everything inside
#   └── README.md        # this file
#
# Only `.wf/meta/`, `.wf/tmp/` (and the service-placed `auth.json`) are
# volatile: they are excluded from version control (via their internal
# `.gitignore` files, never by editing the outer workspace `.gitignore`)
# and from file-checkpoint scans, commits and restores. Everything else
# directly under `.wf/` is synced project configuration and participates
# in checkpoint sampling like any other workspace file. The internal ignore
# files are created on demand and never force-overwritten once customized.
#
# Priority (lower number = higher priority):
#   1. CLI flags / SDK Options (incl. programmatic ConfigOverrides)
#   2. WF_* environment variables
#   3. Explicit managed directory (ops-rendered, e.g. from `deploy/`;
#      only when the host passes one)
#   4. .wf/ files (project-level, wins over the settings dir)
#   5. Explicit settings dir (ambient workstation state joins only with
#      an explicit opt-in; services never read it)
#   6. Built-in code defaults
#
# Templates for every file above live in `config-examples/`; production
# values are rendered by the `deploy/` mechanism, never copied verbatim
# from samples.
#
# =============================================================================
