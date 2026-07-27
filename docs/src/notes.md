# Notes and tradeoffs

## Cloudflare Workers constraints

### TCP from Rust

Cloudflare Workers exposes TCP via the JavaScript `connect()` global. Calling it from Rust requires going through `js_sys::Reflect`, because `tokio` and most synchronous-I/O crates cannot compile to `wasm32-unknown-unknown`. Noye's TCP and SMTP checkers do this directly without intermediate abstractions, which keeps the call shape transparent and avoids dragging in adapter crates.

Practical implications:

- Concrete TLS handshakes (e.g. `STARTTLS`, X.509 inspection) cannot be done with native crypto crates. `monitor/tls.rs` works around this by relying on `fetch()`'s implicit TLS validation and querying [crt.sh](https://crt.sh) for certificate expiry metadata.
- Banner reads happen with one buffered chunk read; multi-line SMTP responses spanning chunk boundaries are not stitched together. This has been adequate for the validations Noye performs, but is something to be aware of when extending the SMTP checker.

### CPU time

The Free plan caps Worker CPU time at 10 ms per request; the paid plan caps it at 30 ms. The scheduled handler in particular needs to fit within this envelope.

The architecture compensates by:

- Pulling at most 50 due-targets per Cron invocation (once per minute).
- Running checks one at a time rather than fanning out, so the per-iteration cost is bounded.
- Doing only minimal post-processing on results — heavy aggregation is read-time.

If the number of monitored targets grows beyond what the scheduler can drain in one minute, the recommended evolution is to split work onto a Cloudflare Queue and a fan-out worker pool. The current single-worker scheduler is a deliberate simplification.

### Web Crypto

Noye uses the Web Crypto API (`globalThis.crypto.subtle`) for everything: random number generation, SHA-256 (for PKCE), and JWT signature verification. This is the only supported high-quality crypto path on Workers; bringing in `ring` or similar is not possible against `wasm32-unknown-unknown`.

The wrapper in `crates/gateway/src/auth/crypto.rs` deliberately avoids `web-sys` and uses `js-sys::Reflect` directly, matching the style of the TCP/SMTP modules. This keeps the JS-binding surface visible in the Rust source, which simplifies auditing.

### Service Binding semantics

Service Bindings perform an in-process call to another worker in the same data center. They are zero-egress and bypass the public Internet, but they are still HTTP semantics: the target worker sees a `fetch` event with a `Request` object, the same as any external request.

Two consequences:

- The URL passed to the binding has its host portion ignored. We use `https://core.internal` purely because `Request::new_with_init` requires a syntactically valid URL.
- The Service Binding does not propagate a caller identity automatically. We carry it in `X-Caller-*` headers, with the gateway token providing the trust anchor.

## Design decisions

### Why no JS framework for the UI?

The UI is small (a handful of pages) and read-mostly. Server-rendered HTML keeps the dependency tree minimal, satisfies ABDD by construction, and avoids the bundle-size and complexity costs of a frontend framework on Workers.

### Why split into Gateway and Core?

Three reasons in order of importance:

1. **Reachability containment.** Only the Gateway has a public route. A bug in the Core (e.g. an over-permissive query) cannot be exploited from the Internet because the Core cannot be called from there.
2. **Binding minimization.** The Gateway has no D1 binding. Even if the Gateway code were compromised, it cannot bypass the Core's API surface to read or write the database.
3. **Ownership clarity.** Each worker has a clear, narrow responsibility, which makes the codebase easier to reason about than a single monolithic worker.

The cost is one extra Service Binding hop per data operation (a few hundred microseconds in the same data center). For a system whose request volume is low and whose latency budget is generous (server health monitoring), this is well worth the security gain.

### Why pre-register users instead of auto-provisioning?

Auto-provisioning from a successful OIDC sign-in is convenient but breaks the "no guests" requirement. Anyone who can authenticate at the IdP would obtain at least read access to the system. Requiring an explicit pre-registration keeps the trust boundary inside Noye.

### Why centralize types in `noye-shared`?

Both workers serialize types across the Service Binding boundary as JSON. A drift between the Gateway's view of `Target` and the Core's view would manifest as a deserialization error at runtime. Keeping the types in a shared crate ensures one definition is the single source of truth for both sides of the wire.
