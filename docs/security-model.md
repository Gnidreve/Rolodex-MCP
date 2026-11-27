# Sicherheits- und Privacy-Modell

Rolodex MCP existiert wegen genau einer Entscheidung: **der Agent bekommt
niemals eine rohe Adresse, Chat-ID oder Webhook-URL als Parameter zu
sehen.** Alles andere im Projekt (Plugin-Architektur, Tool-Namensschema,
Konfigurationstrennung) leitet sich daraus ab.

## Das Problem

Ein generisches `send_message(channel, to, subject, body)`-Tool gibt dem
Agenten (bzw. dem LLM, das die Tool-Calls erzeugt) volle Kontrolle über
**wohin** eine Nachricht geht. Das bedeutet:

- Ein Prompt-Injection-Angriff über einen bearbeiteten Dokumentinhalt, eine
  Webseite oder eine andere Tool-Antwort kann den Agenten dazu bringen,
  eine Nachricht an eine beliebige, vom Angreifer kontrollierte Adresse zu
  schicken — Exfiltration von Kontext, Social Engineering des Empfängers,
  oder einfach Spam im Namen des Nutzers.
- Das Modell sieht im Kontextfenster (Tool-Schema, vorherige Tool-Calls,
  Logs) potenziell reale E-Mail-Adressen, Telefonnummern oder Chat-IDs von
  Kontakten — Daten, die in einem LLM-Kontext grundsätzlich nichts zu
  suchen haben, wenn es sich vermeiden lässt.

## Die Lösung: Tool-Identität statt Tool-Parameter

Rolodex MCP generiert beim Start für jede Kontakt-Kanal-Kombination aus
`config.toml` ein eigenes, fest benanntes Tool:
`send_to_<name>_via_<kanal>`. Es gibt **kein Parameter-Feld für Adresse,
Chat-ID oder Kanal** — wer gemeint ist, steht ausschließlich im
Tool-Namen, den der MCP-Host dem Modell als Teil des Tool-Schemas
vorgibt. Das Tool selbst nimmt nur `subject` und `body` entgegen.

Konkret heißt das:

- Ein Prompt-Injection-Angriff kann den Agenten höchstens dazu bringen,
  ein *existierendes* Tool aufzurufen (also an einen der im Kontaktbuch
  hinterlegten Kontakte zu schicken) — nicht an eine beliebige, vom
  Angreifer kontrollierte Adresse. Das Blast-Radius ist von "Nachricht an
  irgendwen im Internet" auf "Nachricht an einen der eigenen Kontakte"
  reduziert.
- Die eigentlichen Adressen/IDs/Webhook-URLs stehen nur in `config.toml`
  (Kontaktbuch) bzw. `.env` (Zugangsdaten) auf dem Server — nie im
  Modellkontext, nie in einer Tool-Antwort, nie im Tool-Schema selbst.

## Warum das nicht "nur Alias" ist

Mehrere existierende Multi-Channel-Notification-Server lösen das Problem
scheinbar ähnlich über **Alias-Parameter** (ein Name statt einer rohen
Adresse). Der entscheidende Unterschied: Bei einem Alias-Parameter bleibt
ein `recipient`-Feld im Tool-Schema bestehen, und die meisten dieser
Implementierungen fallen bei einem unbekannten Alias auf **rohe
Werte-Passthrough** zurück — der Agent kann also weiterhin eine beliebige
Adresse eintragen, wenn er (oder ein Angreifer über Prompt Injection) das
möchte. Bei Rolodex MCP gibt es dieses Feld schlicht nicht. Es ist keine
Konvention, die der Agent einhalten *sollte* — es ist strukturell
unmöglich, sie zu brechen.

## Zwei Trennungen, die diesem Modell dienen

1. **Kontaktbuch (`config.toml`) vs. Zugangsdaten (`.env`)**: Wer
   erreichbar ist, steht getrennt davon, womit verschickt wird. Ein
   kompromittiertes Kontaktbuch (z.B. versehentlich committed) legt keine
   SMTP-Passwörter oder Bot-Tokens offen.
2. **Validierung vs. Verbindungstest**: `ChannelDef::validate` prüft beim
   Parsen nur das Format (reine Funktion, kein Netzwerk). Erst
   `Channel::test_connection` beim Serverstart baut eine echte Verbindung
   auf — ein falsch konfigurierter Kanal fällt beim Start auf, nicht erst
   beim ersten Sendeversuch mitten im Betrieb.

## Was das Modell nicht abdeckt

- Es schützt nicht vor einem Angreifer, der bereits `config.toml` oder
  `.env` auf dem Server selbst verändern kann — das ist dann schon eine
  Kompromittierung der Infrastruktur, kein Agenten-Problem mehr.
- Es verhindert nicht, dass der Agent eine *inhaltlich* schädliche
  Nachricht an einen echten Kontakt schickt (z.B. durch Prompt Injection
  manipulierter Text) — nur **wohin** ist eingeschränkt, nicht **was**.
