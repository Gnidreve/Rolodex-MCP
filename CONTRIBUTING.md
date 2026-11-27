# Contributing — channel branches (hard project rule)

Every outbound channel (email, Telegram, ntfy.sh, ...) is developed **on
its own git branch**, named after the channel (e.g. `telegram`, `ntfy`,
`whatsapp`). This rule applies permanently to this project from now on.

## Workflow per channel

1. Branch off from `main`, named after the channel (lowercase, e.g.
   `git checkout -b ntfy`).
2. Implementation exclusively in a **new file** `src/channels/<channel>.rs`
   that implements the `Channel` trait from `src/channels/mod.rs` (see
   `src/channels/email.rs` as a reference).
3. Add exactly **one line** in `src/channels/mod.rs::registry()` to
   register the new channel. That's the only place outside the new file
   that may be touched.
4. **Do not touch** `main.rs`, `lib.rs`, `config.rs` and `mcp_server.rs` —
   they only know the `Channel` trait and `ChannelDef`, never a concrete
   backend. If a channel doesn't fit that (e.g. because it fundamentally
   needs different metadata), that's a sign the wrapper itself needs to
   be extended — that then runs as its own discussion, not incidentally
   in a channel PR.
5. Open a pull request to `main`. **Don't merge it yourself** — merges
   (and their order, if several channel PRs are open at the same time)
   are decided by the project owner.

## Exception: changes to the wrapper itself

The `Channel` trait, `ChannelDef` and the generic logic in
`main.rs`/`lib.rs`/`config.rs`/`mcp_server.rs` are the exception to the
branch-per-channel rule and are developed and pushed **directly to
`main`**. This should only rarely be needed after this architecture's
initial introduction — if it is, those are architectural decisions
handled separately from the actual channel feature.

## Why

This lets several channels be developed in parallel and independently of
each other, without their branches blocking one another via merge
conflicts in shared files — every channel PR only touches its own new
file plus one line in the registry.
