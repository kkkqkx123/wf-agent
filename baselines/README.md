# baselines/ — versioned project baselines, not user configuration

This directory holds team-shared deployment baselines. Files here are
reviewed, versioned, and identical for every checkout of the project.
The name is deliberate: these are starting points the team owns, not a
place for personal or per-machine settings. Personal overrides live
elsewhere: per-project deltas in `.wf/`, per-human secrets and
preferences in explicitly provided settings directories
(workstation demo stand-in: `.wf-user/`, real: `--settings-dir`,
`~/.config/wf/`, `~/.wf/`).

Layout after the preset removal (no `index.json`, no selectable presets):

* `infrastructure/*.toml`: fixed filenames, one domain per file
  (`storage`, `metrics`, `output`, `sandbox`, `file-checkpoint`,
  `tool-approval`, `presets`, `tools`, `limits`). Loaded through
  `wf-config::orchestrator` with code `Default` as fallback per domain.
  A missing file boots with the built-in default; a present but invalid
  file fails startup. These files never contain secrets.
* `server/*.toml`: transport and middleware baselines (`server`, `auth`,
  `cors`, `rate-limit`). Loaded directly by `wf-server`. Secrets never
  live here: API keys come from the `API_KEYS` environment variable only.
* `mcp/mcp-settings.example.json`, `skills/skill-settings.example.json`:
  documentation samples. Copy to the explicit settings directory or the
  `.wf/` equivalent and edit; the examples themselves are never loaded.
* `workflows/`, `agent-loops/`, `node-templates/`, `prompt-templates/`:
  definition samples (`*.example.*`). Copy off the `.example` suffix to
  author real definitions; the samples document the schema only. Files
  whose names contain `.example.` are skipped by every loader, so samples
  can never leak into a running configuration.
* `scripts/`: reserved empty family for future script definitions.

What is deliberately not here:

* No per-user identity: LLM providers, profiles, auth declarations, and
  the credential store follow the operator, not the repo. Servers resolve
  them from the explicit settings directory and the project; ambient
  home-directory state is only consulted when the host opts in.
* No per-checkout deltas: project overrides (`mcp.json`, `skills.json`,
  `config.toml`, `keymap-settings.toml`) live in `.wf/`, volatile state
  in `.wf/meta/` and `.wf/tmp/`.
* No environment specifics: deploy-time differences use `WF_*`
  environment variables and programmatic `ConfigOverrides`, not file copies.

Precedence (highest first): CLI flags and programmatic options, `WF_*`
environment, `.wf/` project files plus the explicit settings directory,
`baselines/` fixed files, built-in code defaults.
