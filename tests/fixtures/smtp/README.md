# Synthetic SMTP test identity

`ca.pem` and `server.p12` are disposable test credentials, unrelated to any real
server or account. The PKCS#12 password is `synthetic-test-only`; its certificate
has DNS SAN `localhost` and is signed by the test CA. Its validity is limited to
365 days: **2026-10-11 17:07:42 UTC through 2027-10-11 17:07:42 UTC**.
It uses RSA-2048, SHA-256, critical `CA:FALSE`, critical digital-signature and
key-encipherment key usage, and server-authentication extended key usage.
Tests add this root only to their own TLS parameters. Production uses native
platform trust with default certificate and hostname validation. Never install
this CA or identity in production trust stores. Private CA signing keys are not
retained in the repository. Fixtures are used only by loopback SMTP tests.

## Generation and renewal

The embedded PKCS#12 contains the synthetic server private key, leaf certificate
and matching CA certificate. The PEM file supplies that same CA to the test TLS
parameters. Renew both files together before the leaf expires; never increase
leaf validity beyond 365 days to avoid Apple's TLS validity rejection.

The identity was generated offline with OpenSSL: a fresh RSA-2048 CA (ten-year
validity, critical CA basic constraints and certificate-signing/CRL-signing key
usage), a fresh RSA-2048 localhost CSR, and a SHA-256 CA-signed leaf with
`-days 365` and the extensions above. `openssl pkcs12 -export` packages the leaf,
key and CA with the existing synthetic password. Temporary signing keys and CSRs
were discarded; no CA private key is retained. The CA was replaced because the
previous signing key was deliberately not stored. Tests still add only the
matching synthetic root to their individual TLS parameters.

A static certificate cannot remain valid indefinitely. A future test-only runtime
generator could create an isolated ephemeral CA/keypair once per test process,
anchor validity to the current UTC date (one day of backdating and 364 days forward),
and inject the resulting CA and PKCS#12 through the existing fixture interfaces.
Use an injected clock for generation-policy tests, retain the same hostname and
extensions, and leave platform trust stores untouched. Random key material is
appropriate; the validity policy and test assertions should be deterministic.
This approach is proposed, not implemented here; the current fixture requires
renewal before October 2027 and CI clocks within its validity window.
