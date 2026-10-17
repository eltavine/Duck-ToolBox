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
    let start =
        pem::find(bytes, b"<AndroidAttestation").ok_or_else(|| anyhow::anyhow!("no XML"))?;
    let end_marker = b"</AndroidAttestation>";
    let end = start
        + pem::find(&bytes[start..], end_marker)
            .ok_or_else(|| anyhow::anyhow!("incomplete XML"))?
        + end_marker.len();
    ensure!(pem::find(&bytes[end..], b"<AndroidAttestation").is_none());
    let text = std::str::from_utf8(&bytes[start..end])?;
    let root = tree(text)?;
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
        ensure!(!device_id.trim().is_empty() && device_id.len() <= 256);
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

fn tree(text: &str) -> Result<Node> {
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Node> = Vec::new();
    let mut root = None;
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
                    ensure!(root.is_none());
                    root = Some(node);
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
            Event::Eof => break,
            _ => anyhow::bail!("unsupported XML construct"),
        }
    }
    ensure!(stack.is_empty());
    root.ok_or_else(|| anyhow::anyhow!("no root"))
}
