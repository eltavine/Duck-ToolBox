use super::{decode_modified_utf8, decode_xmlish_bytes, parse_boolish, parse_i64ish};

fn push_utf(buf: &mut Vec<u8>, value: &str) {
    let len = u16::try_from(value.len()).unwrap();
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(value.as_bytes());
}

fn push_utf_bytes(buf: &mut Vec<u8>, value: &[u8]) {
    let len = u16::try_from(value.len()).unwrap();
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(value);
}

fn push_new_interned(buf: &mut Vec<u8>, value: &str) {
    buf.extend_from_slice(&0xFFFF_u16.to_be_bytes());
    push_utf(buf, value);
}

fn push_interned_ref(buf: &mut Vec<u8>, index: u16) {
    buf.extend_from_slice(&index.to_be_bytes());
}

#[test]
fn parse_boolish_handles_common_android_representations() {
    assert_eq!(parse_boolish("true"), Some(true));
    assert_eq!(parse_boolish("False"), Some(false));
    assert_eq!(parse_boolish("1"), Some(true));
    assert_eq!(parse_boolish("0"), Some(false));
    assert_eq!(parse_boolish("maybe"), None);
}

#[test]
fn parse_i64ish_understands_decimal_and_hex() {
    assert_eq!(parse_i64ish("16"), Some(16));
    assert_eq!(parse_i64ish("0x10"), Some(16));
    assert_eq!(parse_i64ish("ff"), Some(255));
    assert_eq!(parse_i64ish("-0x10"), Some(-16));
}

#[test]
fn modified_utf8_decodes_embedded_nul() {
    assert_eq!(
        decode_modified_utf8(b"duck\xc0\x80toolbox").unwrap(),
        "duck\0toolbox"
    );
}

#[test]
fn modified_utf8_decodes_cesu8_surrogate_pairs() {
    assert_eq!(
        decode_modified_utf8(&[0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80]).unwrap(),
        "😀",
    );
}

#[test]
fn android_binary_xml_is_decoded_to_text_xml() {
    let mut abx = Vec::new();
    abx.extend_from_slice(b"ABX\0");
    abx.push(0x00);

    abx.push(0x02);
    push_new_interned(&mut abx, "packages");
    abx.push(0x02);
    push_new_interned(&mut abx, "package");

    abx.push(0x2F);
    push_new_interned(&mut abx, "name");
    push_utf(&mut abx, "com.example.app");

    abx.push(0x2F);
    push_new_interned(&mut abx, "codePath");
    push_utf(&mut abx, "/data/app/~~abc/com.example.app/base.apk");

    abx.push(0x7F);
    push_new_interned(&mut abx, "publicFlags");
    abx.extend_from_slice(&(0x10_i32).to_be_bytes());

    abx.push(0x03);
    push_interned_ref(&mut abx, 1);
    abx.push(0x03);
    push_interned_ref(&mut abx, 0);
    abx.push(0x01);

    let decoded = decode_xmlish_bytes(&abx).unwrap();
    assert!(decoded.contains("<package"));
    assert!(decoded.contains(r#"name="com.example.app""#));
    assert!(decoded.contains(r#"codePath="/data/app/~~abc/com.example.app/base.apk""#));
    assert!(decoded.contains(r#"publicFlags="0x10""#));
}

#[test]
fn android_binary_xml_decodes_boolean_attributes() {
    let mut abx = Vec::new();
    abx.extend_from_slice(b"ABX\0");
    abx.push(0x00);

    abx.push(0x02);
    push_new_interned(&mut abx, "package-restrictions");
    abx.push(0x02);
    push_new_interned(&mut abx, "pkg");

    abx.push(0x2F);
    push_new_interned(&mut abx, "name");
    push_utf(&mut abx, "com.example.alpha");

    abx.push(0xDF);
    push_new_interned(&mut abx, "inst");

    abx.push(0x03);
    push_interned_ref(&mut abx, 1);
    abx.push(0x03);
    push_interned_ref(&mut abx, 0);
    abx.push(0x01);

    assert!(
        decode_xmlish_bytes(&abx)
            .unwrap()
            .contains(r#"<pkg name="com.example.alpha" inst="false">"#)
    );
}

#[test]
fn text_xml_utf16le_without_bom_is_detected() {
    let bytes = b"<root>ok</root>"
        .iter()
        .flat_map(|byte| [*byte, 0x00])
        .collect::<Vec<_>>();

    assert_eq!(decode_xmlish_bytes(&bytes).unwrap(), "<root>ok</root>");
}

#[test]
fn android_binary_xml_supports_art_modified_utf_with_four_byte_sequences() {
    let mut abx = Vec::new();
    abx.extend_from_slice(b"ABX\0");
    abx.push(0x00);

    abx.push(0x02);
    push_new_interned(&mut abx, "emoji");

    abx.push(0x24);
    push_utf_bytes(&mut abx, "😀".as_bytes());
    abx.push(0x03);
    push_interned_ref(&mut abx, 0);
    abx.push(0x01);

    assert_eq!(decode_xmlish_bytes(&abx).unwrap(), "<emoji>😀</emoji>");
}

#[test]
fn android_binary_xml_resolves_entity_refs_like_android() {
    let mut abx = Vec::new();
    abx.extend_from_slice(b"ABX\0");
    abx.push(0x00);

    abx.push(0x02);
    push_new_interned(&mut abx, "root");
    abx.push(0x26);
    push_utf(&mut abx, "amp");
    abx.push(0x03);
    push_interned_ref(&mut abx, 0);
    abx.push(0x01);

    assert_eq!(decode_xmlish_bytes(&abx).unwrap(), "<root>&amp;</root>");
}

#[test]
fn android_binary_xml_base64_attributes_are_rendered_as_base64_text() {
    let mut abx = Vec::new();
    abx.extend_from_slice(b"ABX\0");
    abx.push(0x00);

    abx.push(0x02);
    push_new_interned(&mut abx, "root");

    abx.push(0x5F);
    push_new_interned(&mut abx, "blob");
    abx.extend_from_slice(&(3_u16).to_be_bytes());
    abx.extend_from_slice(&[0x01, 0x02, 0x03]);

    abx.push(0x03);
    push_interned_ref(&mut abx, 0);
    abx.push(0x01);

    assert_eq!(
        decode_xmlish_bytes(&abx).unwrap(),
        r#"<root blob="AQID"></root>"#
    );
}
