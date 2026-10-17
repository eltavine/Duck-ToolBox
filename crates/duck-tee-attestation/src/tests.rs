use std::fs;

use base64ct::{Base64, Encoding};
use duck_core::AppPaths;
use duck_rkp::{
    cose::{DeviceKeys, build_csr, generate_ec_keypair},
    profile::{DeviceInfo, DiceCurve},
};
use rcgen::{CertificateParams, CertifiedIssuer, DistinguishedName, DnType, KeyPair};

use crate::{
    files::{self, InputFile},
    keybox::{self, Format},
    seed,
};

fn params(name: &str) -> CertificateParams {
    let mut params = CertificateParams::default();
    let mut subject = DistinguishedName::new();
    subject.push(DnType::CommonName, name);
    params.distinguished_name = subject;
    params
}

fn bundle() -> (String, Vec<u8>, Vec<u8>) {
    let root =
        CertifiedIssuer::self_signed(params("offline-root"), KeyPair::generate().unwrap()).unwrap();
    let key = KeyPair::generate().unwrap();
    let leaf = params("offline-leaf").signed_by(&key, &root).unwrap();
    (
        key.serialize_pem(),
        leaf.der().to_vec(),
        root.der().to_vec(),
    )
}

fn captured_pem(private: &str, certificates: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"\xffcapture header\0\n".to_vec();
    bytes.extend_from_slice(private.as_bytes());
    for cert in certificates {
        bytes.extend_from_slice(crate::pem::wrap("CERTIFICATE", cert).as_bytes());
    }
    bytes
}

#[test]
fn extracts_a_capture_orders_the_chain_and_round_trips_xml() {
    let (private, leaf, root) = bundle();
    let capture = captured_pem(&private, &[root, leaf]);
    let (xml, report) = keybox::extract(&capture, Format::Auto).unwrap();
    assert_eq!(report.input_format, "pem");
    assert_eq!(report.keys[0].certificates, 2);
    assert!(report.certificate_signatures_valid);
    assert!(!report.trust_verified && !report.revocation_checked);
    let (again, xml_report) = keybox::extract(xml.as_bytes(), Format::Xml).unwrap();
    assert_eq!(*xml, *again);
    assert_eq!(xml_report.input_format, "xml");
    let summary = serde_json::to_string(&report).unwrap();
    assert!(!summary.contains("PRIVATE KEY"));
    assert!(!summary.contains(&private.lines().nth(1).unwrap().to_owned()));
}

#[test]
fn rejects_private_key_pem_labels_that_do_not_match_the_der_encoding() {
    let (private, leaf, root) = bundle();
    for wrong_label in ["EC PRIVATE KEY", "RSA PRIVATE KEY"] {
        let mislabeled = private.replace("PRIVATE KEY", wrong_label);
        assert!(
            keybox::extract(
                &captured_pem(&mislabeled, &[leaf.clone(), root.clone()]),
                Format::Pem
            )
            .is_err()
        );
    }
}

#[test]
fn inline_upload_limits_leave_room_for_two_base64_files_in_the_shell_command() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(dir.path(), dir.path());
    let maximum = InputFile::Content {
        base64: Base64::encode_string(&vec![17; 32768]),
    };
    assert_eq!(maximum.read(&paths).unwrap().len(), 32768);
    let oversized = InputFile::Content {
        base64: Base64::encode_string(&vec![17; 32769]),
    };
    assert!(oversized.read(&paths).is_err());
    let path = dir.path().join("capture");
    fs::File::create(&path)
        .unwrap()
        .set_len(files::MAX_INPUT as u64)
        .unwrap();
    assert_eq!(files::read(&path).unwrap().len(), files::MAX_INPUT);
}

