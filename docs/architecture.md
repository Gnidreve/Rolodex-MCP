# Architecture

## Overview

```
config.toml (contact book)  ──┐
                               ├──> config::load_contacts() ──> Vec<ChannelTool>
.env (credentials)       ─────┘                                       │
                                                                        ▼
channels::registry()  ──> Vec<ChannelDef> ──> main.rs builds the sender map
                                                       │
                                                       ▼
                                    SendMailServer (mcp_server.rs)
                                                       │
                                                       ▼
                                   axum router (lib.rs::build_router)
```

## The plugin system (`src/channels/`)

Each channel is its own file that implements the `Channel` trait:

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    async fn test_connection(&self) -> Result<()>;
    async fn send(&self, contact_name: &str, address: &str, subject: &str, body: &str) -> Result<()>;
}
```

On top of that comes `ChannelDef`: static metadata (config field name,
tool name segment, display name, validation function, ENV loader). A
channel registers itself with **one line** in `channels::registry()`.

`src/main.rs`, `src/config.rs` and `src/mcp_server.rs` never know a
concrete channel by name — they only iterate over `registry()`
generically. That's deliberate: a new channel should be addable without
touching any of the three files (see [CONTRIBUTING.md](../CONTRIBUTING.md)
for the branch-per-channel rule that builds exactly on this).

## lib.rs vs. main.rs

The actual server logic (loading channels, building the router) lives in
`src/lib.rs` as `pub fn build_router(...)`. `src/main.rs` is now only the
thin entry point (read ENV, set up logging, call `build_router`, bind).
This separation exists so `tests/` can run real integration tests against
the public API, instead of only embedded unit tests per module — see
`tests/api.rs` for an example that checks the complete MCP roundtrip
(healthcheck, auth, `tools/list`, `tools/call`) against a mock channel,
entirely without a real TCP bind.

## Startup and request lifecycle

1. `config::load_contacts()` parses `config.toml`, validates every
   channel value purely formally (`ChannelDef::validate`, no network),
   and produces a `ChannelTool` (tool name, title, address, channel slug)
   per contact-channel combination.
2. `main.rs` loads the ENV configuration (`ChannelDef::from_env`) for
   every channel actually in use, and checks the connection once for
   real (`Channel::test_connection`). If that fails, the process doesn't
   start at all.
3. `build_router` builds the MCP server (`SendMailServer`) and the axum
   router from that: `GET /` unprotected healthcheck, `POST /` the
   actual MCP endpoint behind `Authorization: Bearer <MCP_BEARER_TOKEN>`
   (`src/auth.rs`).
4. On a `tools/call`, `SendMailServer::call_tool` looks up the tool by
   its tool name, looks up the matching `Box<dyn Channel>` in the sender
   map, and calls `Channel::send`. The error case is returned as
   `isError: true` in the MCP result, not as a protocol error — the
   agent gets a readable error message instead of a broken call.

## Why `Box<dyn Channel>` instead of generics

The number and kind of channels is only known at runtime (depending on
`config.toml`), and `main.rs` has to handle a heterogeneous collection of
`SmtpConfig`, `TelegramConfig`, `NtfyConfig`, etc. at the same time,
without knowing them by name. Dynamic dispatch via `dyn Channel` isn't a
style compromise here, it's the only option that fits the plugin
principle — a new channel brings its dyn-compatibility along by itself,
without the calling side needing to be adjusted.

## Further reading

- [security-model.md](security-model.md) — why there's no address
  parameter at all.
- [deployment.md](deployment.md) — self-hosting with Docker/Coolify.
- [../CHANNELS.md](../CHANNELS.md) — which channels exist, or are still
  an open idea.
- [../CONTRIBUTING.md](../CONTRIBUTING.md) — how a new channel is
  contributed.
