//! 属性区私有副本：建立、迁移 bionic context 节点、seal。
//!
//! 副本用 `MAP_PRIVATE|MAP_ANONYMOUS` 建立并逐字节复制，seal 后形态与 stock
//! 共享只读属性区一致（`r--p [anon:linker_alloc]` 或无名）。
//! 原始 `/dev/__properties__/*` 映射**不做任何改动**。

use std::mem::ManuallyDrop;

use log::{info, warn};
use prop_rs_android::mmap_prop_area::MmapPropArea;

use crate::prop_backend::PropBackend;

use super::anon_name::name_anon_vma;
use super::mappings::PropAreaMapping;
use super::relabel::readable_context_label;

/// 一个真实属性区的进程私有副本。副本随进程存活，**不回收**
/// （bionic 的 context 节点长期指向它，提前 munmap 会悬空）。
struct AreaClone {
    /// 原始 `/dev/__properties__/*` 映射起始地址
    orig_start: usize,
    clone_start: usize,
    clone_end: usize,
    area: ManuallyDrop<MmapPropArea>,
}

/// Clone metadata is owned by a single patch pass. Only published mappings outlive it.
#[derive(Default)]
pub(super) struct AreaClones(Vec<AreaClone>);

impl AreaClones {
    /// Acquire an exclusive view of an owned private clone. Foreign addresses are rejected.
    pub(super) fn open_area(
        &mut self,
        addr: usize,
        size: usize,
    ) -> anyhow::Result<&mut MmapPropArea> {
        let clone = self
            .0
            .iter_mut()
            .find(|clone| clone.clone_start == addr && clone.clone_end - addr == size)
            .ok_or_else(|| anyhow::anyhow!("property area is not owned by this component"))?;
        make_writable(addr, size)?;
        Ok(&mut clone.area)
    }

    /// `addr` 所属的**原始**属性区起始地址（若 `addr` 落在私有副本里，映射回原区）。
    pub(super) fn original_area_start(
        &self,
        addr: usize,
        mappings: &[PropAreaMapping],
    ) -> Option<usize> {
        let from_clone = self
            .0
            .iter()
            .find(|cl| addr >= cl.clone_start && addr < cl.clone_end)
            .map(|cl| cl.orig_start);
        from_clone.or_else(|| {
            mappings
                .iter()
                .find(|m| addr >= m.start && addr < m.end)
                .map(|m| m.start)
        })
    }

    /// 取包含 `addr` 的可写属性区，返回 `(区域起始地址, 大小)`。
    pub(super) fn writable_area_for(
        &mut self,
        addr: usize,
        mappings: &[PropAreaMapping],
        backend: Option<&PropBackend>,
    ) -> anyhow::Result<(usize, usize)> {
        if let Some(range) = self.clone_range_containing(addr) {
            return Ok(range);
        }
        let mapping = mappings
            .iter()
            .find(|m| addr >= m.start && addr < m.end)
            .ok_or_else(|| {
                anyhow::anyhow!("prop_info at {addr:#x} not in any prop area mapping")
            })?;
        self.writable_area_for_mapping(mapping, backend)
    }

    /// 取某个**原始**属性区对应的可写区（已有副本直接复用，否则新建）。
    pub(super) fn writable_area_for_mapping(
        &mut self,
        mapping: &PropAreaMapping,
        backend: Option<&PropBackend>,
    ) -> anyhow::Result<(usize, usize)> {
        let size = mapping.end - mapping.start;
        if let Some(range) = self.clone_range_of(mapping.start) {
            return Ok(range);
        }
        match backend {
            // 整块克隆 + 迁移 context 节点指针，原映射不动
            Some(b) => {
                // 同一份属性区文件在本进程里可能被映射多次（活跃/非活跃两套
                // ContextsSerialized 各映射一遍）。只有被 context 节点引用的那一份
                // 才值得克隆 —— 否则克隆出来也没有指针指向它，白 mmap/munmap 一轮。
                if !b.references_area(mapping.start) {
                    anyhow::bail!(
                        "no context node references {:#x} ({path})",
                        mapping.start,
                        path = mapping.path
                    );
                }
                Ok((self.create_area_clone(mapping, b)?, size))
            }
            None => {
                anyhow::bail!("unsupported bionic context layout; foreign mappings are unchanged")
            }
        }
    }

    /// 已有副本按原区起始地址复用；写权限由 open_area 统一检查。
    fn clone_range_of(&self, orig_start: usize) -> Option<(usize, usize)> {
        self.0
            .iter()
            .find(|cl| cl.orig_start == orig_start)
            .map(|cl| (cl.clone_start, cl.clone_end - cl.clone_start))
    }