#[test]
fn verifies_reference_ec_and_reports_unsupported_legacy_rsa() {
    let xml = include_str!("../../duck-tricky-store/assets/aosp-keybox.xml");
    assert!(keybox::extract(xml.as_bytes(), Format::Xml).is_err());
    // The reference RSA key is 1024 bits, below the cryptographic backend's range.
    let start = xml.find("<Key algorithm=\"rsa\">").unwrap();
    let end = start + xml[start..].find("</Key>").unwrap() + "</Key>".len();
    let mut ec_only = xml.to_owned();
    ec_only.replace_range(start..end, "");
    let (_, report) = keybox::extract(ec_only.as_bytes(), Format::Xml).unwrap();
    assert_eq!(report.keys.len(), 1);
    assert_eq!(report.keys[0].algorithm, "ecdsa");
}

#[test]
fn verifies_rsa_2048_material_with_an_ec_issuer() {
    let root = CertifiedIssuer::self_signed(params("rsa-test-root"), KeyPair::generate().unwrap())
        .unwrap();
    let key = KeyPair::generate_for(&rcgen::PKCS_RSA_SHA256).unwrap();
    let leaf = params("rsa-test-leaf").signed_by(&key, &root).unwrap();
    let (_, report) = keybox::extract(
        &captured_pem(
            &key.serialize_pem(),
            &[leaf.der().to_vec(), root.der().to_vec()],
        ),
        Format::Pem,
    )
    .unwrap();
    assert_eq!(report.keys[0].algorithm, "rsa");
}

#[test]
fn rejects_a_wrong_private_key_and_unrelated_certificates() {
    let (private, leaf, root) = bundle();
    let (other, _, unrelated) = bundle();
    assert!(
        keybox::extract(
            &captured_pem(&other, &[leaf.clone(), root.clone()]),
            Format::Pem
        )
        .is_err()
    );
    assert!(
        keybox::extract(
            &captured_pem(&private, &[leaf, root, unrelated]),
            Format::Pem
        )
        .is_err()
    );
}

#[test]
fn matching_issuer_names_do_not_hide_a_forged_signature() {
    let (private, mut leaf, root) = bundle();
    *leaf.last_mut().unwrap() ^= 1;
    assert!(keybox::extract(&captured_pem(&private, &[leaf, root]), Format::Pem).is_err());
}

#[test]
fn an_incomplete_or_ambiguous_chain_is_rejected() {
    let (private, leaf, root) = bundle();
    assert!(
        keybox::extract(
            &captured_pem(&private, std::slice::from_ref(&leaf)),
            Format::Pem
        )
        .is_err()
    );
    let root2 = params("offline-root")
        .self_signed(&KeyPair::generate().unwrap())
        .unwrap();
    assert!(
        keybox::extract(
            &captured_pem(&private, &[leaf, root, root2.der().to_vec()]),
            Format::Pem
        )
        .is_err()
    );
}

#[test]
fn verifies_a_self_issued_key_rotation_certificate_in_the_chain() {
    let root =
        CertifiedIssuer::self_signed(params("rotating-ca"), KeyPair::generate().unwrap()).unwrap();
    let intermediate =
        CertifiedIssuer::signed_by(params("rotating-ca"), KeyPair::generate().unwrap(), &root)
            .unwrap();
    let key = KeyPair::generate().unwrap();
    let leaf = params("offline-leaf")
        .signed_by(&key, &intermediate)
        .unwrap();
    let (_, report) = keybox::extract(
        &captured_pem(
            &key.serialize_pem(),
            &[
                root.der().to_vec(),
                intermediate.der().to_vec(),
                leaf.der().to_vec(),
            ],
        ),
        Format::Pem,
    )
    .unwrap();
    assert_eq!(report.keys[0].certificates, 3);
}

