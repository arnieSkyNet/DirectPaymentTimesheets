//! Synthetic NTFS fixtures only. Assertions express the required security
//! behaviour, including known gaps; they must not be ignored to obtain green CI.
use super::*;

fn powershell(path: &Path, script: &str) -> String {
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("DPT_SYNTHETIC_PATH", path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "synthetic_fixture.powershell: code={:?}; class=command_failure",
        output.status.code()
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn require_ntfs(root: &Path) {
    assert_eq!(powershell(root, "$ErrorActionPreference='Stop'; (Get-Volume -FilePath $env:DPT_SYNTHETIC_PATH).FileSystem"), "NTFS", "Native security fixtures require NTFS; no silent skip");
}

fn broad_fixture(path: &Path) {
    set_fixture_dacl(path, "D:P(A;OICI;FA;;;WD)");
}

// Set only the DACL on new synthetic directories. Get-Acl/Set-Acl also tries
// to set audit security on Molly and fails without SeSecurityPrivilege.
fn set_fixture_dacl(path: &Path, sddl: &str) {
    powershell(
        path,
        &format!(r#"
        $ErrorActionPreference='Stop'
        $acl = [Security.AccessControl.DirectorySecurity]::new()
        $acl.SetSecurityDescriptorSddlForm('{sddl}',[Security.AccessControl.AccessControlSections]::Access)
        [IO.Directory]::SetAccessControl($env:DPT_SYNTHETIC_PATH,$acl)
    "#),
    );
}

#[test]
fn phase1_windows_actual_new_object_owners_match_current_token() {
    let root = tempdir().unwrap();
    require_ntfs(root.path());
    let file = root.path().join("synthetic-file");
    write_new(&file, b"synthetic bytes").unwrap();
    for path in [root.path(), file.as_path()] {
        // Query the actual OS owner, rather than interpreting an OW ACE.
        assert_eq!(
            powershell(
                path,
                r#"
            $ErrorActionPreference='Stop'
            $owner = (Get-Acl -LiteralPath $env:DPT_SYNTHETIC_PATH).GetOwner([System.Security.Principal.SecurityIdentifier])
            $current = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
            $owner.Equals($current)
        "#
            ),
            "True"
        );
        assert!(platform::is_private(path).unwrap());
    }
}

#[test]
fn phase1_windows_inherited_acl_is_detected_and_private_creation_excludes_it() {
    let root = tempdir().unwrap();
    require_ntfs(root.path());
    let shared = root.path().join("synthetic-shared");
    create_private_dir(&shared).unwrap();
    broad_fixture(&shared);
    let inherited = shared.join("inherited");
    fs::write(&inherited, b"synthetic inherited bytes").unwrap();
    assert!(!platform::is_private(&inherited).unwrap());
    let private = shared.join("private");
    write_new(&private, b"synthetic private bytes").unwrap();
    assert!(platform::is_private(&private).unwrap());
    assert_eq!(fs::read(inherited).unwrap(), b"synthetic inherited bytes");
}

#[test]
fn phase1_windows_database_refuses_broad_ancestor_without_creating_files() {
    let root = tempdir().unwrap();
    require_ntfs(root.path());
    let shared = root.path().join("synthetic-shared");
    create_private_dir(&shared).unwrap();
    let child = shared.join("protected-child");
    create_private_dir(&child).unwrap();
    // A failure in system ancestry must fail this control, not satisfy the negative.
    drop(platform::database_ancestor_guard(&child.join("positive.sqlite")).unwrap());
    broad_fixture(&shared);
    assert!(platform::is_private(&child).unwrap());
    let database = child.join("synthetic.sqlite");
    let result = prepare_database(&database);
    let created = database.exists();
    let error = result.unwrap_err().to_string();
    assert!(error.contains("class=unsafe_ancestor; reason=untrusted_allow_ace"));
    assert!(error.contains("role=existing_ancestor"));
    assert!(!created, "Refusal must precede SQLite file reservation");
}

#[test]
fn phase1_windows_read_only_staging_policy_survives_tempfile_keep() {
    let root = tempdir().unwrap();
    require_ntfs(root.path());
    let original = root.path().join("original");
    write_new(&original, b"original synthetic bytes").unwrap();
    let mut permissions = fs::metadata(&original).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&original, permissions).unwrap();
    let mut pending = tempfile_in(root.path()).unwrap();
    pending.write_all(b"replacement synthetic bytes").unwrap();
    preserve_access(pending.path(), &original).unwrap();
    assert!(fs::metadata(pending.path())
        .unwrap()
        .permissions()
        .readonly());
    let (file, kept) = keep_tempfile(pending).unwrap();
    let retained_readonly = file.metadata().unwrap().permissions().readonly();
    assert!(platform::is_private(&kept).unwrap());
    assert_eq!(fs::read(&original).unwrap(), b"original synthetic bytes");
    // Clean up only the synthetic files, including on future successful runs.
    for path in [&original, &kept] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap();
    }
    drop(file);
    fs::remove_file(kept).unwrap();
    assert!(
        retained_readonly,
        "Known gap: tempfile keep resets the copied read-only attribute"
    );
}

#[test]
fn phase1_windows_junction_paths_are_refused_and_targets_retained() {
    let root = tempdir().unwrap();
    require_ntfs(root.path());
    let target = root.path().join("target");
    create_private_dir(&target).unwrap();
    write_new(&target.join("retained"), b"synthetic retained bytes").unwrap();
    let junction = root.path().join("junction");
    let result = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "NTFS junction fixture creation failed; not a skipped security check"
    );
    let refused = create_new(&junction.join("unexpected"));
    let unchanged = fs::read(target.join("retained")).unwrap();
    let absent = !target.join("unexpected").exists();
    fs::remove_dir(&junction).unwrap();
    assert!(refused.is_err());
    assert!(absent);
    assert_eq!(unchanged, b"synthetic retained bytes");
}

