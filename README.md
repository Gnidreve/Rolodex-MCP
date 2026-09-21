# Rolodex MCP

[![Docker](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/docker.yml/badge.svg)](https://github.com/Gnidreve/rolodex-mcp/actions/workflows/docker.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Ein MCP-Server, der für jeden Kontakt aus `config.toml` **und Kanal** (E-Mail,
künftig weitere) ein eigenes Tool erzeugt (`send_to_<name>_via_<kanal>`). Der
Agent sieht nur Namen und Kanal über den Tool-Namen, nie die tatsächliche
Adresse — es gibt kein generisches Tool mit freier Adress- oder Kanalwahl.

Neue Kanäle werden auf eigenen Git-Branches entwickelt und per Pull Request
gemerged — siehe [CONTRIBUTING.md](CONTRIBUTING.md). Geplante/umgesetzte
Kanäle: [CHANNELS.md](CHANNELS.md).

## Architektur

- **Plugin-System** (`src/channels/`): jeder Kanal implementiert den
  `Channel`-Trait in einer eigenen Datei und registriert sich mit einer Zeile
  in `channels::registry()`. `main.rs`/`config.rs`/`mcp_server.rs` kennen
  keinen Kanal namentlich.
- `config.toml` = nur Kontaktbuch (Namen + Adressen). `.env` = Zugangsdaten
  pro Kanal. Siehe `config.example.toml` / `.env.example`.
- Kollidierende Tool-Namen, ein Kontakt ohne Kanal, oder ein fehlgeschlagener
  `test_connection()`-Check beim Start → harter Startabbruch mit klarer
  Fehlermeldung, statt später unklar zu scheitern.
- Transport: Streamable HTTP auf `/` — `GET` ist ein ungeschützter
  Healthcheck, `POST` der MCP-Endpunkt und verlangt
  `Authorization: Bearer <MCP_BEARER_TOKEN>` (siehe `src/auth.rs`).

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

Server läuft dann auf `http://<host>:8080/`. Ein eingebauter
Docker-Healthcheck (`curl -f http://localhost:8080/`) ist in
`docker-compose.yml` hinterlegt.

## Lizenz

[MIT](LICENSE)
