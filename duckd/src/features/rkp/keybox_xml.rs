use std::fmt::Write as _;

use anyhow::{Context, Result, anyhow};
use base64ct::{Base64, Encoding};
use p256::SecretKey as P256SecretKey;
use serde::Serialize;
use x509_cert::{
    Certificate,
    der::{Decode, Encode},
};

#[derive(Debug, Clone)]
pub struct ParsedCertificate {
    pub der: Vec<u8>,
    pub subject_der: Vec<u8>,
    pub issuer_der: Vec<u8>,
    pub subject_summary: String,
    pub issuer_summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CertificateChainSummary {
    pub certificates: usize,
    pub subjects: Vec<String>,
}

pub fn parse_der_cert_chain(data: &[u8]) -> Result<Vec<ParsedCertificate>> {
    if data.is_empty() {
        return Err(anyhow!("certificate chain is empty"));
    }

    let mut certificates = Vec::new();
    let mut offset = 0usize;

    while offset < data.len() {
        if data[offset] != 0x30 {
            return Err(anyhow!(
                "certificate chain contains trailing non-DER data at offset {offset}"
            ));
        }

        let (header_len, body_len) = parse_der_len(&data[offset..])?;
        let total_len = header_len
            .checked_add(body_len)
            .ok_or_else(|| anyhow!("DER length overflow"))?;
        let end = offset
            .checked_add(total_len)
            .ok_or_else(|| anyhow!("DER length overflow"))?;
        if end > data.len() {
            return Err(anyhow!("DER certificate is truncated"));
        }

        let slice = &data[offset..end];
        let certificate = Certificate::from_der(slice).context("parse X.509 certificate")?;
        let subject_der = certificate
            .tbs_certificate
            .subject
            .to_der()
            .context("encode subject name")?;
        let issuer_der = certificate
            .tbs_certificate
            .issuer
            .to_der()
            .context("encode issuer name")?;

        certificates.push(ParsedCertificate {
            der: slice.to_vec(),
            subject_der,
            issuer_der,
            subject_summary: format!("{:?}", certificate.tbs_certificate.subject),
            issuer_summary: format!("{:?}", certificate.tbs_certificate.issuer),
        });

        offset = end;
    }

    if certificates.is_empty() {
        return Err(anyhow!("certificate chain is empty"));
    }

    Ok(certificates)
}

pub fn summarize_chain(certs: &[ParsedCertificate]) -> CertificateChainSummary {
    CertificateChainSummary {
        certificates: certs.len(),
        subjects: certs
            .iter()
            .map(|cert| cert.subject_summary.clone())
            .collect(),
    }
}

pub fn build_keybox_xml(
    secret_key: &P256SecretKey,
    ec_cert_chain: &[ParsedCertificate],
    device_id: &str,
) -> Result<String> {
    if ec_cert_chain.is_empty() {
        return Err(anyhow!("certificate chain is empty"));
    }

    let cert_chain = sort_cert_chain(ec_cert_chain);
    let key_der = secret_key
        .to_sec1_der()
        .context("encode EC private key as SEC1 DER")?;
    let key_pem = pem_wrap("EC PRIVATE KEY", key_der.as_slice());
    let device_id = escape_xml_attribute(device_id);

    let mut xml = String::new();
    writeln!(xml, "<?xml version=\"1.0\"?>").unwrap();
    writeln!(xml, "<!-- Made by Eltavine & MhmRdd -->").unwrap();
    writeln!(xml, "<AndroidAttestation>").unwrap();
    writeln!(xml, "    <NumberOfKeyboxes>1</NumberOfKeyboxes>").unwrap();
    writeln!(xml, "    <Keybox DeviceID=\"{device_id}\">").unwrap();
    writeln!(xml, "        <Key algorithm=\"ecdsa\">").unwrap();
    writeln!(xml, "            <PrivateKey format=\"pem\">").unwrap();
    writeln!(xml, "{}", indent_block(&key_pem, "                ")).unwrap();
    writeln!(xml, "            </PrivateKey>").unwrap();
    writeln!(xml, "            <CertificateChain>").unwrap();
    writeln!(
        xml,
        "                <NumberOfCertificates>{}</NumberOfCertificates>",
        cert_chain.len()
    )
    .unwrap();

    for cert in cert_chain {
        let cert_pem = pem_wrap("CERTIFICATE", &cert.der);
        writeln!(xml, "                <Certificate format=\"pem\">").unwrap();
        writeln!(xml, "{}", indent_block(&cert_pem, "                    ")).unwrap();
        writeln!(xml, "                </Certificate>").unwrap();
    }

    writeln!(xml, "            </CertificateChain>").unwrap();
    writeln!(xml, "        </Key>").unwrap();
    writeln!(xml, "    </Keybox>").unwrap();
    writeln!(xml, "</AndroidAttestation>").unwrap();

    Ok(xml)
}

pub fn sort_cert_chain(certs: &[ParsedCertificate]) -> Vec<ParsedCertificate> {
    if certs.len() <= 1 {
        return certs.to_vec();
    }

    let mut leaf = None;
    for candidate in certs {
        let is_issuer = certs.iter().any(|other| {
            other.subject_der != other.issuer_der && other.issuer_der == candidate.subject_der
        });
        if !is_issuer {
            leaf = Some(candidate.clone());
            break;
        }
    }

    let Some(mut current) = leaf else {
        return certs.to_vec();
    };

    let mut ordered = vec![current.clone()];
    while let Some(next) = certs
        .iter()
        .find(|candidate| {
            candidate.subject_der == current.issuer_der
                && candidate.subject_der != current.subject_der
        })
        .cloned()
    {
        if ordered
            .iter()
            .any(|existing| existing.subject_der == next.subject_der)
        {
            break;
        }
        current = next.clone();
        ordered.push(next);
    }

    for cert in certs {
        if !ordered
            .iter()
            .any(|existing| existing.subject_der == cert.subject_der)
        {
            ordered.push(cert.clone());
        }
    }

    ordered
}

fn pem_wrap(label: &str, der: &[u8]) -> String {
    let encoded = Base64::encode_string(der);
    let mut pem = String::new();
    writeln!(pem, "-----BEGIN {label}-----").unwrap();
    for chunk in encoded.as_bytes().chunks(64) {
        writeln!(pem, "{}", String::from_utf8_lossy(chunk)).unwrap();
    }
    write!(pem, "-----END {label}-----").unwrap();
    pem
}

fn indent_block(block: &str, indent: &str) -> String {
    block
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_der_len(bytes: &[u8]) -> Result<(usize, usize)> {
    if bytes.len() < 2 {
        anyhow::bail!("DER sequence is truncated");
    }
    if bytes[0] != 0x30 {
        anyhow::bail!("DER certificate must start with SEQUENCE");
    }

    if bytes[1] & 0x80 == 0 {
        return Ok((2, usize::from(bytes[1])));
    }

    if bytes[1] == 0x80 {
        anyhow::bail!("DER indefinite lengths are not supported");
    }

    let count = usize::from(bytes[1] & 0x7F);
    if count == 0 {
        anyhow::bail!("DER length uses an invalid long-form encoding");
    }
    if count > std::mem::size_of::<usize>() {
        anyhow::bail!("DER length is too large");
    }
    let len_bytes = bytes
        .get(2..2 + count)
        .context("DER length bytes are truncated")?;
    let mut length = 0usize;
    for byte in len_bytes {
        length = (length << 8) | usize::from(*byte);
    }

    Ok((2 + count, length))
}

fn escape_xml_attribute(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{ParsedCertificate, build_keybox_xml, parse_der_cert_chain, sort_cert_chain};
    use base64ct::{Base64, Encoding};
    use p256::SecretKey;

    const TEST_CERT_DER_BASE64: &str = "MIICqTCCAZGgAwIBAgIJAL3L/xZFRhnmMA0GCSqGSIb3DQEBCwUAMBQxEjAQBgNVBAMTCWR1Y2stdGVzdDAeFw0yNjAzMjkxMDEwMDZaFw0yNjA0MjkxMDEwMDZaMBQxEjAQBgNVBAMTCWR1Y2stdGVzdDCCASIwDQYJKoZIhvcNAQEBBQADggEPADCCAQoCggEBAKHYqyiPX9viZpkNft3874CaGdTLP2G83ATQO/KRCwJGq6FZmej76CMLxxYplrCwG1tYc6Jr36KcDG0LbspAczsz3SUkXpeIuxaBPBtdpY7CoT05UjW9uJM+te6rWJ4fiR01hfqmjl0t02cS/zPYsRlaIAEg81Rj0lmpK6APS+GARz+OxIuSf9qURnG2O/92wFtIYVEjt2IrgCsL4i6/tYPuyFhv1TuX+QMOgNWyFGSbydexfCaY2GhT4Jvlvl7VPi1uTzPQzAFf636fRpuWuQSWzboich4REDPd/MrFizx/wDc9zVUt+Bv+x2PpRygTmiwoaAvgK7V0Mpa0EWlqhrUCAwEAATANBgkqhkiG9w0BAQsFAAOCAQEASImyG6qe+SQN7FSEkqDPeCJd9BQaTK+uFH/koESrOaRUZb6CDGXzBgbbxXuPZuoqD6EbzGX/Ca3/8sS4p7klt07uEot1l86/iyUY1DnAl/eTmEF3aoahOpsvSQEoEMb+qAOld6Oi6uVxBQYJ13LRJDxJ2lutRKlmk1Dyp03OgDK4iH5Ja5KkVm+VHMBgKNtnuF1A3cHISF+X8TzKDm08d85VitHjdNMrVcC+MPDAw8HDjVJzu7wMHh/xmqpunjjCJ2yujM3HLqp+uRRSJ+EyBoVheYLyxXwHAg/xHsT8vhqQBOFS39OAhqX/mDUfWcCUYXAwKTy/zeylTkeyZyUHSQ==";

    fn sample_cert_der() -> Vec<u8> {
        Base64::decode_vec(TEST_CERT_DER_BASE64).unwrap()
    }

    #[test]
    fn sort_cert_chain_orders_leaf_first() {
        let root = ParsedCertificate {
            der: vec![1],
            subject_der: vec![0x01],
            issuer_der: vec![0x01],
            subject_summary: "root".into(),
            issuer_summary: "root".into(),
        };
        let leaf = ParsedCertificate {
            der: vec![2],
            subject_der: vec![0x02],
            issuer_der: vec![0x01],
            subject_summary: "leaf".into(),
            issuer_summary: "root".into(),
        };

        let ordered = sort_cert_chain(&[root.clone(), leaf.clone()]);
        assert_eq!(ordered[0].subject_summary, "leaf");
        assert_eq!(ordered[1].subject_summary, "root");
    }

    #[test]
    fn keybox_xml_contains_expected_nodes() {
        let secret_key = SecretKey::from_slice(&[0x11; 32]).unwrap();
        let cert = ParsedCertificate {
            der: vec![0x30, 0x00],
            subject_der: vec![0x01],
            issuer_der: vec![0x01],
            subject_summary: "root".into(),
            issuer_summary: "root".into(),
        };
        let xml = build_keybox_xml(&secret_key, &[cert], "duck-123").unwrap();
        assert!(xml.contains("Made by Eltavine & MhmRdd"));
        assert!(xml.contains("<AndroidAttestation>"));
        assert!(xml.contains("DeviceID=\"duck-123\""));
        assert!(xml.contains("<CertificateChain>"));
    }

    #[test]
    fn keybox_xml_escapes_device_id_attribute() {
        let secret_key = SecretKey::from_slice(&[0x11; 32]).unwrap();
        let cert = ParsedCertificate {
            der: vec![0x30, 0x00],
            subject_der: vec![0x01],
            issuer_der: vec![0x01],
            subject_summary: "root".into(),
            issuer_summary: "root".into(),
        };
        let xml = build_keybox_xml(&secret_key, &[cert], "duck\"<&>'").unwrap();

        assert!(xml.contains("DeviceID=\"duck&quot;&lt;&amp;&gt;&apos;\""));
    }

    #[test]
    fn parse_der_cert_chain_parses_single_certificate() {
        let parsed = parse_der_cert_chain(&sample_cert_der()).unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn parse_der_cert_chain_rejects_truncated_certificate() {
        let mut der = sample_cert_der();
        der.pop();

        let error = parse_der_cert_chain(&der).unwrap_err();
        assert!(error.to_string().contains("truncated"));
    }

    #[test]
    fn parse_der_cert_chain_rejects_trailing_garbage() {
        let mut der = sample_cert_der();
        der.extend_from_slice(&[0x00, 0x01]);

        let error = parse_der_cert_chain(&der).unwrap_err();
        assert!(error.to_string().contains("trailing non-DER data"));
    }
}
