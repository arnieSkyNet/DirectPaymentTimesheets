#[cfg(test)]
mod current_keychain_acceptance {
    use super::*;
    #[test]
    fn native_token_value_rotation() {
        assert_eq!(std::env::consts::OS, "macos");
        assert_eq!(
            std::env::var("DPT_MACOS_CURRENT_ACCEPTANCE").as_deref(),
            Ok("1")
        );
        let ns = uuid::Uuid::new_v4().to_string();
        let store = crate::smtp_credentials::NativeAcceptanceStore::new(&ns);
        let mut e = EmailConfig::default();
        e.smtp_transport = "SMTP Server".into();
        e.smtp_authentication = SmtpAuthentication::MicrosoftOAuth2;
        e.smtp_host = "smtp-mail.outlook.com".into();
        e.smtp_port = 587;
        e.smtp_security = Some(SmtpSecurity::RequiredStarttls);
        e.smtp_username = "synthetic@example.invalid".into();
        e.microsoft_client_id = "12345678-1234-4234-8234-123456789abc".into();
        let ctx = Context::from_email(&e).unwrap();
        let id = format!("oauth-v1-{}", uuid::Uuid::new_v4().simple());
        let mut t = Tokens {
            context: ctx.clone(),
            access: "MACOS_SYNTHETIC_ACCESS_OLD".into(),
            refresh: "MACOS_SYNTHETIC_REFRESH_OLD".into(),
            expires_at: 1000,
        };
        write(&store, &id, &t).unwrap();
        let first: Manifest = serde_json::from_str(&store.get(&id).unwrap().unwrap()).unwrap();
        assert_eq!(read(&store, &id, &ctx).unwrap().access, t.access);
        t.access = "MACOS_SYNTHETIC_ACCESS_NEW".into();
        t.refresh = "MACOS_SYNTHETIC_REFRESH_NEW".into();
        t.expires_at = 2000;
        write(&store, &id, &t).unwrap();
        let second: Manifest = serde_json::from_str(&store.get(&id).unwrap().unwrap()).unwrap();
        assert_ne!(first.generation, second.generation);
        let got = read(&store, &id, &ctx).unwrap();
        assert_eq!(got.access, t.access);
        assert_eq!(got.refresh, t.refresh);
        e.microsoft_credential_ref = Some(id.clone());
        cleanup_with(&store, &e).unwrap();
        assert!(read(&store, &id, &ctx).is_err());
        assert!(write(&store, &id, &t).is_err());
        for m in [first, second] {
            for part in 0..m.parts {
                assert!(store
                    .get(&format!("oauth-part-v1-{}-{part}", m.generation))
                    .unwrap()
                    .is_none());
            }
        }
        drop(store);
        println!("PASS native changed access/refresh token values, verified generation rotation, indexed cleanup and revoked-reference rejection; no HTTP");
    }
}