#[test]
fn phase1_windows_read_only_policy_survives_tempfile_publication() {
    let root = tempdir().unwrap();
    require_ntfs(root.path());
    let original = root.path().join("original");
    write_new(&original, b"synthetic original").unwrap();
    let mut permissions = fs::metadata(&original).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&original, permissions).unwrap();
    let mut pending = tempfile_in(root.path()).unwrap();
    pending.write_all(b"synthetic filing copy").unwrap();
    preserve_access(pending.path(), &original).unwrap();
    let published = root.path().join("published-copy");
    let file = publish_tempfile(pending, &published, false).unwrap();
    let retained_readonly = file.metadata().unwrap().permissions().readonly();
    assert!(platform::is_private(&published).unwrap());
    assert_eq!(fs::read(&original).unwrap(), b"synthetic original");
    for path in [&original, &published] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).unwrap();
    }
    drop(file);
    assert!(
        retained_readonly,
        "Known gap: tempfile persist resets intended read-only attributes"
    );
}

#[test]
fn phase1_windows_refused_read_only_replacement_retains_original() {
    let root = tempdir().unwrap();
    require_ntfs(root.path());
    let original = root.path().join("original");
    write_new(&original, b"synthetic original").unwrap();
    let mut permissions = fs::metadata(&original).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&original, permissions).unwrap();
    let result = write_atomic(&original, b"synthetic replacement");
    let retained = fs::read(&original).unwrap();
    let readonly = fs::metadata(&original).unwrap().permissions().readonly();
    assert!(platform::is_private(&original).unwrap());
    let mut permissions = fs::metadata(&original).unwrap().permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&original, permissions).unwrap();
    assert!(
        result.is_err(),
        "Read-only destination must refuse replacement"
    );
    assert!(readonly);
    assert_eq!(retained, b"synthetic original");
}

#[test]
fn phase1_windows_owner_rights_does_not_replace_actual_owner_validation() {
    let root = tempdir().unwrap();
    require_ntfs(root.path());
    let child = root.path().join("synthetic-other-owner");
    create_private_dir(&child).unwrap();
    let before = platform::TestSecurityPolicy::capture(&child).unwrap();
    assert!(platform::is_private(&child).unwrap());
    platform::test_owner_rights_cannot_authorize_foreign_owner();
    assert!(before.matches(&child).unwrap());
    assert!(child.exists());
}

