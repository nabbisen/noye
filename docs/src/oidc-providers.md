# OIDC providers

Noye is provider-agnostic: any IdP that implements [OpenID Connect Discovery 1.0](https://openid.net/specs/openid-connect-discovery-1_0.html) and Authorization Code Flow with PKCE will work. Only the issuer URL changes between providers.

## Issuer URL examples

| Provider | `OIDC_ISSUER_URL` |
|---|---|
| Google | `https://accounts.google.com` |
| Microsoft Entra ID | `https://login.microsoftonline.com/{tenant-id}/v2.0` |
| Okta | `https://{tenant}.okta.com/oauth2/default` |
| Auth0 | `https://{tenant}.auth0.com/` |
| Keycloak | `https://{host}/realms/{realm}` |
| AWS Cognito | `https://cognito-idp.{region}.amazonaws.com/{userPoolId}` |

For each provider, verify that `{issuer}/.well-known/openid-configuration` returns valid JSON before deploying. Noye's discovery step will refuse to start if the document cannot be fetched or parsed.

## Required scopes

Configure your OAuth client to grant at least:

- `openid`
- `email` (so we can match against the `users` table)
- `profile` (so we can populate the display name)

Other scopes are accepted but ignored.

## Supported signature algorithms

Noye verifies ID Tokens through the Web Crypto API. The following `alg` values are supported:

- `RS256`, `RS384`, `RS512` (RSASSA-PKCS1-v1_5)
- `PS256` (RSA-PSS with SHA-256)
- `ES256`, `ES384` (ECDSA)

The runtime selects the correct algorithm from the JWT header automatically.

## End-session endpoint

If the IdP advertises an `end_session_endpoint` in its discovery document, Noye redirects the user there on logout. Otherwise the user lands on the Gateway's home page after the session cookie is cleared.

## Choosing a provider for self-hosted deployments

Keycloak is the most common self-hosted choice and works well with Noye. Each Keycloak realm maps to one `OIDC_ISSUER_URL`; switching realms is a configuration change.
