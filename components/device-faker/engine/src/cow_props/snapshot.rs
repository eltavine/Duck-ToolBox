//! Validate a private bionic snapshot without waiting on copied futex/dirty state.
use anyhow::{Result, bail, ensure};
use std::{
    collections::HashSet,
    sync::atomic::{Ordering, fence},
};

const HEADER: usize = 128;
const TRIE_HEADER: usize = 20;
const PROP_HEADER: usize = 96;
const LONG_FLAG: u32 = 1 << 16;

#[derive(Debug, PartialEq, Eq)]
struct Layout {
    used: usize,
    serials: Vec<(usize, u32)>,
}
fn word(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| anyhow::anyhow!("property offset overflow"))?;
    Ok(u32::from_ne_bytes(
        bytes
            .get(offset..end)
            .ok_or_else(|| anyhow::anyhow!("property offset outside snapshot"))?
            .try_into()?,
    ))
}
fn inspect(bytes: &[u8]) -> Result<Layout> {
    let used = word(bytes, 0)? as usize;
    ensure!(
        used >= TRIE_HEADER && used.is_multiple_of(4),
        "property trie is incomplete"
    );
    let data = bytes
        .get(
            HEADER
                ..HEADER
                    .checked_add(used)
                    .ok_or_else(|| anyhow::anyhow!("property size overflow"))?,
        )
        .ok_or_else(|| anyhow::anyhow!("property allocation outside snapshot"))?;
    let mut pending = vec![0usize];
    let mut visited = HashSet::new();
    let mut serials = Vec::new();
    while let Some(node) = pending.pop() {
        ensure!(
            node.is_multiple_of(4) && visited.insert(node),
            "cyclic or unaligned property trie"
        );
        let name_len = word(data, node)? as usize;
        let name_start = node
            .checked_add(TRIE_HEADER)
            .ok_or_else(|| anyhow::anyhow!("property node overflow"))?;
        ensure!(name_start <= data.len(), "property node outside snapshot");
        if node != 0 {
            let name_end = name_start
                .checked_add(name_len)
                .ok_or_else(|| anyhow::anyhow!("property name overflow"))?;
            ensure!(
                name_len > 0 && data.get(name_end) == Some(&0),
                "incomplete property trie name"
            );
        }
        let property = word(data, node + 4)? as usize;
        if property != 0 {
            ensure!(property.is_multiple_of(4), "unaligned property record");
            let serial = word(data, property)?;
            ensure!(
                serial & 1 == 0,
                "snapshot contains an unfinished property update"
            );
            let name_start = property
                .checked_add(PROP_HEADER)
                .ok_or_else(|| anyhow::anyhow!("property record overflow"))?;
            let name_bytes = data
                .get(name_start..)
                .ok_or_else(|| anyhow::anyhow!("property name outside snapshot"))?;
            let name_len = name_bytes
                .iter()
                .position(|byte| *byte == 0)
                .ok_or_else(|| anyhow::anyhow!("unterminated property name"))?;
            ensure!(name_len > 0, "empty property name");
            // Mutable counters can set bit 16 too; bionic interprets kLongFlag only for ro.*.
            if name_bytes.starts_with(b"ro.") && serial & LONG_FLAG != 0 {
                let start = property
                    .checked_add(word(data, property + 60)? as usize)
                    .ok_or_else(|| anyhow::anyhow!("long property overflow"))?;
                ensure!(
                    start > name_start + name_len,
                    "long property overlaps its record"
                );
                ensure!(
                    data.get(start..).is_some_and(|value| value.contains(&0)),
                    "unterminated long property"
                );
            } else {
                let length = (serial >> 24) as usize;
                ensure!(
                    length < 92 && data.get(property + 4 + length) == Some(&0),
                    "incomplete inline property"
                );
            }
            serials.push((HEADER + property, serial));
        }
        for field in [8, 12, 16] {
            let child = word(data, node + field)? as usize;
            if child != 0 {
                pending.push(child);
            }
        }
    }
    serials.sort_unstable();
    serials.dedup();
    Ok(Layout { used, serials })
}

