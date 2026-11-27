# Rolodex MCP

[![Test](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/test.yml/badge.svg)](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/test.yml)
[![Release](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/release.yml/badge.svg)](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/release.yml)
[![License: BUSL 1.1](https://img.shields.io/badge/License-BUSL--1.1-blue.svg)](LICENSE)

Ein MCP-Server, der für jeden Kontakt aus `config.toml` **und Kanal**
(E-Mail, ntfy.sh, Telegram, Discord, Slack) ein eigenes Tool erzeugt
(`send_to_<name>_via_<kanal>`). Der Agent sieht nur Namen und Kanal über
den Tool-Namen, nie die tatsächliche Adresse — es gibt kein generisches
Tool mit freier Adress- oder Kanalwahl. Warum das so ist, steht in
[docs/security-model.md](docs/security-model.md).

Neue Kanäle werden auf eigenen Git-Branches entwickelt und per Pull Request
gemerged — siehe [CONTRIBUTING.md](CONTRIBUTING.md). Geplante/umgesetzte
Kanäle: [CHANNELS.md](CHANNELS.md).

## Dokumentation

- [docs/security-model.md](docs/security-model.md) — warum es kein
  Adress-Parameter gibt, und was das gegen Prompt-Injection bringt.
- [docs/architecture.md](docs/architecture.md) — Plugin-System, lib/bin-Split,
  Request-Lebenszyklus.
- [docs/deployment.md](docs/deployment.md) — Self-Hosting mit Docker/Coolify,
  inkl. bekannter Stolperfallen.

## Architektur

**Plugin-System** (`src/channels/`): jeder Kanal implementiert den
`Channel`-Trait in einer eigenen Datei und registriert sich mit einer Zeile
in `channels::registry()`. `main.rs`/`config.rs`/`mcp_server.rs` kennen
keinen Kanal namentlich. Details: [docs/architecture.md](docs/architecture.md).

## Installation

Vier Wege, je nach dem, was du schon hast.

<details>
<summary><strong>Dev</strong> — clonen und direkt mit Cargo laufen lassen</summary>

```bash
cargo check
cargo test        # Unit-Tests (src/) + Integrationstests (tests/)
```

```bash
cp config.example.toml config.toml
cp .env.example .env   # ausfüllen
export $(cat .env | xargs)
export CONFIG_PATH=./config.toml
cargo run
```

</details>

<details>
<summary><strong>Binary</strong> — fertiges Release-Binary, kein Rust/Cargo nötig</summary>

Vorgebaute Binaries (Linux x86_64/aarch64, jeweils glibc und musl, macOS
Intel/Apple Silicon) gibt es auf der
[Releases-Seite](https://github.com/Gnidreve/rolodex-mcp/releases), inkl.
SHA-256-Checksummen.

```bash
curl -L -o rolodex-mcp.tar.gz \
  https://github.com/Gnidreve/rolodex-mcp/releases/latest/download/rolodex-mcp-v0.2.0-x86_64-unknown-linux-gnu.tar.gz
tar xzf rolodex-mcp.tar.gz

cp config.example.toml config.toml
cp .env.example .env   # ausfüllen
export $(cat .env | xargs)
export CONFIG_PATH=./config.toml
./sendmail-mcp
```

Dateiname/Version an das tatsächlich aktuelle Release auf der
Releases-Seite anpassen.

</details>

<details>
<summary><strong>Docker</strong> — fertiges Image, kein Repo-Checkout nötig</summary>

```bash
docker run -d \
  -p 8080:8080 \
  -v "$(pwd)/config.toml:/app/config.toml:ro" \
  --env-file .env \
  ghcr.io/gnidreve/rolodex-mcp:latest
```

`config.toml` und `.env` wie oben vorbereiten (siehe `config.example.toml`
/ `.env.example`).

</details>

<details>
<summary><strong>Docker Compose</strong> — für lokalen Build + Healthcheck/Restart-Policy</summary>

```bash
cp config.example.toml config.toml   # anpassen
cp .env.example .env                 # Zugangsdaten eintragen
docker compose up --build
```

</details>

Server läuft dann auf `http://<host>:8080/`. Details, fertige Images und
bekannte Stolperfallen beim Self-Hosting: [docs/deployment.md](docs/deployment.md).

## Lizenz

[Business Source License 1.1](LICENSE) — frei für Self-Hosting, auch
kommerziell/intern. Nicht erlaubt: das Projekt selbst als konkurrierenden
gehosteten Dienst anbieten. Wandelt sich vier Jahre nach Veröffentlichung
jeder Version automatisch in Apache-2.0 um. BUSL ist keine von der OSI
anerkannte Open-Source-Lizenz, sondern "source-available".
