# Oh My Pi in EchoBird

OMP is available as a **CLI Code** tool card, with smart installation, install detection, terminal launch, and model selection. The icon is copied from Coffee-CLI's `src-ui/src/icons-inline/omp.svg`.

## Install and launch

Use the card's smart-install action, then launch `omp` from a project directory. The bundled and public installation reference is [omp.json](../../docs/api/tools/install/omp.json).

- Windows standalone: `%LOCALAPPDATA%\omp\omp.exe`.
- macOS/Linux standalone: `~/.local/bin/omp`.
- Bun installs: `~/.bun/bin`; macOS/Linux also support the official Homebrew tap.
- The package is `@oh-my-pi/pi-coding-agent`, not the upstream Pi package. Bun installs currently require Bun 1.3.14+; standalone binaries do not need Node or Bun.

## Model configuration

This integration manages the default profile at `~/.omp/agent`. Named profiles, environment overrides, and project-level model overrides remain managed by OMP.

| File | EchoBird-owned configuration |
| --- | --- |
| `models.yml` or existing `models.yaml` | `providers.echobird`, including endpoint, credentials, API protocol, and selected model |
| `config.yml` or existing `config.yaml` | `modelRoles.default: echobird/<model-id>` |

OpenAI uses `openai-completions`; Anthropic uses `anthropic-messages` and the selected model's Anthropic endpoint. Keyless endpoints use `auth: none`.

Other providers and settings are retained. Switching models also updates roles pointing to the EchoBird provider, preserving their thinking-level suffixes. The backend restore handler removes the EchoBird provider and role selectors pointing to it, preserving selections for other providers. OMP has no canonical vendor endpoint, so the UI does not show an official-endpoint card. Restore does not delete auth files or sessions. Invalid YAML is reported before either config file is modified. If only legacy JSON or database settings exist, launch OMP once so its own migration runs before applying a model.

OMP and Pi have separate configuration directories and formats; this adapter does not call the Pi adapter. YAML writes preserve values, but reserialize formatting and comments.

## Verification and sources

- Rust tests cover config round trips, model and protocol switches, keyless endpoints, other-provider preservation, restore behavior, invalid input, YAML filename precedence, and legacy JSON/database protection.
- OMP 17.2.12 on Windows was exercised with a temporary home/config directory and a local mock HTTP server. Its configured default model completed both Chat Completions and Anthropic Messages requests. No live provider credentials or paid inference were used.
- macOS/Linux installation and terminal UI interactions still require checks on those systems.
- [Official repository and installation](https://github.com/can1357/oh-my-pi)
- [Model schema](https://github.com/can1357/oh-my-pi/blob/main/docs/models.md)
- [Config precedence and migration](https://github.com/can1357/oh-my-pi/blob/main/docs/config-usage.md)
- [Official PowerShell installer source](https://github.com/can1357/oh-my-pi/blob/main/scripts/install.ps1)
