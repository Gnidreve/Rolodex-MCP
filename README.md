# Rolodex MCP

[![Test](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/test.yml/badge.svg)](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/test.yml)
[![Release](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/release.yml/badge.svg)](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/release.yml)
[![License: BUSL 1.1](https://img.shields.io/badge/License-BUSL--1.1-blue.svg)](LICENSE)

An MCP server that generates exactly two tools per configured channel
(Email, ntfy.sh, Telegram, Discord, Slack) from `config.toml`:
`send_via_<channel>` and `list_<channel>_contacts`. The agent addresses a
contact by a slug, validated server-side against the live contact book on
every call — there is no generic tool with a free-form address or channel
choice, and an unknown contact is always rejected, never used as a literal
address. Why that is, see [docs/security-model.md](docs/security-model.md).

New channels are developed on their own git branches and merged via pull
request — see [CONTRIBUTING.md](CONTRIBUTING.md). Planned/implemented
channels: [CHANNELS.md](CHANNELS.md).

## Documentation

- [docs/security-model.md](docs/security-model.md) — why `contact` is a
  server-validated slug, never a raw address, and what that buys against
  prompt injection.
- [docs/architecture.md](docs/architecture.md) — plugin system, lib/bin
  split, request lifecycle.
- [docs/deployment.md](docs/deployment.md) — self-hosting with
  Docker/Coolify, including known pitfalls.

## Architecture

**Plugin system** (`src/channels/`): each channel implements the
`Channel` trait in its own file and registers itself with one line in
`channels::registry()`. `main.rs`/`config.rs`/`mcp_server.rs` don't know
any channel by name. Details: [docs/architecture.md](docs/architecture.md).

## Installation

Four ways, depending on what you already have.

<details>
<summary><strong>Dev</strong> — clone and run directly with Cargo</summary>

```bash
cargo check
cargo test        # unit tests (src/) + integration tests (tests/)
```

```bash
cp config.example.toml config.toml
cp .env.example .env   # fill in
export $(cat .env | xargs)
export CONFIG_PATH=./config.toml
cargo run
```

</details>

<details>
<summary><strong>Binary</strong> — prebuilt release binary, no Rust/Cargo needed</summary>

Prebuilt binaries (Linux x86_64/aarch64, each glibc and musl, macOS
Intel/Apple Silicon) are available on the
[Releases page](https://github.com/Gnidreve/rolodex-mcp/releases), incl.
SHA-256 checksums.

```bash
curl -L -o rolodex-mcp.tar.gz \
  https://github.com/Gnidreve/rolodex-mcp/releases/latest/download/rolodex-mcp-v0.2.1-x86_64-unknown-linux-gnu.tar.gz
tar xzf rolodex-mcp.tar.gz

cp config.example.toml config.toml
cp .env.example .env   # fill in
export $(cat .env | xargs)
export CONFIG_PATH=./config.toml
./sendmail-mcp
```

Adjust the file name/version to whatever is actually current on the
Releases page.

</details>

<details>
<summary><strong>Docker</strong> — ready-made image, no repo checkout needed</summary>

```bash
docker run -d \
  -p 8080:8080 \
  -v "$(pwd)/config.toml:/app/config.toml:ro" \
  --env-file .env \
  ghcr.io/gnidreve/rolodex-mcp:latest
```

Prepare `config.toml` and `.env` as above (see `config.example.toml` /
`.env.example`).

</details>

<details>
<summary><strong>Docker Compose</strong> — for a local build + healthcheck/restart policy</summary>

```bash
cp config.example.toml config.toml   # adjust
cp .env.example .env                 # fill in credentials
docker compose up --build
```

</details>

The server then listens on `http://<host>:8080/`. Details, prebuilt
images and known self-hosting pitfalls:
[docs/deployment.md](docs/deployment.md).

## License

[Business Source License 1.1](LICENSE) — free for self-hosting, including
commercial/internal use. Not allowed: offering the project itself as a
competing hosted service. Automatically converts to Apache-2.0 four years
after each version's publication. BUSL is not an OSI-approved open source
license, but "source-available".
