# Security and privacy model

Rolodex MCP exists because of exactly one decision: **the agent can never
cause a message to go to an address, chat ID, or webhook URL it chose
itself.** Everything else in the project (plugin architecture, the
`contact` parameter, configuration separation) follows from that.

## The problem

A generic `send_message(channel, to, subject, body)` tool gives the agent
(or rather, the LLM generating the tool calls) full control over
**where** a message goes. That means:

- A prompt injection attack via a manipulated document, a web page, or
  another tool's response can get the agent to send a message to an
  arbitrary, attacker-controlled address — exfiltration of context,
  social engineering of the recipient, or simply spam in the user's name.
- The model's context window (tool schema, previous tool calls, logs)
  potentially contains real email addresses, phone numbers, or chat IDs
  of contacts — data that fundamentally has no business being in an LLM
  context if it can be avoided.

## The solution: an allow-listed slug, never a raw value

At startup, Rolodex MCP generates exactly one tool pair per configured
channel — `send_via_<channel>` and `list_<channel>_contacts` — from
`config.toml`. `send_via_<channel>` takes a `contact` parameter, but that
parameter is a **slug** (e.g. `max_mustermann`), never the actual
address/chat ID/webhook URL, and it is checked on every single call
against the live contact book loaded from `config.toml` — not against
whatever the client's cached tool schema happens to say. An unknown slug
is rejected outright; it is never substituted into the real address field
or passed to the channel in any form. `list_<channel>_contacts` lets the
agent discover the currently valid slugs without needing a fresh
`tools/list`.

Concretely, that means:

- A prompt injection attack can at most get the agent to call `send_via_*`
  with a slug that resolves to an *existing* contact already present in
  the contact book — not to an arbitrary, attacker-controlled address. The
  blast radius is reduced from "message to anyone on the internet" to
  "message to one of your own contacts".
- The actual addresses/IDs/webhook URLs only live in `config.toml` (the
  contact book) or `.env` (credentials) on the server — never in the
  model context, never in a tool response, never in the tool schema
  itself. The schema only ever contains slugs.

## Why this is not "just an alias"

Several existing multi-channel notification servers appear to solve the
problem similarly, via an **alias/recipient parameter** (a name instead
of a raw address). On the surface, that's what `contact` is too. The
decisive difference is in what happens when the value doesn't resolve:
most of these implementations fall back to **raw value passthrough** when
the alias is unknown — so the agent can still enter an arbitrary address
if it (or an attacker via prompt injection) wants to. In Rolodex MCP, an
unknown `contact` value is a hard, unconditional rejection. There is no
code path anywhere that takes a `contact` value the server didn't
recognize and uses it as a literal destination. It isn't a convention the
agent *should* follow — it is structurally impossible to send anywhere
the contact book doesn't already name.

This also means the JSON Schema for `contact` is deliberately a plain
`string`, not an `enum` of currently known slugs: an MCP client may
validate tool arguments against a cached schema before a call is even
sent, so an `enum` would block a contact added after the client's last
`tools/list` until it reconnects or refetches the schema. Making `contact`
a free string moves 100% of the real validation server-side, where it
always sees the current contact book (see "Live reload" below) — the
`enum` would have been a cosmetic convenience, not a security boundary,
and a stale one would actively get in the way.

## Two separations that serve this model

1. **Contact book (`config.toml`) vs. credentials (`.env`)**: who is
   reachable is kept separate from what is used to send. A compromised
   contact book (e.g. accidentally committed) doesn't expose SMTP
   passwords or bot tokens.
2. **Validation vs. connection test**: `ChannelDef::validate` only
   checks the format while parsing (a pure function, no network). Only
   `Channel::test_connection` at server start actually builds a real
   connection — a misconfigured channel surfaces at startup, not only
   on the first send attempt in the middle of operation.

## Live reload, and why it doesn't weaken this model

`config.toml` is polled for changes and reloaded without a server restart
(see `src/reload.rs`) — otherwise every edit to the contact book would
require an operator to restart the process, which doesn't scale once the
contact book grows. Two things are deliberately unaffected by this:

- The **set of tools** (which channels exist at all) is fixed for the
  process lifetime — it's derived once at startup from which channels
  have a working, connection-tested sender. A contact added for a channel
  that was never configured on this server is accepted into the contact
  book but ignored (with a warning in the log) until a real restart picks
  up the new channel's credentials.
- The **live contact lookup** that `send_via_<channel>` validates against
  updates immediately on reload. This is strictly a reduction of what the
  agent can reach as the contact book is edited — it can never cause an
  unvalidated value to be accepted, since validation always happens
  against whatever the current, just-reloaded contact book says.

## What this model does not cover

- It doesn't protect against an attacker who can already modify
  `config.toml` or `.env` on the server itself — that's already an
  infrastructure compromise, no longer an agent problem.
- It doesn't prevent the agent from sending a message that is
  *content-wise* harmful to a real contact (e.g. text manipulated via
  prompt injection) — only **where** is restricted, not **what**.
