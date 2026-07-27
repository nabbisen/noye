# Cloudflare Turnstile

Per the original specification, Cloudflare Turnstile is used as a *supplementary* defense on public forms — login, contact, signup, and any other endpoint accepting input from unauthenticated users. It is not applied site-wide.

## Current scope in Noye

Noye delegates user authentication to an OIDC provider, so there is no native login form on the Gateway. As of 0.1.0 there are no public forms in the application. The Turnstile module is shipped as **scaffolding** so that the first public form to be added — for example a feedback form, a public-status-page subscription, or a fallback non-OIDC login — can opt in with three lines of code.

## How it works

The implementation lives in [`crates/gateway/src/auth/turnstile.rs`](../crates/gateway/src/auth/turnstile.rs) and exposes four public helpers:

| Function | Purpose |
|---|---|
| `is_enabled(env) -> bool` | True when `TURNSTILE_SITE_KEY` is non-empty |
| `widget_html(env) -> String` | The `<div class="cf-turnstile">` element to embed inside a form. Empty string when disabled. |
| `script_tag_html(env) -> String` | The `<script>` tag that loads Cloudflare's `api.js`. Empty string when disabled. |
| `verify_token(env, token, remote_ip) -> Result<()>` | Server-side verification of a submitted token. Returns `Ok(())` when disabled, so callers can use it unconditionally. |

When `TURNSTILE_SITE_KEY` is empty (the default), every helper acts as a no-op. This keeps local development friction-free; you only set the keys when you actually want bot protection on a particular form.

## Adding Turnstile to a new public form

1. **Render the widget** in the form template:

   ```rust
   format!(
       r#"<form method="post" action="/contact">
            <label for="msg">Message</label>
            <textarea id="msg" name="msg" required></textarea>
            {widget}
            <button type="submit">Send</button>
          </form>"#,
       widget = auth::turnstile::widget_html(env)
   )
   ```

2. **Include the script tag** in the page head (or once per page that contains a widget):

   ```rust
   format!(
       r#"<head>...{script}</head>"#,
       script = auth::turnstile::script_tag_html(env)
   )
   ```

3. **Verify in the handler** before performing any side effects:

   ```rust
   async fn handle_contact(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
       let form = req.form_data().await?;
       let token = form
           .get("cf-turnstile-response")
           .and_then(|v| v.as_string())
           .unwrap_or_default();
       let ip = req.headers().get("CF-Connecting-IP")?;
       auth::turnstile::verify_token(&ctx.env, &token, ip.as_deref()).await?;
       // ... safe to commit side effects from here ...
   }
   ```

## Configuration

In `crates/gateway/wrangler.toml`:

```toml
[vars]
TURNSTILE_SITE_KEY = ""   # public site key from the Cloudflare dashboard, or empty to disable
```

Register the matching secret on the Gateway:

```bash
cd crates/gateway
wrangler secret put TURNSTILE_SECRET_KEY
```

The secret is only consulted when the site key is non-empty, so it can be left unset during development.

## Testing

The pure helpers (`build_form_body`, `html_escape`, response parsing) have unit tests in the same file. The HTTP-bound `verify_token` is covered indirectly by the response-parsing tests; full integration testing requires a live Worker runtime and is out of scope for 0.1.0.
