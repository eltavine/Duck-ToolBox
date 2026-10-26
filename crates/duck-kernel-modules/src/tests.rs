use std::{fs, path::Path};

use duck_core::{AppPaths, Context, Sysroot};

use crate::{
    ModuleError, code_of, files, inventory,
    mutation::{self, LoadRequest, Request, UnloadRequest},
};

fn fixture() -> (tempfile::TempDir, Context) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("module");
    let ctx = Context {
        paths: AppPaths::for_root(&root, &root.join("var")),
        sysroot: Sysroot::new(directory.path().join("sys")),
    };
    put(
        &ctx,
        "/proc/modules",
        "sample 4096 0 - Live 0x0\nconsumer 8192 1 client, Live 0x0 (O)\n",
    );
    put(&ctx, "/proc/sys/kernel/osrelease", "6.6.30-android15\n");
    put(&ctx, "/proc/sys/kernel/modules_disabled", "0\n");
    put(&ctx, "/sys/module/sample/version", "1.0\n");
    put(&ctx, "/sys/module/sample/parameters/debug", "N\n");
    fs::create_dir_all(ctx.sysroot.path("/sys/module/sample/holders")).unwrap();
    put(&ctx, "/sys/module/builtin/parameters/enabled", "Y\n");
    (directory, ctx)
}

