# sendmail-mcp

Ein MCP-Server, der für jeden Kontakt aus `config.toml` genau ein Tool
erzeugt (`send_email_to_<name>`). Der Agent sieht nur Namen, nie
E-Mail-Adressen — es gibt kein generisches Tool mit freiem `to`-Feld.

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
- Transport: Streamable HTTP direkt auf `/` (nicht `/mcp`):
  - `GET /` — ungeschützter Healthcheck, liefert `{"status":"ok"}`. Kein
    Bearer-Token nötig, damit Docker/Coolify ihn erreichen können.
  - `POST /` — der eigentliche MCP-Endpunkt. Erfordert den Header
    `Authorization: Bearer <MCP_BEARER_TOKEN>` (siehe `src/auth.rs`), sonst
    `401 Unauthorized`. Der Wert kommt aus der Pflicht-ENV-Variable
    `MCP_BEARER_TOKEN` (siehe `.env.example`) — zum Rotieren dort ändern
    und neu deployen.
  - Der Server läuft mit `stateful_mode: false` (kein Session-Handling,
    kein `Mcp-Session-Id`), weil GET auf derselben Route bereits für den
    Healthcheck reserviert ist — rmcp würde GET sonst für
    SSE-Session-Resumption brauchen.
- Der Prozess selbst terminiert kein TLS — in Produktion Reverse Proxy
  davorsetzen (bei Coolify übernimmt das dessen Traefik-Instanz).

## Logging

Standardmäßig (ohne `RUST_LOG` gesetzt) läuft der Server auf `INFO`-Level,
nicht auf `ERROR` (was `tracing_subscriber`'s Default wäre) — sonst wären
weder die Startup-Logs noch die Request-Logs sichtbar. Bei jedem Request
gegen `/` loggt eine `tower-http`-`TraceLayer` Methode, Pfad, Statuscode und
Latenz; abgelehnte Bearer-Token loggen zusätzlich den Grund (fehlender vs.
falscher Header — der Token-Wert selbst landet nie im Log). Für mehr Detail
(inkl. `rmcp`s eigenem Request/Response-Tracing) `RUST_LOG=debug` setzen.

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

Der Server lauscht dann auf `http://<host>:8080/` (Streamable HTTP, MCP via
POST mit Bearer-Token; GET liefert den Healthcheck). Ein eingebauter
Docker-Healthcheck (`curl -f http://localhost:8080/`) ist in
`docker-compose.yml` hinterlegt.

## Erweiterungsideen (bewusst nicht gebaut, minimal-invasiv gehalten)

- Hot-Reload der `config.toml` ohne Neustart.
- Mehrere Empfänger pro Aufruf / CC.
- Direkte TLS-Terminierung im Rust-Prozess statt Reverse Proxy.
- `SMTP_ACCEPT_INVALID_CERTS` für selbstsignierte interne Mailserver.
