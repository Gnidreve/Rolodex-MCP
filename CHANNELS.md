# Idea: additional outbound channels besides email

Status: Telegram, ntfy.sh, Discord and Slack are implemented and merged
into `main` alongside email — all five channels listed in the table below
are live. The rest of this document is still open/brainstorming. Goal:
extend the existing principle of `Rolodex MCP` (one fixed, pre-configured
tool per contact, pure outbound send, no generic "send to arbitrary
address" tool) to further channels, not just email.

## Why this fits architecturally

The core principle of `Rolodex MCP` is channel-agnostic:

- The contact book (`config.toml`) defines **who** is reachable.
- Each channel gets **its own tool pair** (`send_via_<channel>` +
  `list_<channel>_contacts`), and `send_via_<channel>`'s `contact`
  parameter is a slug, resolved server-side against the live contact
  book.
- The agent never sees a raw address/ID — only slugs, and only ones the
  server already recognizes.
- Credentials (SMTP credentials so far) come separately from ENV, never
  from the contact book.

This transfers 1:1 to other channels, as long as the channel supports
**pure one-way outbound sending to a previously known address/ID** — so no
channel that needs a conversation/session handling.

## Open design question (before anything was built)

Two ways to extend the contact book:

1. **One channel per contact**, as before — `config.toml` gets a
   `channel` field per contact (`email` | `telegram` | `discord` | ...)
   plus the channel-specific address. The tool stays
   `send_message_to_<name>`, the backend routes internally to the right
   channel.
2. **Several channels per contact** — a contact can have e.g. both
   `email` and `telegram` set, which then gives each contact *and*
   channel its own tool (`send_email_to_max`, `send_telegram_to_max`).

**Option 2 was implemented** (see `src/channels/`): fits better with the
"explicit tool list" principle (the agent clearly sees which channel it's
sending via), `config.toml` uses flat, per-channel fields for that
instead of nested channel entries.

## Candidates

### Very good fit (simple API, no OAuth, pure outbound send)

| Status | Channel | Mechanism | Auth | Self-hostable? | Note |
|---|---|---|---|---|---|
| ✅ Implemented | **Telegram** | Bot API, `sendMessage` | Bot token (via BotFather) | No (Telegram servers) | Recipient must have started the bot once, then a fixed `chat_id`. Very stable, well-documented API, free. |
| ✅ Implemented | **Discord** | Incoming webhook (one URL per channel) | The webhook URL itself is the secret | No (Discord servers) | No bot setup needed, simple POST request. Fits "message into a channel" better than "to a person" though — 1:1 would instead need a real bot + DM channel. |
| ✅ Implemented | **Slack** | Incoming webhook or `chat.postMessage` (bot token) | Webhook URL or bot token | No (Slack servers) | Like Discord: webhook = channel, DMs to a person need the bot-token variant with a user ID. |
| ⬜ Open | **Pushover** | One POST (`user` key + `api_token` + message) | API token + user key | No | Built explicitly for exactly this use case ("send me a push notification"). Small one-time yearly price per user (no subscription). Very simple to integrate. |
| ✅ Implemented | **ntfy.sh** | POST to `https://ntfy.sh/<topic>` (or your own instance) | Optional (the topic is effectively the secret, or your own access token on a self-hosted instance) | **Yes** | Open source, no account needed for the public service. Fits well with this project's Coolify self-hosting approach. |
| ⬜ Open | **Gotify** | POST to your own Gotify instance with an app token | App token | **Yes** | Like ntfy, but you host the server entirely yourself (e.g. also via Coolify). A bit more operational overhead than ntfy. |

### Medium fit (works, but more effort/bureaucracy)

| Status | Channel | Mechanism | Auth | Note |
|---|---|---|---|---|
| ⬜ Open | **SMS via Twilio/Vonage/MessageBird** | REST API, one message per call | API key + account ID | Reaches anyone with a phone number, no app required on the recipient's end. Costs per message, a Twilio number must be rented. |
| ⬜ Open | **Matrix** | Client-server API, `PUT /rooms/{roomId}/send/...` | Access token of a bot account | Self-hostable (own homeserver), but each contact needs a shared room + its room ID up front. More setup effort than Telegram. |
| ⬜ Open | **Signal** | No official bot API — only via `signal-cli` (unofficial) or its REST wrapper (`signal-cli-rest-api`, Docker image) | Phone number registration of the bot account | Attractive for privacy-focused users, but technically the most complicated candidate here (own sidecar container, device verification). |
| ⬜ Open | **WhatsApp Business Cloud API** | Meta Graph API | Meta Business account + verification, access token | Reaches a huge number of people, but: Meta Business verification required, outside a 24h response window only pre-approved message templates may be used — impractical for spontaneous 1:1 messages. |
| ⬜ Open | **Microsoft Teams** | Power Automate HTTP trigger (incoming webhooks are being deprecated by Microsoft) | Workflow URL | In flux (Microsoft is deprecating the classic webhooks), API surface currently not stable enough for a recommendation. |

### Not a good fit for this pattern

- **Mastodon / Bluesky**: post publicly (broadcast), not addressed
  directly to a person — doesn't fit the "one tool per contact" model.
- Anything with an **OAuth user flow** (e.g. sending messages on behalf
  of a user via their own Google/Microsoft account): would need
  redirect-based authorization per contact — doesn't fit the existing
  "set ENV once, done" operating model.

## Generic fallback channel: webhook

Independent of individual platforms, a generic `send_webhook_to_<name>`
channel would be conceivable: a fixed URL + JSON payload per contact,
going to e.g. n8n, Zapier, IFTTT, Home Assistant, or any custom endpoint.
Implicitly covers a very wide range of platforms without needing code
written for each one individually — at the cost of the configuration then
requiring channel-specific knowledge from the user (which URL/payload
format the target service expects).

## Recommendation for a first step (if this gets built out further)

1. ✅ **Telegram** first — best compromise of reach, simple API, no cost,
   no server operation needed beyond our own.
2. ✅ **ntfy.sh** as a second channel — covers the "simple push
   notification to myself" case, built minimally invasively. Pushover
   remains open.
3. Already implemented from "everything else": ✅ Discord, ✅ Slack (both
   as the incoming-webhook variant). Still open: SMS, Matrix, Signal,
   WhatsApp, Gotify, Pushover — depending on concrete need, since each has
   its own trade-offs (broadcast instead of DM semantics, cost, setup
   effort, API stability).

## Not part of this idea

- No receiving/replying to messages (stays pure outbound sending, as
  with email too).
- No generic "send to arbitrary address" tools — the security principle
  (the agent only knows names, never raw addresses/IDs) applies to every
  new channel exactly as it does to email.