/// `copy` reads the validated live mapping; `read` performs aligned atomic source loads.
/// Two copies bracket all property values with their source serials. Changed allocations,
/// records or serials invalidate the attempt; no cloned dirty bit is ever waited on.
pub(super) fn capture(
    destination: &mut [u8],
    mut copy: impl FnMut(&mut [u8]),
    mut read: impl FnMut(usize) -> u32,
) -> Result<()> {
    for _ in 0..3 {
        copy(destination);
        let Ok(before) = inspect(destination) else {
            continue;
        };
        if read(0) as usize != before.used
            || !before
                .serials
                .iter()
                .all(|(offset, serial)| read(*offset) == *serial)
        {
            continue;
        }
        copy(destination);
        fence(Ordering::Acquire);
        if read(0) as usize == before.used
            && inspect(destination).is_ok_and(|after| after == before)
            && before
                .serials
                .iter()
                .all(|(offset, serial)| read(*offset) == *serial)
        {
            return Ok(());
        }
    }
    bail!("property area changed or remained incomplete while copying")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn area() -> Vec<u8> {
        let mut bytes = vec![0; 512];
        for (offset, value) in [
            (0, 256u32),
            (HEADER + 16, 20),
            (HEADER + 20, 4),
            (HEADER + 24, 48),
            (HEADER + 48, 3 << 24),
        ] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
        }
        bytes[HEADER + 40..HEADER + 45].copy_from_slice(b"demo\0");
        bytes[HEADER + 52..HEADER + 56].copy_from_slice(b"old\0");
        bytes[HEADER + 144..HEADER + 154].copy_from_slice(b"sys.demo\0\0");
        bytes
    }
    #[test]
    fn stable_values_are_copied_without_modifying_the_source() {
        let source = area();
        let before = source.clone();
        let mut destination = vec![0; source.len()];
        capture(
            &mut destination,
            |out| out.copy_from_slice(&source),
            |offset| word(&source, offset).unwrap(),
        )
        .unwrap();
        assert_eq!(destination, source);
        assert_eq!(source, before);
    }
    #[test]
    fn a_transient_dirty_copy_is_retried_before_publication() {
        let source = area();
        let mut destination = vec![0; source.len()];
        let mut copies = 0;
        capture(
            &mut destination,
            |out| {
                out.copy_from_slice(&source);
                copies += 1;
                if copies == 1 {
                    out[HEADER + 48] |= 1;
                }
            },
            |offset| word(&source, offset).unwrap(),
        )
        .unwrap();
        assert_eq!(copies, 3);
        assert_eq!(destination, source);
    }
    #[test]
    fn mutable_counter_bits_and_read_only_long_values_remain_supported() {
        let mut source = area();
        let offset = HEADER + 48;
        source[offset..offset + 4].copy_from_slice(&((3 << 24) | LONG_FLAG).to_ne_bytes());
        assert!(
            inspect(&source).is_ok(),
            "mutable serial bit 16 is a counter, not a long-value flag"
        );
        source[HEADER + 144..HEADER + 154].fill(0);
        source[HEADER + 144..HEADER + 152].copy_from_slice(b"ro.demo\0");
        source[offset..offset + 4].copy_from_slice(&((49 << 24) | LONG_FLAG).to_ne_bytes());
        source[HEADER + 108..HEADER + 112].copy_from_slice(&144u32.to_ne_bytes());
        source[HEADER + 192..HEADER + 197].copy_from_slice(b"long\0");
        assert!(inspect(&source).is_ok());
    }
    #[test]
    fn dirty_or_invalid_snapshots_fail_without_waiting() {
        let original = area();
        for (offset, value) in [
            (HEADER + 48, (3u32 << 24) | 1),
            (HEADER + 20 + 16, 20),
            (HEADER + 20 + 4, 508),
            (0, 600),
        ] {
            let mut source = original.clone();
            source[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
            let mut destination = vec![0; source.len()];
            let mut copies = 0;
            assert!(
                capture(
                    &mut destination,
                    |out| {
                        copies += 1;
                        out.copy_from_slice(&source);
                    },
                    |offset| word(&source, offset).unwrap()
                )
                .is_err()
            );
            assert!(copies <= 6);
        }
    }
    #[test]
    fn a_source_serial_change_during_the_value_copy_rejects_the_attempt() {
        use std::cell::Cell;
        let source = area();
        let generation = Cell::new(0);
        let copies = Cell::new(0usize);
        let mut destination = vec![0; source.len()];
        assert!(
            capture(
                &mut destination,
                |out| {
                    out.copy_from_slice(&source);
                    let serial = word(&source, HEADER + 48).unwrap() + generation.get();
                    out[HEADER + 48..HEADER + 52].copy_from_slice(&serial.to_ne_bytes());
                    copies.set(copies.get() + 1);
                    if copies.get().is_multiple_of(2) {
                        generation.set(generation.get() + 2);
                    }
                },
                |offset| {
                    let value = word(&source, offset).unwrap();
                    if offset == HEADER + 48 {
                        value + generation.get()
                    } else {
                        value
                    }
                }
            )
            .is_err()
        );
    }
}