#[test]
fn xml_counts_nesting_and_multiple_documents_are_checked() {
    let (private, leaf, root) = bundle();
    let (xml, _) = keybox::extract(&captured_pem(&private, &[leaf, root]), Format::Pem).unwrap();
    for bad in [
        xml.replace("<NumberOfKeyboxes>1", "<NumberOfKeyboxes>2"),
        xml.replace("<NumberOfCertificates>2", "<NumberOfCertificates>1"),
        xml.replace("algorithm=\"ecdsa\"", "algorithm=\"rsa\""),
        xml.replace("<PrivateKey", "<Unknown"),
        format!("{}{}", xml.as_str(), xml.as_str()),
        xml.replace("</Keybox>", "</Key>"),
        xml.replace("<CertificateChain>", "<CertificateChain>&undefined;"),
    ] {
        assert!(keybox::extract(bad.as_bytes(), Format::Xml).is_err());
    }
}

#[test]
fn xml_capture_boundaries_follow_tags_instead_of_comment_text() {
    let (private, leaf, root) = bundle();
    let capture = captured_pem(&private, &[leaf, root]);
    let (xml, _) = keybox::extract(&capture, Format::Pem).unwrap();
    let commented = xml.replace(
        "<AndroidAttestation>",
        "<AndroidAttestation><!-- closing marker: </AndroidAttestation> -->",
    );
    let mut captured = b"\xffcapture header\0\n".to_vec();
    captured.extend_from_slice(commented.as_bytes());
    captured.extend_from_slice(b"\0\xffcapture footer");
    let (extracted, _) = keybox::extract(&captured, Format::Auto).unwrap();
    assert_eq!(*xml, *extracted);

    let closing_whitespace = xml.replace("</AndroidAttestation>", "</AndroidAttestation >");
    assert!(keybox::extract(closing_whitespace.as_bytes(), Format::Xml).is_ok());
    let mut logged_pem = b"<AndroidAttestationLog>captured PEM follows\n".to_vec();
    logged_pem.extend_from_slice(&capture);
    let (_, report) = keybox::extract(&logged_pem, Format::Auto).unwrap();
    assert_eq!(report.input_format, "pem");
}

#[test]
fn xml_comments_outside_the_root_do_not_select_a_fake_document() {
    let (private, leaf, root) = bundle();
    let (xml, _) = keybox::extract(&captured_pem(&private, &[leaf, root]), Format::Pem).unwrap();
    let commented = xml.replace(
        "<AndroidAttestation>",
        "<!-- Expected root: <AndroidAttestation> -->\n<AndroidAttestation>",
    ) + "<!-- Root was <AndroidAttestation> -->";
    let (extracted, _) = keybox::extract(commented.as_bytes(), Format::Auto).unwrap();
    assert_eq!(*xml, *extracted);
    assert!(
        keybox::extract(
            format!("{commented}{}", xml.as_str()).as_bytes(),
            Format::Xml
        )
        .is_err()
    );
}

#[test]
fn xml_device_ids_must_contain_only_valid_xml_characters() {
    let (private, leaf, root) = bundle();
    let (xml, _) = keybox::extract(&captured_pem(&private, &[leaf, root]), Format::Pem).unwrap();
    for invalid in ['\0', '\u{1}', '\u{b}', '\u{fffe}', '\u{ffff}'] {
        let bad = xml.replace("DeviceID=\"offline\"", &format!("DeviceID=\"a{invalid}b\""));
        assert!(keybox::extract(bad.as_bytes(), Format::Xml).is_err());
    }
    for invalid in ["&#0;", "&#x1;", "&#xFFFE;"] {
        let bad = xml.replace("DeviceID=\"offline\"", &format!("DeviceID=\"a{invalid}b\""));
        assert!(keybox::extract(bad.as_bytes(), Format::Xml).is_err());
    }
}