#[test]
fn windows_publication_existing_absent_and_failure_preserve_contents() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempdir().unwrap();
    let path = root.path().join("result");
    write_atomic(&path, b"first").unwrap();
    let old = open_read(&path).unwrap(); // Delete-shared identity anchor remains held.
    let old_id = same_file::Handle::from_file(old).unwrap();
    write_atomic(&path, b"second").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second");
    assert!(old_id != same_file::Handle::from_path(&path).unwrap());
    let blocker = fs::OpenOptions::new().read(true).share_mode(3).open(&path).unwrap();
    assert!(write_atomic(&path, b"must fail").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"second");
    drop(blocker);
    assert!(platform::is_private(&path).unwrap());
}

#[test]
fn windows_publication_trusted_atomic_renderer_establishes_completed_identity() {
    let root = tempdir().unwrap();
    let destination = root.path().join("final.pdf");
    let guard = platform::publication_guard(&destination, &destination).unwrap();
    let pending = tempfile_in(root.path()).unwrap();
    let original_id = same_file::Handle::from_file(pending.as_file().try_clone().unwrap()).unwrap();
    write_atomic(pending.path(), b"rendered bytes").unwrap();
    assert!(original_id != same_file::Handle::from_path(pending.path()).unwrap());
    let completed = guard.rendered_stage(pending).unwrap();
    publish_tempfile(completed, &destination, false).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"rendered bytes");
}

#[test]
fn windows_publication_identity_change_before_validation_is_refused() {
    let root = tempdir().unwrap();
    let path = root.path().join("target");
    write_new(&path, b"original").unwrap();
    let original = path.with_extension("original");
    let target = path.clone(); let moved = original.clone();
    let result = platform::with_publication_interposition(false, move || {
        fs::rename(&target, &moved).unwrap();
        write_new(&target, b"substitute").unwrap();
    }, || write_atomic(&path, b"staged"));
    assert!(result.unwrap_err().to_string().contains("storage.write_atomic.publish"));
    assert_eq!(fs::read(&path).unwrap(), b"substitute");
    assert_eq!(fs::read(&original).unwrap(), b"original");
}

#[test]
fn windows_publication_unsafe_destination_cannot_expose_stage_bytes() {
    let root = tempdir().unwrap();
    let target = root.path().join("target");
    write_new(&target, b"original").unwrap();
    powershell(&target, r#"
        $ErrorActionPreference='Stop'
        $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
        $acl=[Security.AccessControl.FileSecurity]::new()
        $acl.SetSecurityDescriptorSddlForm("D:P(A;;FA;;;$sid)(A;;FA;;;SY)(A;;FA;;;WD)",[Security.AccessControl.AccessControlSections]::Access)
        [IO.File]::SetAccessControl($env:DPT_SYNTHETIC_PATH,$acl)
    "#);
    let before = platform::TestSecurityPolicy::capture(&target).unwrap();
    let pending = tempfile_in(root.path()).unwrap();
    let stage = pending.path().to_path_buf();
    assert!(preserve_access(&stage, &target).is_err());
    assert!(platform::is_private(&stage).unwrap());
    assert_eq!(fs::read(&target).unwrap(), b"original");
    assert!(write_atomic(&target, b"new secret").is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2, "No extra stage may be allocated");
    assert!(before.matches(&target).unwrap());
    assert_eq!(fs::read(&target).unwrap(), b"original");
}

#[test]
fn windows_publication_absent_collision_is_never_clobbered() {
    let root = tempdir().unwrap();
    let path = root.path().join("target"); let target = path.clone();
    let result = platform::with_publication_interposition(true, move || {
        write_new(&target, b"winner").unwrap();
    }, || write_atomic(&path, b"staged"));
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), b"winner");
}

