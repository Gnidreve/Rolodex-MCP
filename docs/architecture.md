# Architecture

## Overview

```
config.toml (contact book)  ──┐
                               ├──> config::load_contacts() ──> Vec<ContactEntry>
.env (credentials)       ─────┘           │                           │
                                           ▼                           ▼
                             config::group_by_channel()   channels::registry() ──> main.rs builds
                                           │                                        the sender map
                                           ▼                                              │
                          Arc<RwLock<HashMap<channel, Vec<ContactEntry>>>> <── reload::spawn()
                                           │                                              │
                                           └──────────────────┬───────────────────────────┘
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
   and produces a `ContactEntry` (contact name, contact slug, address,
   channel slug) per contact-channel combination. `config::group_by_channel`
   turns that flat list into the shape the server needs: contacts grouped
   by channel.
2. `main.rs` loads the ENV configuration (`ChannelDef::from_env`) for
   every channel actually in use, and checks the connection once for
   real (`Channel::test_connection`). If that fails, the process doesn't
   start at all. The channels that pass this become the fixed
   `ActiveChannel` list for the process lifetime — see
   `mcp_server::ActiveChannel`.
3. The grouped contacts are wrapped in `Arc<RwLock<...>>` and handed both
   to `build_router` and to `reload::spawn`, which polls `config.toml`'s
   mtime and swaps in a freshly parsed contact book whenever it changes —
   no restart needed to pick up a new or removed contact (see
   [security-model.md](security-model.md#live-reload-and-why-it-doesnt-weaken-this-model)).
4. `build_router` builds the MCP server (`SendMailServer`) and the axum
   router from that: `GET /` unprotected healthcheck, `POST /` the
   actual MCP endpoint behind `Authorization: Bearer <MCP_BEARER_TOKEN>`
   (`src/auth.rs`). `SendMailServer::new` builds exactly two tools per
   active channel — `send_via_<channel>` and `list_<channel>_contacts` —
   once; this set never changes afterward, only the contact data behind
   it does.
5. On a `tools/call`, `SendMailServer::call_tool` looks up the tool name
   in a precomputed dispatch map to find the channel and whether it's a
   `send` or `list` call. For `send_via_<channel>`, the `contact` slug is
   looked up in the live contact book under a read lock; an unknown slug
   is rejected (`invalid_params`), a known one resolves to the real
   address and is handed to `Channel::send`. A send failure is returned
   as `isError: true` in the MCP result, not as a protocol error — the
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