#[test]
fn xml_does_not_discard_extra_certificates_or_keys() {
    let (private, leaf, root) = bundle();
    let (xml, _) = keybox::extract(&captured_pem(&private, &[leaf, root]), Format::Pem).unwrap();
    let (_, _, unrelated) = bundle();
    let cert = format!(
        "<Certificate format=\"pem\">{}</Certificate>",
        crate::pem::wrap("CERTIFICATE", &unrelated).as_str()
    );
    let bad = xml
        .replace("<NumberOfCertificates>2", "<NumberOfCertificates>3")
        .replace("</CertificateChain>", &format!("{cert}</CertificateChain>"));
    assert!(keybox::extract(bad.as_bytes(), Format::Xml).is_err());
    let key_start = xml.find("    <Key ").unwrap();
    let key_end = xml.find("    </Key>").unwrap() + "    </Key>".len();
    let duplicate = xml.replace(
        "  </Keybox>",
        &format!("{}  </Keybox>", &xml[key_start..key_end]),
    );
    assert!(keybox::extract(duplicate.as_bytes(), Format::Xml).is_err());
}

#[test]
fn malformed_pem_is_not_silently_ignored() {
    let (private, leaf, root) = bundle();
    let mut capture = captured_pem(&private, &[leaf, root]);
    capture.extend_from_slice(b"-----BEGIN PRIVATE KEY-----\nnot-complete");
    assert!(keybox::extract(&capture, Format::Pem).is_err());
    assert!(keybox::extract(b"a log with no key material", Format::Auto).is_err());
}

#[test]
fn xml_device_ids_are_preserved_without_double_escaping() {
    let (private, leaf, root) = bundle();
    let (xml, _) = keybox::extract(&captured_pem(&private, &[leaf, root]), Format::Pem).unwrap();
    let special = xml.replace("DeviceID=\"offline\"", "DeviceID=\"a&amp;b&quot;c\"");
    let (first, _) = keybox::extract(special.as_bytes(), Format::Xml).unwrap();
    let (second, _) = keybox::extract(first.as_bytes(), Format::Xml).unwrap();
    assert_eq!(*first, *second);
    assert!(first.contains("DeviceID=\"a&amp;b&quot;c\""));
}

fn csr(keys: &DeviceKeys) -> Vec<u8> {
    let device = DeviceInfo {
        brand: "test".into(),
        model: "fixture".into(),
        device: "fixture".into(),
        product: "fixture".into(),
        manufacturer: "test".into(),
        fused: 1,
        vb_state: "green".into(),
        os_version: "13".into(),
        security_level: "tee".into(),
        bootloader_state: "locked".into(),
        boot_patch_level: 20260101,
        system_patch_level: 202601,
        vendor_patch_level: 20260101,
        vbmeta_digest: Some("11".repeat(32)),
        dice_issuer: "CN=Android".into(),
        dice_subject: "CN=Android".into(),
    };
    build_csr(
        keys,
        &[0x22; 32],
        &[generate_ec_keypair().unwrap().cose_public],
        &[0x33; 32],
        &[0x44; 8],
        &device,
        2,
    )
    .unwrap()
    .csr_bytes
}

#[test]
fn verifies_the_actual_dice_leaf_instead_of_accepting_any_32_bytes() {
    let original = [0x11; 32];
    let request = csr(&DeviceKeys::from_seed(original));
    let report = seed::verify(&original, &request, seed::Derivation::Ed25519Seed).unwrap();
    assert!(report.identity_matches && report.csr_signature_valid);
    assert!(
        !seed::verify(&[0x12; 32], &request, seed::Derivation::Ed25519Seed)
            .unwrap()
            .identity_matches
    );
    assert!(
        !seed::verify(&original, &request, seed::Derivation::P256Seed)
            .unwrap()
            .identity_matches
    );
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains(&hex::encode(original))
    );
}

#[test]
fn p256_seed_derivation_is_distinct_from_a_direct_private_scalar() {
    let request = csr(&DeviceKeys::from_seed_with_curve([0; 32], DiceCurve::P256));
    assert!(
        seed::verify(&[0; 32], &request, seed::Derivation::P256Seed)
            .unwrap()
            .identity_matches
    );
    let mut scalar = [0; 32];
    scalar[31] = 1;
    assert!(
        seed::verify(&scalar, &request, seed::Derivation::P256Scalar)
            .unwrap()
            .identity_matches
    );
    assert!(
        !seed::verify(&scalar, &request, seed::Derivation::P256Seed)
            .unwrap()
            .identity_matches
    );
    assert!(seed::verify(&[0; 32], &request, seed::Derivation::P256Scalar).is_err());
}

