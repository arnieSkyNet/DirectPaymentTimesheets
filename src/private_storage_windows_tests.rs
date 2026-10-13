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
    powershell(
        path,
        r#"
        $ErrorActionPreference='Stop'
        $acl = Get-Acl -LiteralPath $env:DPT_SYNTHETIC_PATH
        $acl.SetAccessRuleProtection($true, $false)
        foreach ($rule in @($acl.Access)) { [void]$acl.RemoveAccessRuleSpecific($rule) }
        $everyone = [System.Security.Principal.SecurityIdentifier]::new('S-1-1-0')
        $rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
            $everyone, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
        $acl.AddAccessRule($rule)
        Set-Acl -LiteralPath $env:DPT_SYNTHETIC_PATH -AclObject $acl
    "#,
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
    powershell(
        &child,
        r#"
        $ErrorActionPreference='Stop'
        $acl = Get-Acl -LiteralPath $env:DPT_SYNTHETIC_PATH
        $administrators = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
        $ownerRights = [System.Security.Principal.SecurityIdentifier]::new('S-1-3-4')
        $acl.SetOwner($administrators)
        foreach ($sid in @($administrators, $ownerRights)) {
            $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new(
                $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
        }
        Set-Acl -LiteralPath $env:DPT_SYNTHETIC_PATH -AclObject $acl
    "#,
    );
    assert!(!platform::is_private(&child).unwrap());
    assert!(child.exists());
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
        $acl = Get-Acl -LiteralPath $env:DPT_SYNTHETIC_PATH
        $acl.SetAccessRuleProtection($true, $false)
        foreach ($rule in @($acl.Access)) { [void]$acl.RemoveAccessRuleSpecific($rule) }
        $current = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
        $system = [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18')
        foreach ($sid in @($current, $system)) {
            $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new(
                $sid, 'FullControl', 'None', 'None', 'Allow'))
        }
        Set-Acl -LiteralPath $env:DPT_SYNTHETIC_PATH -AclObject $acl
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
                let outcome = platform::publication_probe(pending.path(), &destination, exists);
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
