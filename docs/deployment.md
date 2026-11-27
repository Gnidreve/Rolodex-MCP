# Deployment / Self-Hosting

Four equally valid ways, none "better" than the other — depending on
what's already available on the target system: Dev (local Rust build),
Binary (prebuilt release binary, no Rust needed), Docker (ready-made
image, no repo checkout needed) and Docker Compose (local build +
healthcheck/restart policy out of the box). Quick instructions for all
four are in the [README](../README.md#installation); only the details
that don't fit there are here.

## Release pipeline

A single workflow (`.github/workflows/release.yml`) automatically creates
a git tag `vX.Y.Z` + a GitHub Release on every push to `main` that
changes the version number in `Cargo.toml`, and builds in parallel for it:

- **Binaries** for six target platforms (Linux x86_64/aarch64 each glibc
  and musl, macOS Intel/Apple Silicon), with SHA-256 checksums, as release
  assets. The musl variants are fully statically linked — no `glibc`, no
  `libssl`, no runtime dependency at all (since the switch from
  `native-tls`/OpenSSL to `rustls` with bundled root certificates, see the
  commit history). That makes them especially robust on older or minimal
  distros.
- **Docker image**, tagged with `latest` and `vX.Y.Z`, pushed to GHCR:
  ```
  ghcr.io/gnidreve/rolodex-mcp:latest
  ```

A pure dependency bump or a documentation change on `main`, without a
version jump in `Cargo.toml`, deliberately triggers **no** new build —
release artifacts are produced exclusively alongside the version number
tracked in the code, not on every commit.

## Reverse proxy / TLS

The process itself only speaks HTTP, no TLS. In production a reverse
proxy belongs in front of it (Traefik, Caddy, nginx) — on Coolify its own
Traefik instance handles that automatically.

## Known pitfalls (from real operation)

These three points actually cost real time in practice during the first
deployment — not theoretical, but concretely during a deployment via
Coolify:

- **Non-ASCII characters in `MCP_BEARER_TOKEN`**: HTTP header values must
  be ASCII only. With a non-ASCII token, some HTTP clients silently don't
  set the `Authorization` header at all — the server then rejects *every*
  request with "header missing", which looks like a client bug but was
  actually an invalid token. The server therefore fails hard right at
  startup, with a clear error message, instead of letting the problem
  only surface as odd 401s in the log viewer. Fix: generate the token
  from purely printable ASCII characters.
- **Port conflict with Docker Compose**: a fixed `ports: - "8080:8080"`
  collides as soon as a second service wants to listen on the same host
  port. On Coolify (or any setup with a reverse proxy in front) `expose:
  - "8080"` is enough — the proxy routes internally, a host port mapping
  isn't needed for that and only causes "Bad Gateway" once another
  container already occupies the port.
- **`GET /` as healthcheck + `stateful_mode`**: rmcp's
  `StreamableHttpService` reserves GET on the same route for SSE session
  resumption when `stateful_mode: true`. Since this server needs GET
  itself for the healthcheck and doesn't want session handling, it runs
  with `stateful_mode: false` (see `src/lib.rs`). Changing that loses the
  healthcheck or breaks rmcp's session assumptions — neither shows up as
  a clear error, just as silent misbehavior.

## Logging

By default (without `RUST_LOG`), the server runs at `INFO`, not `ERROR`
(`tracing_subscriber`'s default) — otherwise neither the startup logs nor
the request logs would be visible. `RUST_LOG=debug` additionally pulls up
rmcp's own internal request/response tracing, which is deliberately
throttled to `warn` in normal operation (`info,rmcp=warn`), because
otherwise it dumps every response completely into the log.

## Further reading

- [architecture.md](architecture.md) — how the server is built
  internally.
- [security-model.md](security-model.md) — the security model, in case
  the question "why not just a generic send tool" comes up.