#[test]
fn windows_publication_excluded_same_user_after_check_is_not_compare_and_swap() {
    let root = tempdir().unwrap();
    let path = root.path().join("target");
    write_new(&path, b"original").unwrap();
    let moved = path.with_extension("original"); let old = moved.clone(); let target = path.clone();
    platform::with_publication_interposition(true, move || {
        // Deliberately ignores the cooperating-writer mutex. This authority is
        // excluded, and the regression records the residual race explicitly.
        fs::rename(&target, &old).unwrap();
        write_new(&target, b"substitute").unwrap();
    }, || write_atomic(&path, b"published")).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"published");
    assert_eq!(fs::read(&moved).unwrap(), b"original");
}

#[test]
fn windows_publication_unsafe_ancestry_and_stage_substitution_are_refused() {
    let root = tempdir().unwrap();
    let target = root.path().join("target");
    let mut pending = tempfile_in(root.path()).unwrap();
    pending.write_all(b"stage").unwrap();
    let source = pending.path().to_path_buf();
    let moved = source.with_extension("original");
    fs::rename(&source, &moved).unwrap();
    write_new(&source, b"substitute").unwrap();
    let failure = publish_tempfile(pending, &target, false).unwrap_err();
    assert!(failure.error.to_string().contains("stage_identity_changed"));
    drop(failure);
    assert_eq!(fs::read(&source).unwrap(), b"substitute"); // Failure never deletes the wrong name.
    assert_eq!(fs::read(&moved).unwrap(), b"stage");
    let unsafe_parent = root.path().join("unsafe");
    create_private_dir(&unsafe_parent).unwrap();
    broad_fixture(&unsafe_parent); // Only a fresh synthetic fixture DACL.
    let refused = unsafe_parent.join("refused");
    assert!(write_atomic(&refused, b"refused").is_err());
    assert!(!refused.exists());
}

#[test]
#[ignore = "Child harness only; executed explicitly by windows_publication_cooperating_processes_serialize"]
fn windows_publication_cooperating_child() {
    let root = PathBuf::from(std::env::var_os("DPT_PUBLICATION_CHILD_FIXTURE").expect("Synthetic child fixture required"));
    assert!(root.file_name().unwrap().to_string_lossy().starts_with(".dpt-private-"));
    assert_eq!(root.parent().unwrap().canonicalize().unwrap(), std::env::temp_dir().canonicalize().unwrap());
    check_path(&root).unwrap();
    assert!(platform::is_private(&root).unwrap());
    write_new(&root.join("child-entered"), b"ready").unwrap();
    write_atomic(&root.join("target"), b"child-complete").unwrap();
}

#[test]
fn windows_publication_cooperating_processes_serialize() {
    use std::time::{Duration, Instant};
    let root = tempdir().unwrap();
    let target = root.path().join("target");
    write_new(&target, b"original").unwrap();
    let guard = platform::publication_guard(&target, &target).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "private_storage::windows_verification_tests::windows_publication_cooperating_child", "--ignored", "--test-threads=1"])
        .env("DPT_PUBLICATION_CHILD_FIXTURE", root.path())
        .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while !root.path().join("child-entered").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(root.path().join("child-entered").exists());
    std::thread::sleep(Duration::from_millis(100));
    assert!(child.try_wait().unwrap().is_none());
    assert_eq!(fs::read(&target).unwrap(), b"original");
    drop(guard);
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success(), "Child publication failed: {}", String::from_utf8_lossy(&result.stdout));
    assert_eq!(fs::read(&target).unwrap(), b"child-complete");
}

#[test]
fn phase1_windows_database_refuses_private_parent_without_inheritable_access() {
    let root = tempdir().unwrap();
    let child = root.path().join("synthetic-no-inheritance");
    create_private_dir(&child).unwrap();
    drop(platform::database_ancestor_guard(&child.join("positive.sqlite")).unwrap());
    powershell(
        &child,
        r#"
        $ErrorActionPreference='Stop'
        $acl = [Security.AccessControl.DirectorySecurity]::new()
        $acl.SetAccessRuleProtection($true, $false)
        $current = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
        $system = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
        foreach ($sid in @($current, $system)) {
            $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new(
                $sid, 'FullControl', 'None', 'None', 'Allow'))
        }
        [IO.Directory]::SetAccessControl($env:DPT_SYNTHETIC_PATH,$acl)
    "#,
    );
    assert!(platform::is_private(&child).unwrap());
    let database = child.join("refused.sqlite");
    let error = prepare_database(&database).unwrap_err().to_string();
    assert!(error
        .contains("class=unsafe_sidecar_inheritance; reason=missing_inheritable_current_access"));
    assert!(error.contains("role=immediate_parent"));
    assert!(!database.exists());
}