    /// 已有副本按地址区间复用。
    fn clone_range_containing(&self, addr: usize) -> Option<(usize, usize)> {
        self.0
            .iter()
            .find(|cl| addr >= cl.clone_start && addr < cl.clone_end)
            .map(|cl| (cl.clone_start, cl.clone_end - cl.clone_start))
    }

    /// patch 全部完成后设为只读；匿名映射命名失败不会重新开放写权限。
    pub(super) fn seal(&self) {
        for cl in &self.0 {
            let size = cl.clone_end - cl.clone_start;
            let addr = cl.clone_start as *mut libc::c_void;
            name_anon_vma(addr, size);
            if unsafe { libc::mprotect(addr, size, libc::PROT_READ) } != 0 {
                warn!(
                    "property clone could not be made read-only: {}",
                    std::io::Error::last_os_error()
                );
            } else {
                info!("sealed property clone @{addr:p} size={size:#x}");
            }
        }
    }

    /// 把真实属性区整块复制到进程私有匿名映射，并让 bionic 的 context 节点指向副本。
    ///
    /// 原始 `/dev/__properties__/*` 映射**不做任何改动**（权限形态保持 `r--s`）。
    /// 副本用 MAP_PRIVATE|MAP_ANONYMOUS：seal 后呈 `r--p`，再视内核能力命名成
    /// `[anon:linker_alloc]`（≥5.17）或保持无名（空路径，与 stock 无名 `r--p` 同形）。
    /// （不用 MAP_SHARED|ANON——那会被内核挂成 `/dev/zero (deleted)`，反而非典型；
    /// 也不 bind-mount 伪路径——maple 等 mountinfo 扫描直接命中。）
    fn create_area_clone(
        &mut self,
        mapping: &PropAreaMapping,
        backend: &PropBackend,
    ) -> anyhow::Result<usize> {
        let size = mapping.end - mapping.start;
        let mut map = memmap2::MmapMut::map_anon(size)?;
        super::snapshot::capture(
            &mut map,
            |destination| {
                // SAFETY: the validated source mapping has this exact readable extent.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        mapping.start as *const u8,
                        destination.as_mut_ptr(),
                        size,
                    )
                };
            },
            |offset| {
                // SAFETY: snapshot validation bounds every aligned serial/allocation field.
                unsafe {
                    (&*((mapping.start + offset) as *const std::sync::atomic::AtomicU32))
                        .load(std::sync::atomic::Ordering::Acquire)
                }
            },
        )?;
        let area = MmapPropArea::new(map)?;
        self.publish_area_clone(area, mapping, size, backend)
    }

    fn publish_area_clone(
        &mut self,
        area: MmapPropArea,
        mapping: &PropAreaMapping,
        size: usize,
        backend: &PropBackend,
    ) -> anyhow::Result<usize> {
        self.0.try_reserve(1)?;
        let clone = area.as_ptr() as usize;
        let nodes =
            backend.repoint_area_and_relabel(mapping.start, clone, readable_context_label());
        if nodes == 0 {
            // 没有任何 context 节点引用这个区（正常情况下已被 `references_area`
            // 预检挡掉），迁移无意义 —— 回滚，避免留下一个无人使用的私有映射。
            anyhow::bail!("no context node references {:#x}", mapping.start);
        }

        self.0.push(AreaClone {
            orig_start: mapping.start,
            clone_start: clone,
            clone_end: clone + size,
            area: ManuallyDrop::new(area),
        });

        info!(
            "cloned prop area {path} [{start:#x}-{end:#x}] -> [{clone:#x}-{cend:#x}] ({nodes} context node(s) repointed)",
            path = mapping.path,
            start = mapping.start,
            end = mapping.end,
            cend = clone + size,
        );
        Ok(clone)
    }
}

/// A separate private file view for read-only inspection; its owner cannot unmap bionic's view.
pub(super) fn readonly_area(mapping: &PropAreaMapping) -> Option<MmapPropArea> {
    let file = std::fs::File::open(&mapping.path).ok()?;
    let size = mapping.end.checked_sub(mapping.start)?;
    if !(128..=16 * 1024 * 1024).contains(&size) {
        return None;
    }
    // SAFETY: the known property-area file remains mapped for this view's lifetime.
    // AOSP property-area files have stable size; atomic reads are implemented by MmapPropArea.
    let map = unsafe {
        memmap2::MmapOptions::new()
            .offset(mapping.offset)
            .len(size)
            .map_copy(&file)
    }
    .ok()?;
    MmapPropArea::new(map).ok()
}

fn make_writable(addr: usize, size: usize) -> anyhow::Result<()> {
    let ret = unsafe {
        libc::mprotect(
            addr as *mut libc::c_void,
            size,
            libc::PROT_READ | libc::PROT_WRITE,
        )
    };
    if ret != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
