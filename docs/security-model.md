# Security and privacy model

Rolodex MCP exists because of exactly one decision: **the agent never
gets to see a raw address, chat ID, or webhook URL as a parameter.**
Everything else in the project (plugin architecture, tool naming scheme,
configuration separation) follows from that.

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

## The solution: tool identity instead of tool parameter

At startup, Rolodex MCP generates a separate, fixed-named tool for every
contact-channel combination from `config.toml`:
`send_to_<name>_via_<channel>`. There is **no parameter field for
address, chat ID, or channel** — who is meant is encoded exclusively in
the tool name, which the MCP host presents to the model as part of the
tool schema. The tool itself only accepts `subject` and `body`.

Concretely, that means:

- A prompt injection attack can at most get the agent to call an
  *existing* tool (i.e. send to one of the contacts already present in
  the contact book) — not to an arbitrary, attacker-controlled address.
  The blast radius is reduced from "message to anyone on the internet" to
  "message to one of your own contacts".
- The actual addresses/IDs/webhook URLs only live in `config.toml` (the
  contact book) or `.env` (credentials) on the server — never in the
  model context, never in a tool response, never in the tool schema
  itself.

## Why this is not "just an alias"

Several existing multi-channel notification servers appear to solve the
problem similarly, via **alias parameters** (a name instead of a raw
address). The decisive difference: with an alias parameter, a
`recipient` field still exists in the tool schema, and most of these
implementations fall back to **raw value passthrough** when the alias is
unknown — so the agent can still enter an arbitrary address if it (or an
attacker via prompt injection) wants to. In Rolodex MCP, that field
simply doesn't exist. It isn't a convention the agent *should* follow —
it is structurally impossible to break.

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

## What this model does not cover

- It doesn't protect against an attacker who can already modify
  `config.toml` or `.env` on the server itself — that's already an
  infrastructure compromise, no longer an agent problem.
- It doesn't prevent the agent from sending a message that is
  *content-wise* harmful to a real contact (e.g. text manipulated via
  prompt injection) — only **where** is restricted, not **what**.