#[test]
fn phase1_windows_database_refuses_additional_read_only_trustee_without_repair() {
    let root = tempdir().unwrap();
    let parent = root.path().join("synthetic-read-shared-parent");
    create_private_dir(&parent).unwrap();
    drop(platform::database_ancestor_guard(&parent.join("positive.sqlite")).unwrap());
    let current = powershell(&parent, "[Security.Principal.WindowsIdentity]::GetCurrent().User.Value");
    set_fixture_dacl(&parent, &format!("D:P(A;OICI;FA;;;{current})(A;OICI;FA;;;SY)(A;OICI;GR;;;WD)"));
    let policy = platform::TestSecurityPolicy::capture(&parent).unwrap();
    let refused = parent.join("refused.sqlite");
    let error = prepare_database(&refused).unwrap_err().to_string();
    assert!(error.contains("class=unsafe_sidecar_inheritance; reason=untrusted_allow_ace"));
    assert!(error.contains("role=immediate_parent"));
    assert!(!refused.exists());
    assert!(policy.matches(&parent).unwrap());
}

#[test]
fn phase1_windows_ancestor_guard_blocks_rename_while_held() {
    let root = tempdir().unwrap();
    let child = root.path().join("synthetic-held-parent");
    create_private_dir(&child).unwrap();
    let database = child.join("live.sqlite");
    let guard = platform::database_ancestor_guard(&database).unwrap();
    let moved = root.path().join("moved-parent");
    assert!(fs::rename(&child, &moved).is_err());
    assert!(child.is_dir() && !moved.exists());
    drop(guard);
    fs::rename(&child, &moved).unwrap();
}

#[test]
fn phase1_windows_guard_blocks_ancestor_substitution_and_keeps_identities() {
    let root = tempdir().unwrap();
    let ancestor = root.path().join("synthetic-ancestor");
    let parent = ancestor.join("private-parent");
    create_private_dir(&ancestor).unwrap();
    create_private_dir(&parent).unwrap();
    let database = parent.join("synthetic.sqlite");
    write_new(&database, b"synthetic original").unwrap();
    // No observation handles may remain open and accidentally block rename.
    let ancestor_id = platform::TestFileIdentity::capture(&ancestor).unwrap();
    let parent_id = platform::TestFileIdentity::capture(&parent).unwrap();
    let database_id = platform::TestFileIdentity::capture(&database).unwrap();
    let guard = platform::database_ancestor_guard(&database).unwrap();
    for path in [&ancestor, &parent] {
        let moved = path.with_extension("moved");
        assert!(fs::rename(path, &moved).is_err(), "Held ancestry must not move");
        assert!(!moved.exists());
        assert!(fs::create_dir(path).is_err(), "Original name must remain occupied");
    }
    assert!(ancestor_id == platform::TestFileIdentity::capture(&ancestor).unwrap());
    assert!(parent_id == platform::TestFileIdentity::capture(&parent).unwrap());
    assert!(database_id == platform::TestFileIdentity::capture(&database).unwrap());
    assert_eq!(fs::read(&database).unwrap(), b"synthetic original");
    drop(guard);
    // Positive control: releasing the guard permits rename and substitution.
    let moved = ancestor.with_extension("moved");
    fs::rename(&ancestor, &moved).unwrap();
    create_private_dir(&ancestor).unwrap();
    assert!(ancestor_id != platform::TestFileIdentity::capture(&ancestor).unwrap());
    assert!(ancestor_id == platform::TestFileIdentity::capture(&moved).unwrap());
}

