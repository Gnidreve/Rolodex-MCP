# Contributing — Channel-Branches (harte Projektregel)

Jeder Outbound-Kanal (E-Mail, Telegram, ntfy.sh, ...) wird **auf einer
eigenen Git-Branch** entwickelt, benannt nach dem Kanal (z.B. `telegram`,
`ntfy`, `whatsapp`). Diese Regel gilt ab jetzt dauerhaft für dieses Projekt.

## Workflow pro Kanal

1. Branch von `main` abzweigen, benannt nach dem Kanal (kleingeschrieben,
   z.B. `git checkout -b ntfy`).
2. Implementierung ausschließlich in einer **neuen Datei**
   `src/channels/<kanal>.rs`, die den `Channel`-Trait aus
   `src/channels/mod.rs` implementiert (siehe `src/channels/email.rs` als
   Referenz).
3. Genau **eine Zeile** in `src/channels/mod.rs::registry()` ergänzen, um
   den neuen Kanal zu registrieren. Das ist die einzige Stelle außerhalb der
   neuen Datei, die angefasst werden darf.
4. `main.rs`, `lib.rs`, `config.rs` und `mcp_server.rs` **nicht anfassen** —
   die kennen nur den `Channel`-Trait und `ChannelDef`, nie ein konkretes
   Backend. Wenn ein Kanal das nicht hergibt (z.B. weil er fundamental
   andere Metadaten braucht), ist das ein Zeichen, dass der Wrapper selbst
   erweitert werden muss — das läuft dann als eigene Diskussion, nicht
   nebenbei in einem Kanal-PR.
5. Pull Request nach `main` öffnen. **Nicht selbst mergen** — Merges
   (und deren Reihenfolge, falls mehrere Kanal-PRs gleichzeitig offen sind)
   entscheidet der Projekt-Owner.

## Ausnahme: Änderungen am Wrapper selbst

Der `Channel`-Trait, `ChannelDef` und die generische Logik in `main.rs`/
`lib.rs`/`config.rs`/`mcp_server.rs` sind die Ausnahme von der
Branch-pro-Kanal-Regel und werden **direkt auf `main`** entwickelt und
gepusht. Das sollte nach der initialen Einführung dieser Architektur nur
noch selten nötig sein — wenn doch, sind das architektonische
Entscheidungen, die separat vom eigentlichen Kanal-Feature behandelt werden.

## Warum

So lassen sich mehrere Kanäle parallel und unabhängig voneinander
entwickeln, ohne dass ihre Branches sich durch Merge-Konflikte in
gemeinsamen Dateien gegenseitig blockieren — jeder Kanal-PR berührt nur
seine eigene neue Datei plus eine Zeile in der Registry.