fn put(ctx: &Context, path: &str, value: impl AsRef<[u8]>) {
    let path = ctx.sysroot.path(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}

#[test]
fn inventories_loaded_modules_without_claiming_sysfs_only_entries_are_unloadable() {
    let (_directory, ctx) = fixture();
    let status = inventory::status(&ctx.sysroot).unwrap();
    assert!(status.proc_available && status.sysfs_available);
    assert_eq!(status.modules_disabled, Some(false));
    assert!(!status.operations_supported);
    let builtin = status.modules.iter().find(|m| m.name == "builtin").unwrap();
    assert_eq!(builtin.source, "sysfs_only");
    assert_eq!(builtin.references, None);
    let consumer = status
        .modules
        .iter()
        .find(|m| m.name == "consumer")
        .unwrap();
    assert_eq!(consumer.used_by, ["client"]);
    assert!(inventory::parse_proc("bad row").is_err());
    assert_eq!(
        inventory::parse_proc("permanent 32 -1 - Live 0x0").unwrap()[0].references,
        Some(-1)
    );
}

#[test]
fn unreadable_or_malformed_proc_is_distinct_from_an_empty_inventory() {
    let (_directory, ctx) = fixture();
    put(&ctx, "/proc/modules", "malformed");
    let status = inventory::status(&ctx.sysroot).unwrap();
    assert!(!status.proc_available);
    assert!(!status.warnings.is_empty());
    put(&ctx, "/proc/modules", "");
    assert!(inventory::status(&ctx.sysroot).unwrap().proc_available);
    fs::remove_file(ctx.sysroot.path("/proc/sys/kernel/modules_disabled")).unwrap();
    assert_eq!(
        inventory::status(&ctx.sysroot).unwrap().modules_disabled,
        None
    );
}

#[test]
fn details_keep_parameter_read_failures_and_reject_path_traversal() {
    let (_directory, ctx) = fixture();
    fs::create_dir_all(ctx.sysroot.path("/sys/module/sample/parameters/unreadable")).unwrap();
    let details = inventory::details(&ctx.sysroot, "sample").unwrap();
    assert_eq!(details.version.as_deref(), Some("1.0"));
    assert_eq!(details.parameters[0].value.as_deref(), Some("N"));
    assert!(details.parameters[1].value.is_none());
    assert!(details.parameters[1].error.is_some());
    assert!(inventory::details(&ctx.sysroot, "../sample").is_err());
    assert!(inventory::details(&ctx.sysroot, "missing").is_err());
    put(
        &ctx,
        "/sys/module/sample/parameters/label",
        "  value with spaces  \n",
    );
    let details = inventory::details(&ctx.sysroot, "sample").unwrap();
    assert_eq!(
        details
            .parameters
            .iter()
            .find(|p| p.name == "label")
            .unwrap()
            .value
            .as_deref(),
        Some("  value with spaces  ")
    );
}

#[test]
fn elf_inspection_preserves_repeated_metadata_and_checks_release() {
    let (_directory, ctx) = fixture();
    put(
        &ctx,
        "/data/local/tmp/sample.ko",
        files::fixture("6.6.30-android15 SMP preempt"),
    );
    let (info, bytes) = files::inspect(&ctx.sysroot, "/data/local/tmp/sample.ko").unwrap();
    assert_eq!(info.name, "sample");
    assert_eq!(info.architecture, files::Architecture::Aarch64);
    assert_eq!(
        serde_json::to_value(&info).unwrap()["architecture"],
        "aarch64"
    );
    assert_eq!(info.elf_class, 64);
    assert!(info.little_endian);
    assert_eq!(info.metadata["alias"], ["test:first", "test:second"]);
    assert_eq!(info.kernel_release_match, Some(true));
    assert_eq!(info.size, bytes.len() as u64);
    assert_eq!(info.sha256.len(), 64);
    put(
        &ctx,
        "/data/local/tmp/sample.ko",
        files::fixture("different-kernel SMP"),
    );
    assert_eq!(
        files::inspect(&ctx.sysroot, "/data/local/tmp/sample.ko")
            .unwrap()
            .0
            .kernel_release_match,
        Some(false)
    );
}

#[test]
fn inspection_preserves_raw_digest_and_reports_non_utf8_metadata() {
    let (_directory, ctx) = fixture();
    put(
        &ctx,
        "/tmp/sample.ko",
        files::fixture_elf(
            object::Architecture::Aarch64,
            object::Endianness::Little,
            b"name=sample\0vermagic=6.6.30-android15\0description=bad\xfftext\0",
        ),
    );
    let info = files::inspect(&ctx.sysroot, "/tmp/sample.ko").unwrap().0;
    assert_eq!(info.warnings, ["metadata_text_replaced"]);
    assert_eq!(info.metadata["description"], ["bad\u{fffd}text"]);
}

#[test]
fn inspection_bounds_metadata_entries_and_distinct_keys_before_expanding_them() {
    let (_directory, ctx) = fixture();
    for metadata in [
        format!("name=sample\0{}", "alias=a\0".repeat(8192)),
        (0..129).map(|n| format!("key{n}=value\0")).collect(),
        format!("{}=value\0", "a".repeat(129)),
    ] {
        put(
            &ctx,
            "/tmp/budget.ko",
            files::fixture_elf(
                object::Architecture::Aarch64,
                object::Endianness::Little,
                metadata.as_bytes(),
            ),
        );
        assert_eq!(
            code_of(&files::inspect(&ctx.sysroot, "/tmp/budget.ko").unwrap_err()),
            "invalid_input"
        );
    }
}

#[test]
fn inspection_rejects_duplicate_metadata_sections_instead_of_displaying_only_one() {
    let (_directory, ctx) = fixture();
    let mut object = object::write::Object::new(
        object::BinaryFormat::Elf,
        object::Architecture::Aarch64,
        object::Endianness::Little,
    );
    for name in ["first", "second"] {
        let section = object.add_section(
            vec![],
            b".modinfo".to_vec(),
            object::SectionKind::ReadOnlyData,
        );
        object.append_section_data(
            section,
            format!("name={name}\0vermagic=6.6.30-android15\0").as_bytes(),
            1,
        );
    }
    put(&ctx, "/tmp/duplicate.ko", object.write().unwrap());
    assert_eq!(
        code_of(&files::inspect(&ctx.sysroot, "/tmp/duplicate.ko").unwrap_err()),
        "invalid_input"
    );
}

#[test]
fn invalid_metadata_offsets_are_reported_as_invalid_elf_input() {
    use object::{Object as _, ObjectSection as _};
    let (_directory, ctx) = fixture();
    let mut bytes = files::fixture("6.6.30-android15");
    let index = object::File::parse(bytes.as_slice())
        .unwrap()
        .section_by_name(".modinfo")
        .unwrap()
        .index()
        .0;
    // ELF64 e_shoff, e_shentsize and sh_offset from the ELF file-format contract.
    let table = u64::from_le_bytes(bytes[40..48].try_into().unwrap()) as usize;
    let stride = u16::from_le_bytes(bytes[58..60].try_into().unwrap()) as usize;
    let field = table + index * stride + 24;
    let offset = bytes.len() as u64 + 128;
    bytes[field..field + 8].copy_from_slice(&offset.to_le_bytes());
    put(&ctx, "/tmp/offset.ko", bytes);
    assert_eq!(
        code_of(&files::inspect(&ctx.sysroot, "/tmp/offset.ko").unwrap_err()),
        "invalid_input"
    );
}

#[test]
fn loading_refuses_wrong_elf_byte_order_class_and_missing_vermagic() {
    let (_directory, ctx) = fixture();
    put(&ctx, "/proc/modules", "");
    fs::remove_dir_all(ctx.sysroot.path("/sys/module/sample")).unwrap();
    for bytes in [
        files::fixture_elf(
            object::Architecture::Aarch64,
            object::Endianness::Big,
            b"name=sample\0vermagic=6.6.30-android15\0",
        ),
        files::fixture_elf(
            object::Architecture::Arm,
            object::Endianness::Little,
            b"name=sample\0vermagic=6.6.30-android15\0",
        ),
        files::fixture_elf(
            object::Architecture::Aarch64,
            object::Endianness::Little,
            b"name=sample\0license=GPL\0",
        ),
    ] {
        put(&ctx, "/tmp/sample.ko", bytes);
        let info = files::inspect(&ctx.sysroot, "/tmp/sample.ko").unwrap().0;
        let request = LoadRequest {
            path: info.path,
            sha256: info.sha256,
            parameters: String::new(),
            confirmed: true,
        };
        assert_eq!(
            code_of(&mutation::validate_load(&ctx, &request).unwrap_err()),
            "invalid_input"
        );
    }
}

#[test]
fn catalog_reports_the_file_limit_even_when_a_directory_ends_at_that_limit() {
    let (_directory, ctx) = fixture();
    for index in 0..1024 {
        put(&ctx, &format!("/vendor/lib/modules/{index}.ko"), "");
    }
    let catalog = files::catalog(&ctx.sysroot).unwrap();
    assert_eq!(catalog.files.len(), 1024);
    assert!(catalog.truncated);
}

#[test]
fn rejects_non_elf_compressed_oversized_and_escaped_files() {
    let (directory, ctx) = fixture();
    put(&ctx, "/tmp/bad.ko", "not an ELF");
    assert!(files::inspect(&ctx.sysroot, "/tmp/bad.ko").is_err());
    assert!(files::inspect(&ctx.sysroot, "/tmp/bad.ko.xz").is_err());
    assert!(files::inspect(&ctx.sysroot, "/tmp/../tmp/bad.ko").is_err());
    assert!(files::inspect(&ctx.sysroot, "tmp/bad.ko").is_err());
    fs::File::create(ctx.sysroot.path("/tmp/large.ko"))
        .unwrap()
        .set_len(files::MAX_FILE_BYTES + 1)
        .unwrap();
    assert!(files::inspect(&ctx.sysroot, "/tmp/large.ko").is_err());
    #[cfg(unix)]
    {
        let outside = directory.path().join("outside.ko");
        fs::write(&outside, files::fixture("6.6.30-android15")).unwrap();
        std::os::unix::fs::symlink(outside, ctx.sysroot.path("/tmp/escape.ko")).unwrap();
        assert!(files::inspect(&ctx.sysroot, "/tmp/escape.ko").is_err());
    }
}

#[test]
fn refuses_changed_or_already_loaded_files_but_leaves_release_compatibility_to_kernel() {
    let (_directory, ctx) = fixture();
    let path = "/data/local/tmp/sample.ko";
    put(&ctx, path, files::fixture("6.6.30-android15"));
    let info = files::inspect(&ctx.sysroot, path).unwrap().0;
    let mut request = LoadRequest {
        path: path.into(),
        sha256: "0".repeat(64),
        parameters: String::new(),
        confirmed: true,
    };
    assert_eq!(
        code_of(&mutation::validate_load(&ctx, &request).unwrap_err()),
        "file_changed"
    );
    request.sha256 = info.sha256;
    assert!(mutation::validate_load(&ctx, &request).is_err());
    put(&ctx, "/proc/modules", "");
    fs::remove_dir_all(ctx.sysroot.path("/sys/module/sample")).unwrap();
    assert!(mutation::validate_load(&ctx, &request).is_ok());
    request.parameters = "debug=1\n".into();
    assert!(mutation::validate_load(&ctx, &request).is_err());
    request.parameters.clear();
    put(&ctx, path, files::fixture("wrong-kernel"));
    request.sha256 = files::inspect(&ctx.sysroot, path).unwrap().0.sha256;
    assert!(mutation::validate_load(&ctx, &request).is_ok());
}

#[test]
fn proc_format_without_module_unload_support_is_still_a_loaded_module() {
    let modules = inventory::parse_proc(
        "driver 4096 - - Live 0x0\npermanent 2048 0 [permanent], Live 0x0 (O)\n",
    )
    .unwrap();
    assert_eq!(modules[0].source, "loaded");
    assert_eq!(modules[0].references, None);
    assert_eq!(modules[1].used_by, ["[permanent]"]);
}

#[test]
fn unloading_requires_live_unused_module_with_no_holders() {
    let (_directory, ctx) = fixture();
    assert!(mutation::validate_unload(&ctx, "sample").is_ok());
    assert!(mutation::validate_unload(&ctx, "consumer").is_err());
    assert!(mutation::validate_unload(&ctx, "builtin").is_err());
    put(&ctx, "/sys/module/sample/holders/dependent", "");
    assert!(mutation::validate_unload(&ctx, "sample").is_err());
    fs::remove_file(ctx.sysroot.path("/sys/module/sample/holders/dependent")).unwrap();
    put(&ctx, "/proc/modules", "sample 4096 0 - Loading 0x0\n");
    assert!(mutation::validate_unload(&ctx, "sample").is_err());
}

#[test]
fn dotted_module_names_are_visible_but_arm64_mutations_enforce_the_name_buffer() {
    let (_directory, ctx) = fixture();
    put(&ctx, "/proc/modules", "driver.test+1 4096 0 - Live 0x0\n");
    fs::create_dir_all(ctx.sysroot.path("/sys/module/driver.test+1/holders")).unwrap();
    assert!(inventory::details(&ctx.sysroot, "driver.test+1").is_ok());
    assert!(mutation::validate_unload(&ctx, "driver.test+1").is_ok());
    for name in [".", "..", "../sample", "a/b", "a\0b"] {
        assert!(!inventory::valid_name(name));
    }
    for (length, valid) in [(55, true), (56, false)] {
        let name = "a".repeat(length);
        fs::create_dir_all(ctx.sysroot.path(format!("/sys/module/{name}/holders"))).unwrap();
        put(&ctx, "/proc/modules", format!("{name} 4096 0 - Live 0x0\n"));
        assert_eq!(mutation::validate_unload(&ctx, &name).is_ok(), valid);
        fs::remove_dir_all(ctx.sysroot.path(format!("/sys/module/{name}"))).unwrap();
        put(&ctx, "/proc/modules", "");
        put(
            &ctx,
            "/tmp/name.ko",
            files::fixture_elf(
                object::Architecture::Aarch64,
                object::Endianness::Little,
                format!("name={name}\0vermagic=6.6.30-android15\0").as_bytes(),
            ),
        );
        let info = files::inspect(&ctx.sysroot, "/tmp/name.ko").unwrap().0;
        assert_eq!(
            mutation::validate_load(
                &ctx,
                &LoadRequest {
                    path: info.path,
                    sha256: info.sha256,
                    parameters: String::new(),
                    confirmed: true,
                }
            )
            .is_ok(),
            valid
        );
    }
}

#[test]
fn catalog_does_not_follow_nested_symlink_loops_or_include_compressed_files() {
    let (_directory, ctx) = fixture();
    put(
        &ctx,
        "/vendor/lib/modules/nested/driver.ko",
        "ELF placeholder",
    );
    put(&ctx, "/vendor/lib/modules/driver.ko.xz", "compressed");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        Path::new(".."),
        ctx.sysroot.path("/vendor/lib/modules/nested/loop"),
    )
    .unwrap();
    let catalog = files::catalog(&ctx.sysroot).unwrap();
    assert_eq!(catalog.files.len(), 1);
    assert_eq!(
        catalog.files[0].path,
        "/vendor/lib/modules/nested/driver.ko"
    );
    assert!(!catalog.truncated);
}