#[test]
fn phase1_windows_blocking_handle_refuses_publication_and_retains_original() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempdir().unwrap();
    let original = root.path().join("synthetic-existing");
    write_new(&original, b"synthetic original").unwrap();
    let blocker = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&original)
        .unwrap();
    assert!(write_atomic(&original, b"synthetic replacement").is_err());
    assert_eq!(fs::read(&original).unwrap(), b"synthetic original");
    assert!(platform::is_private(&original).unwrap());
    drop(blocker);
}

#[test]
fn phase1_windows_library_temporary_attribute_is_refused_without_reset() {
    let root = tempdir().unwrap();
    let mut pending = tempfile::NamedTempFile::new_in(root.path()).unwrap();
    pending.write_all(b"synthetic temporary").unwrap();
    let before = pending.as_file().metadata().unwrap().permissions();
    let error = keep_tempfile(pending).unwrap_err();
    assert_eq!(
        before,
        error.file.as_file().metadata().unwrap().permissions()
    );
    let target = root.path().join("unexpected");
    let error = publish_tempfile(error.file, &target, false).unwrap_err();
    assert!(!target.exists());
    assert_eq!(fs::read(error.file.path()).unwrap(), b"synthetic temporary");
}

#[test]
fn phase1_windows_positive_database_ancestor_admission() {
    let root = tempdir().unwrap();
    require_ntfs(root.path());
    let parent = root.path().join("synthetic-private-parent");
    create_private_dir(&parent).unwrap();
    let database = parent.join("positive.sqlite");
    // Only this synthetic ancestry walk may consume the single observation.
    // A foreign root remains refused; diagnostics never turn this control green.
    let guard =
        platform::with_synthetic_root_observation(|| platform::database_ancestor_guard(&database));
    drop(guard.unwrap());
    prepare_database(&database).unwrap();
    assert!(platform::is_private(&database).unwrap());
}

// Explicit Win32 denial classes; these do not prove the underlying cause.
fn plausible_publication_refusal(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32 | 33 | 1314))
}

#[test]
fn phase1_windows_publication_refusal_requires_structured_denial() {
    for code in [5, 32, 33, 1314] {
        assert!(plausible_publication_refusal(
            &std::io::Error::from_raw_os_error(code)
        ));
    }
    // Missing file/path, invalid parameter, unrelated failure and unstructured errors.
    for code in [2, 3, 87, 1, 112] {
        assert!(!plausible_publication_refusal(
            &std::io::Error::from_raw_os_error(code)
        ));
    }
    assert!(!plausible_publication_refusal(&std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "synthetic unstructured refusal"
    )));
}

