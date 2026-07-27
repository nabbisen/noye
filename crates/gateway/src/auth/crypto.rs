//! Wrapper around the Web Crypto API (`globalThis.crypto`, `globalThis.crypto.subtle`).
//!
//! Native crypto crates such as `ring` are not available in the Workers
//! environment, so we call into the runtime's SubtleCrypto through JS
//! bindings. For transparency we avoid the high-level web-sys bindings and
//! use direct `js_sys::Reflect` calls, consistent with the TCP/SMTP modules.
//!
//! The functionality is organized into four submodules:
//!
//! - [`random`] - cryptographically random bytes (PKCE verifier, state, nonce, session IDs)
//! - [`digest`] - SHA-256 (used for PKCE S256 challenge derivation)
//! - [`jwt_verify`] - JWT signature verification using a JWK public key
//! - [`base64url`] - Base64URL encode/decode used throughout the OIDC flow

pub mod base64url;
pub mod digest;
pub mod jwt_verify;
pub mod random;

pub use base64url::{base64url_decode, base64url_encode};
pub use digest::sha256;
pub use jwt_verify::verify_jwt_signature;
pub use random::random_bytes;
