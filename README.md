# sendmail-mcp

Ein MCP-Server, der für jeden Kontakt aus `config.toml` genau ein Tool
erzeugt (`send_email_to_<name>`). Der Agent sieht nur Namen, nie
E-Mail-Adressen — es gibt kein generisches Tool mit freiem `to`-Feld.

## ⚠️ Bekannte Unsicherheiten — bitte vor erstem Einsatz prüfen

Diese Sandbox hat **keinen Netzwerkzugriff**, ich konnte also nicht
`cargo build`/`cargo test` gegen crates.io fahren. Der Code ist nach
bestem Wissen gegen die dokumentierte `rmcp`-API (Version 0.16,
Stand der Recherche in diesem Chat) geschrieben, aber folgende Stellen
sind API-Oberfläche, die sich zwischen SDK-Versionen am ehesten
verschiebt und die ich **nicht kompiliert/verifiziert** habe:

- Exakte Signatur von `StreamableHttpService::new(...)` in `main.rs`
  (Reihenfolge/Typ der Parameter, insb. `LocalSessionManager` und
  `StreamableHttpServerConfig`).
- Ob `axum::Router::nest_service("/mcp", service)` so direkt funktioniert
  oder der Service erst in eine `tower::Service`-kompatible Form
  gebracht werden muss.
- `Tool::new(name, description, schema)` — Reihenfolge/Typ des dritten
  Parameters (`Arc<Map<String, Value>>`).
- `ListToolsResult::with_all_items(...)`, `CallToolResult::success/error`,
  `McpError::invalid_params` — Namen stimmen laut Doku-Recherche, aber
  Feature-Flags/Pfade können je nach `rmcp`-Version leicht abweichen.

**Erster Schritt bei euch:** `cargo check` lokal laufen lassen. Erwartbar
sind höchstens kleinere Signatur-Anpassungen, keine grundsätzliche
Neuarchitektur — die Kernlogik (Config laden, Slug-Kollision hart
ablehnen, dynamische Tool-Liste, SMTP-Versand) ist SDK-unabhängig und
in eigenen Unit-Tests (`src/config.rs`) abgedeckt.

## Architektur

- `config.toml` (per Volume gemountet) = **nur** Kontaktbuch:
  ```toml
  [[to]]
  name = "Max Mustermann"
  email = "max@example.com"
  ```
- `.env` (per `env_file`) = **komplette** SMTP-Konfiguration:
  Host, Port, Encryption, optionale Credentials, Absenderadresse,
  Absender-Anzeigename, Timeout. Siehe `.env.example`.
- Beim Start wird die Kontaktliste geparst. Zwei Namen, die auf denselben
  Tool-Namen ("Slug") abbilden würden, führen zu einem **harten
  Startabbruch** mit klarer Fehlermeldung — kein Fuzzy-Matching, keine
  stille Kollisionsauflösung.
- Jedes Tool erwartet `subject` (string) und `body` (string, Klartext).
- Transport: Streamable HTTP unter `/mcp`. Der Prozess selbst terminiert
  kein TLS — in Produktion Reverse Proxy davorsetzen (Traefik/Caddy/nginx).

## Lokal bauen & prüfen

```bash
cargo check
cargo test        # prüft u.a. die Umlaut-Slugifizierung und Kollisionserkennung
cargo run
```

`CONFIG_PATH` (Default `/app/config.toml`) und die `SMTP_*`-Variablen
müssen gesetzt sein, z.B.:

```bash
cp config.example.toml config.toml
cp .env.example .env
# .env ausfüllen
export $(cat .env | xargs)
export CONFIG_PATH=./config.toml
cargo run
```

## Docker

```bash
cp config.example.toml config.toml   # anpassen
cp .env.example .env                 # SMTP-Zugangsdaten eintragen
docker compose up --build
```

Der Server lauscht dann auf `http://<host>:8080/mcp` (Streamable HTTP).

## Erweiterungsideen (bewusst nicht gebaut, minimal-invasiv gehalten)

- Hot-Reload der `config.toml` ohne Neustart.
- Mehrere Empfänger pro Aufruf / CC.
- Direkte TLS-Terminierung im Rust-Prozess statt Reverse Proxy.
- `SMTP_ACCEPT_INVALID_CERTS` für selbstsignierte interne Mailserver.