#[test]
fn phase1_windows_atomic_publication_handle_attribute_matrix() {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    // Two absent-destination controls and six existing-destination cases.
    // Continue after API refusals to expose the complete controlled matrix.
    let mut unexpected = 0;
    for exists in [false, true] {
        for readonly in [false, true] {
            for share in [None, Some(7u32), Some(3u32)] {
                if !exists && share.is_some() {
                    continue; // An absent destination cannot have an open handle.
                }
                let root = tempdir().unwrap();
                require_ntfs(root.path());
                let destination = root.path().join("synthetic-destination");
                if exists {
                    write_new(&destination, b"synthetic original").unwrap();
                    let mut permissions = fs::metadata(&destination).unwrap().permissions();
                    permissions.set_readonly(readonly);
                    fs::set_permissions(&destination, permissions).unwrap();
                }
                let blocker = share.map(|mask| {
                    fs::OpenOptions::new()
                        .access_mode(0x80000000) // Controlled GENERIC_READ.
                        .share_mode(mask)
                        .open(&destination)
                        .unwrap()
                });
                let original_identity = blocker
                    .as_ref()
                    .map(|file| same_file::Handle::from_file(file.try_clone().unwrap()).unwrap());
                let mut pending = tempfile_in(root.path()).unwrap();
                pending.write_all(b"synthetic replacement").unwrap();
                pending.as_file().sync_all().unwrap();
                if exists {
                    preserve_access(pending.path(), &destination).unwrap();
                } else {
                    let mut permissions = pending.as_file().metadata().unwrap().permissions();
                    permissions.set_readonly(readonly);
                    pending.as_file().set_permissions(permissions).unwrap();
                }
                let staged_identity =
                    same_file::Handle::from_file(pending.as_file().try_clone().unwrap()).unwrap();
                let attributes = pending.as_file().metadata().unwrap().file_attributes();
                let destination_attributes = if exists {
                    Some(fs::metadata(&destination).unwrap().file_attributes())
                } else {
                    None
                };
                let staged_policy = platform::TestSecurityPolicy::capture(pending.path()).unwrap();
                let original_policy = if exists {
                    Some(platform::TestSecurityPolicy::capture(&destination).unwrap())
                } else {
                    None
                };
                let outcome = platform::publication_probe(pending.as_file(), pending.path(), &destination, exists);
                eprintln!("windows.synthetic.MoveFileExW: exists={exists}; readonly={readonly}; source_attributes={attributes:#x}; destination_attributes={destination_attributes:?}; destination_access={}; destination_share={share:?}; source_access=0xc0000000; source_share=7; code={:?}; class={:?}",
                    if share.is_some() { 0x80000000u32 } else { 0 },
                    outcome.as_ref().err().and_then(|e| e.raw_os_error()),
                    outcome.as_ref().err().map(|e| e.kind()));
                let expected_success = !exists || (!readonly && share != Some(3));
                let expected_outcome = match &outcome {
                    Ok(()) => expected_success,
                    Err(error) => !expected_success && plausible_publication_refusal(error),
                };
                if !expected_outcome {
                    unexpected += 1;
                }
                if outcome.is_ok() {
                    assert_eq!(fs::read(&destination).unwrap(), b"synthetic replacement");
                    assert!(staged_identity == same_file::Handle::from_path(&destination).unwrap());
                    assert!(!pending.path().exists());
                    assert_eq!(
                        fs::metadata(&destination).unwrap().file_attributes(),
                        attributes
                    );
                    assert!(platform::is_private(&destination).unwrap());
                    assert!(staged_policy.matches(&destination).unwrap());
                    if let Some(policy) = &original_policy {
                        assert!(policy.matches(&destination).unwrap());
                    }
                } else {
                    assert_eq!(fs::read(pending.path()).unwrap(), b"synthetic replacement");
                    assert!(
                        staged_identity == same_file::Handle::from_path(pending.path()).unwrap()
                    );
                    assert_eq!(
                        pending.as_file().metadata().unwrap().file_attributes(),
                        attributes
                    );
                    assert!(platform::is_private(pending.path()).unwrap());
                    assert!(staged_policy.matches(pending.path()).unwrap());
                    if exists {
                        assert!(original_policy
                            .as_ref()
                            .unwrap()
                            .matches(&destination)
                            .unwrap());
                        assert_eq!(fs::read(&destination).unwrap(), b"synthetic original");
                        assert_eq!(
                            Some(fs::metadata(&destination).unwrap().file_attributes()),
                            destination_attributes
                        );
                        assert!(platform::is_private(&destination).unwrap());
                        if let Some(original) = &original_identity {
                            assert!(
                                original == &same_file::Handle::from_path(&destination).unwrap()
                            );
                        }
                    } else {
                        assert!(!destination.exists());
                    }
                }
                // Only synthetic fixture cleanup may reset attributes, after assertions.
                drop(original_identity);
                drop(blocker);
                drop(staged_identity);
                for path in [pending.path(), destination.as_path()] {
                    if path.exists() {
                        let mut permissions = fs::metadata(path).unwrap().permissions();
                        permissions.set_readonly(false);
                        fs::set_permissions(path, permissions).unwrap();
                    }
                }
                if destination.exists() {
                    fs::remove_file(&destination).unwrap();
                }
                // Pending retains cleanup on refusal; success leaves its old name absent.
                drop(pending);
            }
        }
    }
    assert_eq!(unexpected, 0, "windows.synthetic.publication: class=matrix_expectation_mismatch; inspect controlled operation diagnostics");
}

