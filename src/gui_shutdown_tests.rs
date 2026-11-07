//! Headless egui and synthetic worker tests. No application window or SMTP send.
use super::*;

fn dirty_app() -> (crate::private_storage::PrivateTempDir, DirectPaymentApp) {
    let (root, application, screen) =
        crate::payroll_timesheet_screen::tests::dirty_preparation_fixture();
    let mut app = DirectPaymentApp::new(application);
    app.payroll_timesheet_screen = screen;
    (root, app)
}

fn fingerprint(app: &DirectPaymentApp) -> String {
    crate::database_recovery::fingerprint(
        &app.application
            .payroll_timesheet_email_repository
            .connection,
    )
    .unwrap()
}

#[test]
fn restart_close_requires_explicit_discard_on_every_screen_without_saving() {
    for screen in [
        ActiveScreen::Dashboard,
        ActiveScreen::PayrollTimesheet,
        ActiveScreen::EmailSettings,
    ] {
        let (_root, mut app) = dirty_app();
        app.active_screen = screen;
        app.restart_required_message = Some("Restored; restart required".into());
        let before = fingerprint(&app);
        let ctx = egui::Context::default();
        let output = ctx.run(Default::default(), |ctx| {
            assert!(!app.request_restart_close(ctx, true, false));
        });
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        assert!(app.restart_close_confirmation);
        assert!(app.payroll_timesheet_screen.has_unsaved_changes());
        let output = ctx.run(Default::default(), |ctx| {
            assert!(app.confirm_restart_close(ctx));
        });
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close));
        // Confirmation is not a stale Save or a mutation of the restored DB.
        assert!(app.payroll_timesheet_screen.has_unsaved_changes());
        assert_eq!(fingerprint(&app), before);
    }
}

#[test]
fn restart_panel_cancels_old_navigation_and_never_writes_the_database() {
    let (_root, mut app) = dirty_app();
    app.restart_required_message = Some("Restored; restart required".into());
    app.pending_preparation_navigation = Some((
        ActiveScreen::Dashboard,
        app.operational_payroll_period.clone(),
        true,
    ));
    let before = fingerprint(&app);
    let ctx = egui::Context::default();
    let _output = ctx.run(Default::default(), |ctx| {
        assert!(app.show_restart_required(ctx, false));
    });
    assert!(app.pending_preparation_navigation.is_none());
    assert!(app.payroll_timesheet_screen.has_unsaved_changes());
    assert_eq!(fingerprint(&app), before);
}

#[test]
fn database_failure_preserves_recovery_guidance_and_latches_only_once() {
    let (_root, application) = crate::payroll_timesheet_screen::tests::test_application();
    let mut app = DirectPaymentApp::new(application);
    app.restart_required_message = Some("Verified safety backup retained".into());
    app.restart_required_paths = vec![std::path::PathBuf::from("synthetic-backup")];
    let before = fingerprint(&app);
    app.apply_database_failure(Some(
        crate::database_failure::Failure::BoundaryCloseUnconfirmed,
    ));
    let first = app.restart_required_message.clone();
    app.apply_database_failure(Some(crate::database_failure::Failure::CloseUnwound));
    assert_eq!(app.restart_required_message, first);
    assert!(first.unwrap().contains("Verified safety backup retained"));
    assert_eq!(
        app.restart_required_paths,
        vec![std::path::PathBuf::from("synthetic-backup")]
    );
    assert_eq!(fingerprint(&app), before);
}

