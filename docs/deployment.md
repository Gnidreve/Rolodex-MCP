# Deployment / Self-Hosting

Vier gleichwertige Wege, keiner "besser" als der andere — je nachdem, was
auf dem Zielsystem schon vorhanden ist: Dev (lokaler Rust-Build), Binary
(fertiges Release-Binary, kein Rust nötig), Docker (fertiges Image, kein
Repo-Checkout nötig) und Docker Compose (lokaler Build + Healthcheck/
Restart-Policy out of the box). Kurzanleitungen für alle vier stehen in
der [README](../README.md#installation); hier nur die Details, die dort
keinen Platz haben.

## Fertige Binaries

Bei jedem Git-Tag `vX.Y.Z` baut `.github/workflows/release.yml` das
Binary für sechs Zielplattformen (Linux x86_64/aarch64 je glibc und musl,
macOS Intel/Apple Silicon) und lädt sie samt SHA-256-Checksummen als
GitHub-Release-Assets hoch. Die musl-Varianten sind vollständig statisch
gelinkt — kein `glibc`, kein `libssl`, keine Laufzeit-Abhängigkeit
überhaupt (seit dem Wechsel von `native-tls`/OpenSSL auf `rustls` mit
gebündelten Root-Zertifikaten, siehe Commit-Historie). Das macht sie
besonders robust für ältere oder minimale Distros.

## Mit einem fertigen Image

Images werden per GitHub Actions nach jedem Push auf `main` (der nicht
nur Doku/Skill-Dateien betrifft, siehe `.github/workflows/docker.yml`)
nach GHCR gebaut:

```
ghcr.io/gnidreve/rolodex-mcp:latest
```

## Reverse Proxy / TLS

Der Prozess selbst spricht nur HTTP, kein TLS. In Produktion gehört ein
Reverse Proxy davor (Traefik, Caddy, nginx) — bei Coolify übernimmt das
dessen eigene Traefik-Instanz automatisch.

## Bekannte Stolperfallen (aus echtem Betrieb)

Diese drei Punkte haben in der Praxis beim Erstdeployment wirklich
Zeit gekostet — nicht theoretisch, sondern konkret bei einem Deployment
über Coolify:

- **Nicht-ASCII-Zeichen in `MCP_BEARER_TOKEN`**: HTTP-Header-Werte dürfen
  nur ASCII sein. Mit einem nicht-ASCII-Token setzen manche HTTP-Clients
  den `Authorization`-Header stillschweigend gar nicht erst — der Server
  lehnt dann *jeden* Request mit "Header fehlt" ab, was wie ein
  Client-Bug aussieht, tatsächlich aber ein ungültiger Token war. Der
  Server bricht das deshalb schon beim Start hart ab, mit einer klaren
  Fehlermeldung, statt das Problem erst bei merkwürdigen 401ern im
  Log-Viewer auffallen zu lassen. Abhilfe: Token aus rein druckbaren
  ASCII-Zeichen generieren.
- **Portkonflikt bei Docker Compose**: Ein festes `ports: - "8080:8080"`
  kollidiert, sobald ein zweiter Dienst auf demselben Host-Port lauschen
  will. Bei Coolify (oder jedem Setup mit vorgeschaltetem Reverse Proxy)
  reicht `expose: - "8080"` — der Proxy routet intern, ein
  Host-Port-Mapping ist dafür nicht nötig und verursacht nur "Bad
  Gateway", wenn ein anderer Container den Port schon belegt.
- **`GET /` als Healthcheck + `stateful_mode`**: rmcps
  `StreamableHttpService` reserviert bei `stateful_mode: true` GET auf
  derselben Route für SSE-Session-Resumption. Da dieser Server GET selbst
  für den Healthcheck braucht und kein Session-Handling will, läuft er
  mit `stateful_mode: false` (siehe `src/lib.rs`). Wer das ändert, verliert
  den Healthcheck oder bricht rmcps Session-Annahmen — beides äußert sich
  nicht als klarer Fehler, sondern als stilles Fehlverhalten.

## Logging

Standardmäßig (ohne `RUST_LOG`) läuft der Server auf `INFO`, nicht auf
`ERROR` (`tracing_subscriber`s Default) — sonst wären weder Startup- noch
Request-Logs sichtbar. `RUST_LOG=debug` zieht zusätzlich rmcps eigenes
internes Request/Response-Tracing hoch, das im Normalbetrieb bewusst auf
`warn` gedrosselt ist (`info,rmcp=warn`), weil es sonst jede Antwort
komplett in den Log dumpt.

## Weiterführend

- [architecture.md](architecture.md) — wie der Server intern
  aufgebaut ist.
- [security-model.md](security-model.md) — das Sicherheitsmodell, falls
  sich die Frage "warum nicht einfach ein generisches Send-Tool"
  stellt.
