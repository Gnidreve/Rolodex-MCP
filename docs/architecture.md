# Architektur

## Überblick

```
config.toml (Kontaktbuch)  ──┐
                              ├──> config::load_contacts() ──> Vec<ChannelTool>
.env (Zugangsdaten)      ────┘                                       │
                                                                       ▼
channels::registry()  ──> Vec<ChannelDef> ──> main.rs baut Sender-Map
                                                       │
                                                       ▼
                                    SendMailServer (mcp_server.rs)
                                                       │
                                                       ▼
                                   axum-Router (lib.rs::build_router)
```

## Das Plugin-System (`src/channels/`)

Jeder Kanal ist eine eigene Datei, die den `Channel`-Trait implementiert:

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    async fn test_connection(&self) -> Result<()>;
    async fn send(&self, contact_name: &str, address: &str, subject: &str, body: &str) -> Result<()>;
}
```

Dazu kommt `ChannelDef`: statische Metadaten (Konfigurationsfeld-Name,
Tool-Namen-Segment, Anzeigename, Validierungsfunktion, ENV-Loader). Ein
Kanal registriert sich mit **einer Zeile** in `channels::registry()`.

`src/main.rs`, `src/config.rs` und `src/mcp_server.rs` kennen zu keinem
Zeitpunkt einen konkreten Kanal namentlich — sie iterieren ausschließlich
generisch über `registry()`. Das ist absichtlich: ein neuer Kanal soll
sich hinzufügen lassen, ohne eine der drei Dateien anzufassen (siehe
[CONTRIBUTING.md](../CONTRIBUTING.md) für die Branch-pro-Kanal-Regel, die
genau darauf aufbaut).

## lib.rs vs. main.rs

Die eigentliche Serverlogik (Kanäle laden, Router bauen) lebt in
`src/lib.rs` als `pub fn build_router(...)`. `src/main.rs` ist nur noch
der dünne Einstiegspunkt (ENV lesen, Logging aufsetzen, `build_router`
aufrufen, binden). Diese Trennung existiert, damit `tests/` echte
Integrationstests gegen die öffentliche API fahren kann, statt nur
eingebettete Unit-Tests pro Modul — siehe `tests/api.rs` für ein
Beispiel, das den kompletten MCP-Roundtrip (Healthcheck, Auth,
`tools/list`, `tools/call`) gegen einen Mock-Channel prüft, ganz ohne
echten TCP-Bind.

## Start- und Request-Lebenszyklus

1. `config::load_contacts()` parst `config.toml`, validiert jeden
   Kanal-Wert rein formal (`ChannelDef::validate`, kein Netzwerk) und
   erzeugt pro Kontakt-Kanal-Kombination ein `ChannelTool`
   (Tool-Name, Titel, Adresse, Kanal-Slug).
2. `main.rs` lädt für jeden tatsächlich genutzten Kanal die
   ENV-Konfiguration (`ChannelDef::from_env`) und prüft die Verbindung
   einmal real (`Channel::test_connection`). Schlägt das fehl, startet
   der Prozess gar nicht erst.
3. `build_router` baut daraus den MCP-Server (`SendMailServer`) und den
   axum-Router: `GET /` ungeschützter Healthcheck, `POST /` der
   eigentliche MCP-Endpunkt hinter `Authorization: Bearer
   <MCP_BEARER_TOKEN>` (`src/auth.rs`).
4. Bei einem `tools/call` sucht `SendMailServer::call_tool` das Tool über
   den Tool-Namen, schlägt den passenden `Box<dyn Channel>` in der
   Sender-Map nach und ruft `Channel::send` auf. Der Fehlerfall wird als
   `isError: true` im MCP-Ergebnis zurückgegeben, nicht als
   Protokollfehler — der Agent bekommt eine lesbare Fehlermeldung statt
   eines kaputten Calls.

## Warum `Box<dyn Channel>` statt Generics

Die Anzahl und Art der Kanäle steht erst zur Laufzeit fest (abhängig von
`config.toml`), und `main.rs` muss mit einer heterogenen Sammlung aus
`SmtpConfig`, `TelegramConfig`, `NtfyConfig`, usw. gleichzeitig umgehen
können, ohne sie namentlich zu kennen. Dynamic Dispatch über `dyn Channel`
ist hier kein Stilkompromiss, sondern die einzige Option, die zum
Plugin-Prinzip passt — ein neuer Kanal bringt seine Dyn-Compatibility von
selbst mit, ohne dass die aufrufende Seite angepasst werden muss.

## Weiterführend

- [security-model.md](security-model.md) — warum es überhaupt kein
  Adress-Parameter gibt.
- [deployment.md](deployment.md) — Self-Hosting mit Docker/Coolify.
- [../CHANNELS.md](../CHANNELS.md) — welche Kanäle existieren bzw. als
  Idee offen sind.
- [../CONTRIBUTING.md](../CONTRIBUTING.md) — wie ein neuer Kanal
  beigetragen wird.