#[test]
fn seed_check_requires_valid_signed_csr_and_no_trailing_cbor() {
    let seed_bytes = [0x11; 32];
    let request = csr(&DeviceKeys::from_seed(seed_bytes));
    let mut bad = request.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert!(seed::verify(&seed_bytes, &bad, seed::Derivation::Ed25519Seed).is_err());
    let mut trailing = request;
    trailing.push(0);
    assert!(seed::verify(&seed_bytes, &trailing, seed::Derivation::Ed25519Seed).is_err());
}

#[test]
fn extraction_requires_an_exact_candidate_or_explicit_in_bounds_offset() {
    assert_eq!(
        *seed::candidate(&[17; 32], seed::Format::Raw, None).unwrap(),
        [17; 32]
    );
    assert_eq!(
        *seed::candidate(
            format!("{}\n", "11".repeat(32)).as_bytes(),
            seed::Format::Hex,
            None
        )
        .unwrap(),
        [17; 32]
    );
    let mut capture = vec![0; 7];
    capture.extend_from_slice(&[17; 32]);
    capture.push(0);
    assert_eq!(
        *seed::candidate(&capture, seed::Format::Raw, Some(7)).unwrap(),
        [17; 32]
    );
    assert!(seed::candidate(&capture, seed::Format::Raw, None).is_err());
    assert!(seed::candidate(&capture, seed::Format::Raw, Some(usize::MAX)).is_err());
    assert!(seed::candidate(&capture, seed::Format::Raw, Some(9)).is_err());
    assert!(seed::candidate(&[0; 64], seed::Format::Hex, None).is_err());
    assert!(seed::candidate(&[0; 32], seed::Format::Hex, Some(0)).is_err());
}

#[test]
fn input_sources_are_bounded_and_unambiguous() {
    let invalid = serde_json::json!({"path":"/tmp/x","base64":"YWJj"});
    assert!(serde_json::from_value::<InputFile>(invalid).is_err());
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(dir.path(), dir.path());
    let source = InputFile::Content {
        base64: Base64::encode_string(&[17; 32]),
    };
    assert_eq!(source.read(&paths).unwrap().as_slice(), &[17; 32]);
    let large = dir.path().join("large");
    fs::File::create(&large)
        .unwrap()
        .set_len(files::MAX_INPUT as u64 + 1)
        .unwrap();
    assert!(files::read(&large).is_err());
    assert!(files::read(dir.path()).is_err());
    let empty = dir.path().join("empty");
    fs::write(&empty, []).unwrap();
    assert!(files::read(&empty).is_err());
}

#[cfg(unix)]
#[test]
fn input_symlinks_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("seed");
    fs::write(&file, [0x11; 32]).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(file, &link).unwrap();
    assert!(files::read(&link).is_err());
}

#[test]
fn private_exports_are_complete_and_failed_exports_are_removed() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::for_root(dir.path(), dir.path());
    let report = serde_json::json!({"identity_matches":true});
    assert!(files::export(&paths, "missing/identity.bin", &[17; 32], &report).is_err());
    assert_eq!(fs::read_dir(&paths.outputs_dir).unwrap().count(), 0);
    let output = files::export(&paths, "identity.bin", &[17; 32], &report).unwrap();
    assert_eq!(fs::read(&output.material_path).unwrap(), [17; 32]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(&output.report_path).unwrap())
            .unwrap(),
        report
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for file in [&output.material_path, &output.report_path] {
            assert_eq!(
                fs::metadata(file).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let parent = std::path::Path::new(&output.material_path)
            .parent()
            .unwrap();
        assert_eq!(
            fs::metadata(parent).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