#[test]
fn restart_and_final_normal_close_wait_for_the_existing_synthetic_worker() {
    let (_root, application) = crate::payroll_timesheet_screen::tests::test_application();
    let mut app = DirectPaymentApp::new(application);
    app.restart_required_message = Some("Restart required".into());
    let (release, wait) = std::sync::mpsc::channel();
    let (wake, notification) = std::sync::mpsc::channel();
    app.smtp_test
        .start_mock(
            move || {
                let _ = wait.recv();
                Ok(())
            },
            move || {
                let _ = wake.send(());
            },
        )
        .unwrap();
    let ctx = egui::Context::default();
    let output = ctx.run(Default::default(), |ctx| {
        assert!(!app.confirm_restart_close(ctx));
        // Covers final close after an earlier save/discard confirmation as well.
        assert!(!app.complete_close_request(ctx, true));
    });
    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(commands.contains(&egui::ViewportCommand::CancelClose));
    assert!(!commands.contains(&egui::ViewportCommand::Close));
    release.send(()).unwrap();
    notification
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    app.poll_smtp_test();
    let output = ctx.run(Default::default(), |ctx| {
        assert!(app.request_restart_close(ctx, true, false));
    });
    assert!(output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::Close));
}

#[test]
fn normal_close_request_has_no_effect_without_a_request() {
    let (_root, application) = crate::payroll_timesheet_screen::tests::test_application();
    let mut app = DirectPaymentApp::new(application);
    let ctx = egui::Context::default();
    let output = ctx.run(Default::default(), |ctx| {
        assert!(!app.complete_close_request(ctx, false));
        assert!(!app.show_restart_required(ctx, false));
    });
    assert!(!output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::Close));
}

#[test]
fn normal_close_from_another_screen_defers_to_unsaved_preparation() {
    for screen in [ActiveScreen::Dashboard, ActiveScreen::EmailSettings] {
        let (_root, mut app) = dirty_app();
        app.active_screen = screen;
        let before = fingerprint(&app);
        let ctx = egui::Context::default();
        let output = ctx.run(Default::default(), |ctx| {
            assert!(!app.complete_close_request(ctx, true));
        });
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        assert_eq!(app.active_screen, ActiveScreen::PayrollTimesheet);
        assert!(matches!(
            app.pending_preparation_navigation,
            Some((_, _, true))
        ));
        assert!(!app.finish_preparation_navigation(false));
        assert!(app.payroll_timesheet_screen.has_unsaved_changes());
        assert_eq!(fingerprint(&app), before);
    }
}

#[test]
fn blocked_discard_close_does_not_leave_reusable_close_authorisation() {
    let (_root, mut app) = dirty_app();
    app.restart_required_message = Some("Restart required".into());
    let (release, wait) = std::sync::mpsc::channel();
    let (wake, notification) = std::sync::mpsc::channel();
    app.smtp_test
        .start_mock(
            move || {
                let _ = wait.recv();
                Ok(())
            },
            move || {
                let _ = wake.send(());
            },
        )
        .unwrap();
    let ctx = egui::Context::default();
    let _output = ctx.run(Default::default(), |ctx| {
        assert!(!app.confirm_restart_close(ctx));
    });
    release.send(()).unwrap();
    notification
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    app.poll_smtp_test();
    let output = ctx.run(Default::default(), |ctx| {
        assert!(!app.request_restart_close(ctx, true, false));
    });
    assert!(app.restart_close_confirmation);
    assert!(!output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::Close));
    assert!(app.payroll_timesheet_screen.has_unsaved_changes());
}

#[test]
fn confirmed_restart_close_allows_exactly_the_following_egui_close_event() {
    let (_root, mut app) = dirty_app();
    app.restart_required_message = Some("Restart required".into());
    let ctx = egui::Context::default();
    let _output = ctx.run(Default::default(), |ctx| {
        assert!(app.confirm_restart_close(ctx));
    });
    assert!(app.restart_close_authorized);
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let output = ctx.run(input, |ctx| {
        let close = ctx.input(|input| input.viewport().close_requested());
        assert!(close);
        assert!(app.show_restart_required(ctx, close));
    });
    // This is the same cancellation predicate used by pinned eframe. A Close
    // command alone is insufficient if this input frame also sends CancelClose.
    assert!(!output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::CancelClose));
    assert!(!app.restart_close_authorized);
}
