use super::PROP_PREFIX;
use std::collections::HashSet;

pub(super) struct Region {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) offset: u64,
    pub(super) readable: bool,
    pub(super) writable: bool,
    pub(super) exec: bool,
    /// `major:minor`，与 `inode` 一起标识映射来源的文件
    pub(super) dev: String,
    pub(super) inode: u64,
    pub(super) path: String,
}

pub(super) struct Maps {
    pub(super) regions: Vec<Region>,
}

impl Maps {
    pub(super) fn load() -> Self {
        let mut regions = Vec::new();
        if let Ok(text) = std::fs::read_to_string("/proc/self/maps") {
            for line in text.lines() {
                if let Some(r) = Region::parse(line) {
                    regions.push(r);
                }
            }
        }
        Self { regions }
    }

    /// `[addr, addr+len)` 是否完整落在某个映射内。
    pub(super) fn readable(&self, addr: usize, len: usize) -> bool {
        if addr == 0 {
            return false;
        }
        let Some(end) = addr.checked_add(len) else {
            return false;
        };
        self.regions
            .iter()
            .any(|r| r.readable && addr >= r.start && end <= r.end)
    }

    pub(super) fn writable(&self, addr: usize, len: usize) -> bool {
        addr.checked_add(len).is_some_and(|end| {
            self.regions
                .iter()
                .any(|r| r.writable && r.readable && addr >= r.start && end <= r.end)
        })
    }

    pub(super) fn context_name(&self, addr: usize) -> bool {
        let Some(region) = self.region_of(addr).filter(|r| r.readable) else {
            return false;
        };
        let len = (region.end - addr).min(256);
        // The maps snapshot bounds every byte read; no unbounded C string scan.
        let bytes = unsafe { std::slice::from_raw_parts(addr as *const u8, len) };
        let Some(end) = bytes.iter().position(|b| *b == 0) else {
            return false;
        };
        bytes[..end].starts_with(b"u:object_r:") && bytes[..end].ends_with(b":s0")
    }

    pub(super) fn region_of(&self, addr: usize) -> Option<&Region> {
        self.regions
            .iter()
            .find(|r| addr >= r.start && addr < r.end)
    }

    /// 本进程所有 `/dev/__properties__/*` 映射的起始地址。
    /// bionic 的 `ContextNode::prop_area_` 就等于映射起始地址（file offset 0）。
    pub(super) fn prop_area_starts(&self) -> HashSet<usize> {
        self.regions
            .iter()
            .filter(|r| r.path.starts_with(PROP_PREFIX) && r.offset == 0)
            .map(|r| r.start)
            .collect()
    }
}

impl Region {
    pub(super) fn parse(line: &str) -> Option<Self> {
        let mut ws = line.split_whitespace();
        let range = ws.next()?;
        let perms = ws.next()?;
        let offset = ws.next()?;
        let dev = ws.next()?;
        let inode = ws.next()?;
        let path = ws.next().unwrap_or("");
        let (start_s, end_s) = range.split_once('-')?;
        Some(Self {
            start: usize::from_str_radix(start_s, 16).ok()?,
            end: usize::from_str_radix(end_s, 16).ok()?,
            offset: u64::from_str_radix(offset, 16).ok()?,
            readable: perms.as_bytes().first() == Some(&b'r'),
            writable: perms.as_bytes().get(1) == Some(&b'w'),
            exec: perms.as_bytes().get(2) == Some(&b'x'),
            dev: dev.to_string(),
            inode: inode.parse::<u64>().ok()?,
            path: path.to_string(),
        })
    }

    /// 与另一个映射是否来自同一个文件（同一 `dev:inode`）。
    pub(super) fn same_file(&self, other: &Self) -> bool {
        self.inode != 0 && self.dev == other.dev && self.inode == other.inode
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unreadable_mappings_overflow_and_partial_spans() {
        let maps = Maps {
            regions: vec![
                Region::parse("1000-2000 ---p 00000000 00:00 0").unwrap(),
                Region::parse("3000-4000 r--p 00000000 00:00 0").unwrap(),
                Region::parse("5000-6000 rw-p 00000000 00:00 0").unwrap(),
            ],
        };
        assert!(!maps.readable(0x1000, 8));
        assert!(maps.readable(0x3000, 8));
        assert!(!maps.writable(0x3000, 8));
        assert!(maps.writable(0x5000, 0x1000));
        assert!(!maps.writable(0x5000, 0x1001));
        assert!(!maps.readable(usize::MAX - 1, 8));
        assert!(!maps.readable(0, 8));
    }
    #[test]
    fn bounded_context_names_require_a_terminator_and_selinux_shape() {
        let bytes = b"u:object_r:default_prop:s0\0";
        let start = bytes.as_ptr() as usize;
        let mut region = Region::parse("1000-2000 r--p 00000000 00:00 0").unwrap();
        region.start = start;
        region.end = start + bytes.len();
        let mut maps = Maps {
            regions: vec![region],
        };
        assert!(maps.context_name(start));
        maps.regions[0].end -= 1;
        assert!(!maps.context_name(start));
        assert!(!maps.context_name(usize::MAX));
    }
}
