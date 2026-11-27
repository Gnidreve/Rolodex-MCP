# Rolodex MCP

[![Docker](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/docker.yml/badge.svg)](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/docker.yml)
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

## Lokal bauen & prüfen

```bash
cargo check
cargo test        # Unit-Tests (src/) + Integrationstests (tests/)
cargo run
```

```bash
cp config.example.toml config.toml
cp .env.example .env   # ausfüllen
export $(cat .env | xargs)
export CONFIG_PATH=./config.toml
cargo run
```

## Docker

```bash
cp config.example.toml config.toml   # anpassen
cp .env.example .env                 # Zugangsdaten eintragen
docker compose up --build
```

Server läuft dann auf `http://<host>:8080/`. Details, fertige Images und
bekannte Stolperfallen beim Self-Hosting: [docs/deployment.md](docs/deployment.md).

## Lizenz

[Business Source License 1.1](LICENSE) — frei für Self-Hosting, auch
kommerziell/intern. Nicht erlaubt: das Projekt selbst als konkurrierenden
gehosteten Dienst anbieten. Wandelt sich vier Jahre nach Veröffentlichung
jeder Version automatisch in Apache-2.0 um. BUSL ist keine von der OSI
anerkannte Open-Source-Lizenz, sondern "source-available".
