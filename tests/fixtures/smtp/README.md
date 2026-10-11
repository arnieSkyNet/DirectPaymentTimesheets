# Synthetic SMTP test identity

`ca.pem` and `server.p12` are disposable test credentials, unrelated to any real
server or account. The PKCS#12 password is `synthetic-test-only`; its certificate
has DNS SAN `localhost`, is signed by the test CA, and has a long test-only lifetime.
Tests add this root only to their own TLS parameters. Production uses native
platform trust with default certificate and hostname validation. Never install
this CA or identity in production trust stores. Private CA signing keys are not
retained in the repository. Fixtures are used only by loopback SMTP tests.
