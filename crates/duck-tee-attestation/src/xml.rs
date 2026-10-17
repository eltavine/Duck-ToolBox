use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use quick_xml::{Reader, XmlVersion, events::Event};
use x509_cert::{Certificate, der::Decode};
use zeroize::Zeroizing;

use crate::{
    error::MaterialError,
    keybox::{self, Keybox},
    pem,
};

struct Node {
    name: String,
    attributes: BTreeMap<String, String>,
    text: Zeroizing<String>,
    children: Vec<Node>,
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Vec<Keybox>> {
    parse_inner(bytes).map_err(|error| {
        if error.is::<MaterialError>() {
            error
        } else {
            MaterialError::Keybox("expected one complete keybox XML with matching counts, PEM keys and certificate chains").into()
        }
    })
}

fn parse_inner(bytes: &[u8]) -> Result<Vec<Keybox>> {
    let start = find_start(bytes).ok_or_else(|| anyhow::anyhow!("no XML"))?;
    let (root, length) = tree(&bytes[start..])?;
    let end = start + length;
    ensure!(find_start(&bytes[end..]).is_none());
    ensure!(
        std::str::from_utf8(&bytes[start..end])?
            .chars()
            .all(xml_character)
    );
    ensure!(root.name == "AndroidAttestation" && root.attributes.is_empty());
    ensure!(root.children.len() >= 2 && root.children.len() <= 17);
    ensure!(count(&root.children[0], "NumberOfKeyboxes")? == root.children.len() - 1);
    let mut result = Vec::new();
    for entry in &root.children[1..] {
        ensure!(entry.name == "Keybox" && entry.attributes.len() == 1);
        let device_id = entry
            .attributes
            .get("DeviceID")
            .ok_or_else(|| anyhow::anyhow!("no ID"))?;
        ensure!(
            !device_id.trim().is_empty()
                && device_id.len() <= 256
                && device_id.chars().all(xml_character)
        );
        ensure!(!entry.children.is_empty() && entry.children.len() <= 2);
        let mut keys = Vec::new();
        for key in &entry.children {
            ensure!(key.name == "Key" && key.attributes.len() == 1 && key.children.len() == 2);
            let private = &key.children[0];
            ensure!(private.name == "PrivateKey");
            let private = one_pem(private, false)?;
            let chain = &key.children[1];
            ensure!(chain.name == "CertificateChain" && chain.attributes.is_empty());
            ensure!(chain.children.len() >= 2 && chain.children.len() <= 9);
            ensure!(count(&chain.children[0], "NumberOfCertificates")? == chain.children.len() - 1);
            let mut certificates = Vec::new();
            for cert in &chain.children[1..] {
                ensure!(cert.name == "Certificate");
                let blocks = pem::blocks(one_pem(cert, true)?.as_bytes())?;
                certificates.push(Certificate::from_der(&blocks[0].der)?);
            }
            let material = keybox::material(private, certificates)?;
            let algorithm = key.attributes.get("algorithm").map(String::as_str);
            ensure!(
                algorithm == Some(material.algorithm)
                    || (algorithm == Some("ec") && material.algorithm == "ecdsa")
            );
            keys.push(material);
        }
        result.push(Keybox {
            device_id: device_id.clone(),
            keys,
        });
    }
    Ok(result)
}

fn one_pem(node: &Node, certificate: bool) -> Result<Zeroizing<String>> {
    ensure!(node.children.is_empty() && node.attributes.len() == 1);
    ensure!(node.attributes.get("format").map(String::as_str) == Some("pem"));
    let blocks = pem::blocks(node.text.as_bytes())?;
    ensure!(blocks.len() == 1 && (blocks[0].label == "CERTIFICATE") == certificate);
    // Re-encode only the parsed block, excluding any capture annotations.
    Ok(pem::wrap(&blocks[0].label, &blocks[0].der))
}

fn count(node: &Node, name: &str) -> Result<usize> {
    ensure!(node.name == name && node.attributes.is_empty() && node.children.is_empty());
    Ok(node.text.trim().parse()?)
}

pub(crate) fn find_start(bytes: &[u8]) -> Option<usize> {
    const MARKER: &[u8] = b"<AndroidAttestation";
    let mut offset = 0;
    while let Some(next) = pem::find(&bytes[offset..], b"<") {
        offset += next;
        let remaining = &bytes[offset..];
        if let Some(comment) = remaining.strip_prefix(b"<!--") {
            offset += 4 + pem::find(comment, b"-->")? + 3;
            continue;
        }
        if remaining.starts_with(MARKER)
            && matches!(
                remaining.get(MARKER.len()),
                Some(b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n')
            )
        {
            return Some(offset);
        }
        offset += 1;
    }
    None
}

fn xml_character(value: char) -> bool {
    matches!(value, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

fn tree(bytes: &[u8]) -> Result<(Node, usize)> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().check_comments = true;
    let mut stack: Vec<Node> = Vec::new();
    let mut nodes = 0;
    loop {
        match reader.read_event()? {
            Event::Start(tag) => {
                nodes += 1;
                ensure!(nodes <= 512 && stack.len() < 8);
                let mut attributes = BTreeMap::new();
                for attribute in tag.attributes() {
                    let attribute = attribute?;
                    let name = attribute.key.as_ref().to_owned();
                    let value = attribute
                        .normalized_value(XmlVersion::Implicit1_0)?
                        .into_owned();
                    ensure!(attributes.insert(name, value).is_none());
                }
                stack.push(Node {
                    name: tag.name().as_ref().to_owned(),
                    attributes,
                    text: Zeroizing::new(String::new()),
                    children: Vec::new(),
                });
            }
            Event::End(_) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("unexpected end"))?;
                ensure!(node.children.is_empty() || node.text.trim().is_empty());
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    return Ok((node, reader.buffer_position() as usize));
                }
            }
            Event::Text(text) => {
                if let Some(node) = stack.last_mut() {
                    node.text
                        .push_str(&text.xml_content(XmlVersion::Implicit1_0));
                } else {
                    ensure!(text.xml_content(XmlVersion::Implicit1_0).trim().is_empty());
                }
            }
            Event::Comment(_) => {}
            Event::Eof => anyhow::bail!("incomplete XML"),
            _ => anyhow::bail!("unsupported XML construct"),
        }
    }
}