#[test]
fn about_windows_missing_database_is_not_created() {
    let root = tempdir().unwrap();
    let path = root.path().join("missing.sqlite");
    let policy = platform::TestSecurityPolicy::capture(root.path()).unwrap();
    let error = crate::database::open_read_only(&path)
        .err()
        .expect("Missing database must be refused");
    match error {
        rusqlite::Error::ToSqlConversionFailure(error) => {
            assert_eq!(
                error.downcast_ref::<io::Error>().unwrap().kind(),
                io::ErrorKind::NotFound
            );
        }
        _ => panic!("about.synthetic: unexpected_missing_classification"),
    }
    assert!(!path.exists());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    assert!(policy.matches(root.path()).unwrap());
}

#[test]
#[ignore = "Native file/directory/sidecar symlinks require SeCreateSymbolicLinkPrivilege or Developer Mode; run explicitly on a capable host, never count as passed"]
fn about_windows_reparse_database_ancestor_and_sidecars_are_refused() {
    use std::os::windows::fs::{symlink_dir, symlink_file};
    let root = tempdir().unwrap();
    let parent = root.path().join("actual");
    create_private_dir(&parent).unwrap();
    let database = parent.join("synthetic.sqlite");
    write_new(&database, b"synthetic target; never queried").unwrap();
    let before = fs::read(&database).unwrap();
    let parent_policy = platform::TestSecurityPolicy::capture(&parent).unwrap();
    let policy = platform::TestSecurityPolicy::capture(&database).unwrap();
    let alias = root.path().join("alias.sqlite");
    symlink_file(&database, &alias).expect("about.synthetic: unsupported_reparse_fixture");
    let parent_alias = root.path().join("alias-parent");
    symlink_dir(&parent, &parent_alias).expect("about.synthetic: unsupported_reparse_fixture");
    let refused = |path: &Path| {
        let error = crate::database::open_read_only(path)
            .err()
            .expect("Reparse must be refused");
        assert!(error
            .to_string()
            .contains("Reparse point in managed storage path"));
    };
    refused(&alias);
    refused(&parent_alias.join("synthetic.sqlite"));
    for suffix in ["-journal", "-wal", "-shm"] {
        let sidecar = parent.join(format!("synthetic.sqlite{suffix}"));
        symlink_file(&database, &sidecar).expect("about.synthetic: unsupported_reparse_fixture");
        refused(&database);
        assert!(fs::symlink_metadata(&sidecar)
            .unwrap()
            .file_type()
            .is_symlink());
        fs::remove_file(sidecar).unwrap();
    }
    assert_eq!(fs::read(&database).unwrap(), before);
    assert!(parent_policy.matches(&parent).unwrap());
    assert!(policy.matches(&database).unwrap());
}

#[test]
fn about_windows_verified_root_and_unsafe_ancestor_preserve_fixture() {
    let root = tempdir().unwrap();
    let ancestor=root.path().join("controlled-ancestor");
    let parent=ancestor.join("private-parent");
    create_private_dir(&ancestor).unwrap();
    create_private_dir(&parent).unwrap();
    let database = parent.join("synthetic.sqlite");
    write_new(&database,b"").unwrap();
    let db=rusqlite::Connection::open(&database).unwrap();
    db.execute_batch("CREATE TABLE schema_version(version INTEGER); INSERT INTO schema_version VALUES(7);").unwrap();
    db.close().unwrap();
    let before = fs::read(&database).unwrap();
    let policy = platform::TestSecurityPolicy::capture(&database).unwrap();
    let db=crate::database::open_read_only(&database).unwrap();
    assert_eq!(db.query_row("SELECT version FROM schema_version",[],|r|r.get::<_,i64>(0)).unwrap(),7);
    db.close().unwrap();
    assert_eq!(fs::read(&database).unwrap(),before);
    assert!(policy.matches(&database).unwrap());
    // Mutate only this new synthetic ancestor, never any volume/profile ACL.
    broad_fixture(&ancestor);
    let error = crate::database::open_read_only(&database).err().expect("Controlled unsafe ancestor must remain refused");
    assert!(error
        .to_string()
        .contains("class=unsafe_ancestor; reason=untrusted_allow_ace"));
    assert!(error.to_string().contains("role=existing_ancestor"));
    assert_eq!(fs::read(&database).unwrap(), before);
    assert!(policy.matches(&database).unwrap());
}
