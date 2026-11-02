use super::*;
use crate::activation::Activation;
use std::{
    thread,
    time::{Duration, Instant},
};

#[test]
fn resolve_transfers_a_maximum_size_private_snapshot() {
    let (mut client, mut server) = UnixStream::pair().unwrap();
    let mut content = "[[apps]]\npackage='com.example.app@999'\nmodel='Phone'\n#".to_owned();
    // Tabs require JSON escaping while remaining valid TOML comment characters.
    content.push_str(&"\t".repeat(48 * 1024 - content.len()));
    let expected = content.clone();
    let worker = thread::spawn(move || {
        let CompanionRequest::Resolve(package) = read_companion_request(&mut server).unwrap()
        else {
            panic!("expected a profile resolution request");
        };
        assert_eq!(package, "com.example.app@999");
        write_companion_response(
            &mut server,
            &CompanionResponse {
                activation: Some(Activation {
                    generation: "initial".into(),
                    content,
                }),
                ..CompanionResponse::ok()
            },
        )
        .unwrap();
    });
    let request = encode_request(&CompanionRequest::Resolve("com.example.app@999".into())).unwrap();
    let response = exchange(&mut client, request).unwrap();
    worker.join().unwrap();
    assert_eq!(response.status, 0);
    let snapshot = response.activation.unwrap();
    assert_eq!(snapshot.generation, "initial");
    assert_eq!(snapshot.content, expected);
    assert!(crate::config::Config::from_toml(&snapshot.content).is_ok());
}

#[test]
fn generation_rejection_and_an_unselected_target_remain_distinct() {
    for selected in [false, true] {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            let request = read_companion_request(&mut server).unwrap();
            let response = if selected {
                let CompanionRequest::CheckGeneration(generation) = request else {
                    panic!("expected a generation check");
                };
                assert_eq!(generation, "old-generation");
                CompanionResponse::err("engine disabled")
            } else {
                assert!(matches!(request, CompanionRequest::Resolve(_)));
                CompanionResponse::ok()
            };
            write_companion_response(&mut server, &response).unwrap();
        });
        let request = if selected {
            CompanionRequest::CheckGeneration("old-generation".into())
        } else {
            CompanionRequest::Resolve("com.other.app@0".into())
        };
        let response = exchange(&mut client, encode_request(&request).unwrap()).unwrap();
        worker.join().unwrap();
        assert!(response.activation.is_none());
        assert_eq!(response.status, if selected { -1 } else { 0 });
        assert_eq!(
            response.message.as_deref(),
            selected.then_some("engine disabled")
        );
    }
}

#[test]
fn fragmented_headers_and_payloads_are_reassembled() {
    let (client, mut server) = UnixStream::pair().unwrap();
    let payload = encode_request(&CompanionRequest::CheckGeneration("next".into())).unwrap();
    let expected = CompanionResponse::err("engine disabled");
    let worker = thread::spawn(move || {
        let request = read_companion_request(&mut server).unwrap();
        assert!(matches!(request, CompanionRequest::CheckGeneration(value) if value == "next"));
        let bytes = serde_json::to_vec(&expected).unwrap();
        let frame: Vec<_> = (bytes.len() as u32)
            .to_le_bytes()
            .into_iter()
            .chain(bytes)
            .collect();
        for byte in frame {
            server.write_all(&[byte]).unwrap();
        }
    });
    // Exercise the server's framing with partial writes as well as the client's reader.
    let request: Vec<_> = (payload.len() as u32)
        .to_le_bytes()
        .into_iter()
        .chain(payload)
        .collect();
    client.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    for byte in request {
        transfer(&client, &mut [byte], true, deadline).unwrap();
    }
    let response: CompanionResponse =
        serde_json::from_slice(&frame(&client, deadline).unwrap()).unwrap();
    assert_eq!(response.status, -1);
    assert_eq!(response.message.as_deref(), Some("engine disabled"));
    worker.join().unwrap();
}

#[test]
fn invalid_lengths_and_truncated_frames_fail_without_waiting_for_a_payload() {
    for bytes in [
        0_u32.to_le_bytes().to_vec(),
        (MAX_FRAME as u32 + 1).to_le_bytes().to_vec(),
        vec![1, 0],
        vec![4, 0, 0, 0, b'{'],
    ] {
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(&bytes).unwrap();
        drop(writer);
        assert!(read_companion_request(&mut reader).is_err());
    }
}

#[test]
fn malformed_or_closed_responses_never_become_success() {
    for bytes in [b"invalid".as_slice(), b"{}", b"{\"status\":\"0\"}", b""] {
        let (mut client, mut server) = UnixStream::pair().unwrap();
        let bytes = bytes.to_vec();
        let worker = thread::spawn(move || {
            read_companion_request(&mut server).unwrap();
            server
                .write_all(&(bytes.len() as u32).to_le_bytes())
                .unwrap();
            server.write_all(&bytes).unwrap();
        });
        let payload = encode_request(&CompanionRequest::CheckGeneration("initial".into())).unwrap();
        assert!(exchange(&mut client, payload).is_err());
        worker.join().unwrap();
    }
}

#[test]
fn oversized_requests_and_responses_are_rejected_before_writing() {
    assert!(encode_request(&CompanionRequest::Resolve("x".repeat(MAX_FRAME))).is_err());
    let (mut writer, mut reader) = UnixStream::pair().unwrap();
    let response = CompanionResponse::err("x".repeat(MAX_FRAME));
    assert!(write_companion_response(&mut writer, &response).is_err());
    reader.set_nonblocking(true).unwrap();
    let mut byte = [0];
    assert_eq!(
        reader.read(&mut byte).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn a_stalled_companion_response_has_a_deadline() {
    let (mut client, _server) = UnixStream::pair().unwrap();
    let started = Instant::now();
    let payload = encode_request(&CompanionRequest::CheckGeneration("initial".into())).unwrap();
    let error = exchange(&mut client, payload).unwrap_err();
    assert!(error.to_string().contains("timed out"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
}
