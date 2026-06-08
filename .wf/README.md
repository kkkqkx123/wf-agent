# =============================================================================
# .wf/ directory — Project-level configuration
# =============================================================================
#
# This directory contains project-specific configuration that overrides
# global defaults. Files here are loaded on top of any global config
# found at default system locations.
#
# File layout:
#
#   .wf/
#   ├── mcp.json         # MCP server connections (project-specific)
#   ├── skills.json      # Skill paths (project-specific)
#   └── README.md        # this file
#
# Priority (lower number = higher priority):
#   1. CLI flags / SDK Options
#   2. .wf/ files (project-level)
#   3. Global config at system default location
#   4. Processor hardcoded defaults
#
# =============================================================================