#[test]
fn mutations_require_confirmation_and_never_execute_against_a_test_sysroot() {
    let (_directory, ctx) = fixture();
    let mut request = Request::Unload(UnloadRequest {
        name: "sample".into(),
        confirmed: false,
    });
    assert_eq!(
        code_of(&request.check(&ctx).unwrap_err()),
        "confirmation_required"
    );
    if let Request::Unload(v) = &mut request {
        v.confirmed = true
    }
    assert_eq!(
        code_of(&request.check(&ctx).unwrap_err()),
        "unsupported_environment"
    );
    assert_eq!(
        code_of(&anyhow::Error::from(ModuleError::KernelRejected(
            "test".into()
        ))),
        "kernel_rejected"
    );
}

#[test]
fn root_core_is_protected_even_when_it_reports_no_references() {
    let (_directory, ctx) = fixture();
    put(&ctx, "/proc/modules", "kernelsu 4096 0 - Live 0x0\n");
    let module = inventory::status(&ctx.sysroot)
        .unwrap()
        .modules
        .into_iter()
        .find(|m| m.name == "kernelsu")
        .unwrap();
    assert!(module.unload_protected);
    assert_eq!(
        code_of(&mutation::validate_unload(&ctx, "kernelsu").unwrap_err()),
        "module_protected"
    );
}

