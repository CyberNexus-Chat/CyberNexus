# Codex Integration

This directory holds the **assets** EchoBird's Rust-native Codex proxy uses at runtime: the global config (`config.json`) and the ChatGPT (formerly Codex Desktop) launch metadata (`paths.json`).

The proxy itself — Responses ↔ Chat Completions translation, model-id rewrite, session history, onboarding bypass, binary resolution, and process spawn — lives entirely inside the Rust source tree under `src-tauri/src/services/codex_proxy/`. No Node.js, no `.cjs` files, no external process.

## How it works

```
                                                                   
   Codex CLI       Responses API    EchoBird Rust    Chat Completions   LLM Provider 
                 ─────────────►     (codex_proxy)   ───────────────►   (DeepSeek/    
   ChatGPT desktop                   127.0.0.1:53682                     Moonshot/etc)
                 ◄─────────────                     ◄───────────────                 
                  Responses SSE                       Chat SSE                       
```

1. **`apply_codex`** (in `src-tauri/src/services/tool_config_manager/codex.rs`) writes the canonical 12-line `~/.codex/config.toml` so Codex sees `base_url = "http://127.0.0.1:53682/v1"` and `wire_api = "responses"`.
2. **`spawn_proxy_task`** binds `127.0.0.1:53682` on Tauri startup and serves `POST /v1/responses`. The handler reads `~/.echobird/codex.json` fresh on every request to pick up model / API key / upstream URL switches without restarting anything.
3. **Translation**: incoming Responses-shape request becomes a Chat Completions request, gets forwarded to the user's provider, and the streaming response is translated back to Responses-shape SSE on the way out.
4. **Smart spoof**: Codex's `gpt-5.4` model id is rewritten to the real upstream model id before forwarding, then mirrored back on the response so Codex's bookkeeping stays consistent.

## Files in this directory

- `config.json` — global toggles read by `tool_config_manager`
- `paths.json` — ChatGPT desktop install hints (Programs paths, Microsoft Store AUMID for `shell:AppsFolder`)
- `README.md` — this file

## Why not Node?

Earlier versions shipped a Node-based `codex-launcher.cjs` that ran the proxy as a child process. As of v5.0 the proxy is in Rust and runs inside the Tauri binary, so:

- End users no longer need Node.js installed
- The translation dictionary compiles to machine code instead of plaintext `.cjs`
- SSE forwarding has less buffering (no Node shim layer between sockets)
- One implementation to maintain instead of two

## Source map

| Concern                       | Where it lives                                                |
| ----------------------------- | ------------------------------------------------------------- |
| HTTP server + handler         | `src-tauri/src/services/codex_proxy/server.rs`                |
| Responses → Chat translation  | `src-tauri/src/services/codex_proxy/protocol_converter.rs`    |
| Chat SSE → Responses SSE      | `src-tauri/src/services/codex_proxy/stream_handler.rs`        |
| Session history + reasoning   | `src-tauri/src/services/codex_proxy/session_store.rs`         |
| Multimodal content mapping    | `src-tauri/src/services/codex_proxy/content_mapper.rs`        |
| Relay / config.toml           | `src-tauri/src/services/codex_proxy/config_manager.rs`        |
| Onboarding skip               | `src-tauri/src/services/codex_proxy/onboarding_bypass.rs`     |
| Binary resolution             | `src-tauri/src/services/codex_proxy/codex_binary.rs`          |
| Spawn (CLI + Desktop)         | `src-tauri/src/services/process_manager.rs` (`start_codex_*`) |

## Troubleshooting

### "No active model configured in EchoBird"

The proxy returned a 503 because `~/.echobird/codex.json` is missing or incomplete. Open EchoBird, pick a model, and click **Apply to Codex**.

### Codex returns 401 / 403 from the upstream

The provider rejected your credentials. Re-check the API key in EchoBird's model settings and re-apply.

### Codex CLI shows mojibake or TUI degrades

EchoBird couldn't locate the npm-bundled native Codex binary and fell back to the `codex.cmd` shim. Make sure `@openai/codex` is installed globally:

```bash
npm i -g @openai/codex
```

### ChatGPT (desktop) won't launch

Install ChatGPT from <https://openai.com/codex> or the Microsoft Store. EchoBird looks for it at the standard install locations and falls back to the Store activation URI in `paths.json`.

## Related

- `src-tauri/src/services/tool_config_manager/codex.rs` — writes the canonical `~/.codex/config.toml`
- `src-tauri/src/services/process_manager.rs` — `start_codex_native` spawns Codex CLI / Desktop after the pre-flight checks
