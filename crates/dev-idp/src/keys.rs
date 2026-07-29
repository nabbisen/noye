//! RSA keypair management for the dev IdP.
//!
//! A fresh 2048-bit RSA keypair is generated on every process start.
//! Persistence is intentionally avoided: this is a development-only
//! tool and a leaked private key on disk is more dangerous than the
//! cost of regenerating one (which is one-time on startup).
//!
//! The key is exposed publicly via `/jwks` in JWK form, and used
//! privately by `jwt.rs` to sign ID Tokens.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde_json::json;

pub struct KeyMaterial {
    pub private: RsaPrivateKey,
    pub kid: String,
}

impl KeyMaterial {
    /// Generate a fresh 2048-bit RSA keypair.
    pub fn fresh() -> anyhow::Result<Self> {
        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048)?;
        let kid = format!("dev-idp-{}", chrono::Utc::now().timestamp());
        Ok(Self { private, kid })
    }

    pub fn public(&self) -> RsaPublicKey {
        RsaPublicKey::from(&self.private)
    }

    /// Public key in JWK form, suitable for the JWKS endpoint.
    ///
    /// The `n` and `e` are encoded as base64url without padding per
    /// RFC 7518 §6.3.1, and `use=sig` + `alg=RS256` advertise that this
    /// key is for signature verification with RS256.
    pub fn to_jwk(&self) -> serde_json::Value {
        let pubkey = self.public();
        let n = URL_SAFE_NO_PAD.encode(pubkey.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(pubkey.e().to_bytes_be());
        json!({
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": self.kid,
            "n": n,
            "e": e,
        })
    }

    /// Convenience: emit a complete JWKS document.
    pub fn to_jwks(&self) -> serde_json::Value {
        json!({ "keys": [self.to_jwk()] })
    }

    /// PEM dump of the public key, useful when debugging by hand.
    #[allow(dead_code)]
    pub fn public_key_pem(&self) -> anyhow::Result<String> {
        let pubkey = self.public();
        Ok(pubkey.to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)?)
    }
}