#[test]
fn unreadable_or_missing_holders_are_unknown_and_prevent_removal() {
    let (_directory, ctx) = fixture();
    let path = ctx.sysroot.path("/sys/module/sample/holders");
    assert!(mutation::validate_unload(&ctx, "sample").is_ok());
    fs::remove_dir(&path).unwrap();
    for unreadable in [false, true] {
        if unreadable {
            fs::write(&path, "not a directory").unwrap();
        }
        let details = inventory::details(&ctx.sysroot, "sample").unwrap();
        assert!(!details.holders_available);
        assert!(!details.warnings.is_empty());
        assert_eq!(
            code_of(&mutation::validate_unload(&ctx, "sample").unwrap_err()),
            "state_unavailable"
        );
    }
}

#[test]
fn load_parameters_enforce_utf8_bytes_and_control_characters() {
    let (_directory, ctx) = fixture();
    put(&ctx, "/proc/modules", "");
    fs::remove_dir_all(ctx.sysroot.path("/sys/module/sample")).unwrap();
    put(&ctx, "/tmp/sample.ko", files::fixture("6.6.30-android15"));
    let info = files::inspect(&ctx.sysroot, "/tmp/sample.ko").unwrap().0;
    let mut request = LoadRequest {
        path: info.path,
        sha256: info.sha256,
        parameters: String::new(),
        confirmed: true,
    };
    for (unit, count) in [("a", 4096), ("字", 1365), ("😀", 1024)] {
        request.parameters = unit.repeat(count);
        assert!(mutation::validate_load(&ctx, &request).is_ok());
        request.parameters.push_str(unit);
        assert_eq!(
            code_of(&mutation::validate_load(&ctx, &request).unwrap_err()),
            "invalid_input"
        );
    }
    for control in ['\0', '\n', '\t', '\u{7f}', '\u{85}', '\u{9f}'] {
        request.parameters = format!("label={control}");
        assert_eq!(
            code_of(&mutation::validate_load(&ctx, &request).unwrap_err()),
            "invalid_input"
        );
    }
}

#[test]
fn missing_name_metadata_keeps_the_kbuild_filename_estimate_distinct() {
    let (_directory, ctx) = fixture();
    put(
        &ctx,
        "/tmp/test,-name.ko",
        files::fixture_elf(
            object::Architecture::Aarch64,
            object::Endianness::Little,
            b"license=GPL\0vermagic=6.6.30-android15\0",
        ),
    );
    let file = files::inspect(&ctx.sysroot, "/tmp/test,-name.ko")
        .unwrap()
        .0;
    assert_eq!(file.name, "test__name");
    assert!(!file.metadata.contains_key("name"));
}
