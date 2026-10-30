//! 属性区私有副本：建立、迁移 bionic context 节点、seal。
//!
//! 副本用 `MAP_PRIVATE|MAP_ANONYMOUS` 建立并逐字节复制，seal 后形态与 stock
//! 共享只读属性区一致（`r--p [anon:linker_alloc]` 或无名）。
//! 原始 `/dev/__properties__/*` 映射**不做任何改动**。

use std::cell::RefCell;

use log::info;
use prop_rs_android::mmap_prop_area::MmapPropArea;

use crate::prop_backend::PropBackend;

use super::anon_name::{CLONE_ANON_NAME, anon_vma_name_is, name_anon_vma, set_anon_name};
use super::mappings::PropAreaMapping;
use super::owned::{Owner, View};
use super::relabel::readable_context_label;
use std::rc::Rc;

/// 一个真实属性区的进程私有副本。副本随进程存活，**不回收**
/// （bionic 的 context 节点长期指向它，提前 munmap 会悬空）。
struct AreaClone {
    /// 原始 `/dev/__properties__/*` 映射起始地址
    orig_start: usize,
    clone_start: usize,
    clone_end: usize,
    owner: Rc<Owner>,
}

thread_local! {
    #[allow(clippy::missing_const_for_thread_local)]
    static AREA_CLONES: RefCell<Vec<AreaClone>> = const { RefCell::new(Vec::new()) };
}

// ── 可写属性区：私有副本 / 降级原地 COW ─────────────────────────────────

/// Acquire an exclusive view of an owned private clone. Foreign addresses are rejected.
pub(super) fn open_area(addr: usize, size: usize) -> anyhow::Result<View> {
    let owner = AREA_CLONES
        .with(|clones| {
            clones
                .borrow()
                .iter()
                .find(|clone| clone.clone_start == addr && clone.clone_end - addr == size)
                .map(|clone| clone.owner.clone())
        })
        .ok_or_else(|| anyhow::anyhow!("property area is not owned by this component"))?;
    make_writable(addr, size)?;
    Owner::borrow(owner)
}

/// A separate private file view for read-only inspection; its owner cannot unmap bionic's view.
pub(super) fn readonly_area(mapping: &PropAreaMapping) -> Option<MmapPropArea> {
    let file = std::fs::File::open(&mapping.path).ok()?;
    let size = mapping.end.checked_sub(mapping.start)?;
    if size < 128 || size > 16 * 1024 * 1024 {
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

/// `addr` 所属的**原始**属性区起始地址（若 `addr` 落在私有副本里，映射回原区）。
pub(super) fn original_area_start(addr: usize, mappings: &[PropAreaMapping]) -> Option<usize> {
    let from_clone = AREA_CLONES.with(|c| {
        c.borrow()
            .iter()
            .find(|cl| addr >= cl.clone_start && addr < cl.clone_end)
            .map(|cl| cl.orig_start)
    });
    from_clone.or_else(|| {
        mappings
            .iter()
            .find(|m| addr >= m.start && addr < m.end)
            .map(|m| m.start)
    })
}

/// 取包含 `addr` 的可写属性区，返回 `(区域起始地址, 大小)`。
pub(super) fn writable_area_for(
    addr: usize,
    mappings: &[PropAreaMapping],
    backend: Option<&PropBackend>,
) -> anyhow::Result<(usize, usize)> {
    if let Some(range) = clone_range_containing(addr) {
        return Ok(range);
    }
    let mapping = mappings
        .iter()
        .find(|m| addr >= m.start && addr < m.end)
        .ok_or_else(|| anyhow::anyhow!("prop_info at {addr:#x} not in any prop area mapping"))?;
    writable_area_for_mapping(mapping, mappings, backend)
}

/// 取某个**原始**属性区对应的可写区（已有副本直接复用，否则新建）。
pub(super) fn writable_area_for_mapping(
    mapping: &PropAreaMapping,
    _mappings: &[PropAreaMapping],
    backend: Option<&PropBackend>,
) -> anyhow::Result<(usize, usize)> {
    let size = mapping.end - mapping.start;
    if let Some(range) = clone_range_of(mapping.start) {
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
            Ok((create_area_clone(mapping, b)?, size))
        }
        None => anyhow::bail!("unsupported bionic context layout; foreign mappings are unchanged"),
    }
}

/// 已有副本按原区起始地址复用（顺带确保可写——`seal_area_clones` 之后需要）。
fn clone_range_of(orig_start: usize) -> Option<(usize, usize)> {
    AREA_CLONES
        .with(|c| {
            c.borrow()
                .iter()
                .find(|cl| cl.orig_start == orig_start)
                .map(|cl| (cl.clone_start, cl.clone_end - cl.clone_start))
        })
        .map(|(start, size)| {
            let _ = make_writable(start, size);
            (start, size)
        })
}

/// 已有副本按地址区间复用。
fn clone_range_containing(addr: usize) -> Option<(usize, usize)> {
    AREA_CLONES
        .with(|c| {
            c.borrow()
                .iter()
                .find(|cl| addr >= cl.clone_start && addr < cl.clone_end)
                .map(|cl| (cl.clone_start, cl.clone_end - cl.clone_start))
        })
        .map(|(start, size)| {
            let _ = make_writable(start, size);
            (start, size)
        })
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

/// patch 全部完成：尽量 seal 成只读；命名与 mprotect 在部分内核上互相打架，
/// 最终形态是 `r--p [anon:linker_alloc]` 或退回 `rw-p [anon:linker_alloc]`（stock 同有）。
pub(super) fn seal_area_clones() {
    AREA_CLONES.with(|c| {
        for cl in c.borrow().iter() {
            let size = cl.clone_end - cl.clone_start;
            let addr = cl.clone_start as *mut libc::c_void;
            // 必须在仍可写时命名（RO 后 PR_SET 会变成 [anon:<fault>]）
            name_anon_vma(addr, size);
            let named_rw = anon_vma_name_is(addr, CLONE_ANON_NAME.to_str().unwrap_or(""));
            let _ = unsafe { libc::mprotect(addr, size, libc::PROT_READ) };
            let named_ro = anon_vma_name_is(addr, CLONE_ANON_NAME.to_str().unwrap_or(""));
            info!("seal clone @{addr:p} size={size:#x} named_rw={named_rw} named_ro={named_ro}");
            if !named_ro {
                let _ = unsafe { libc::mprotect(addr, size, libc::PROT_READ | libc::PROT_WRITE) };
                set_anon_name(addr, size);
                let named_rw2 = anon_vma_name_is(addr, CLONE_ANON_NAME.to_str().unwrap_or(""));
                info!("seal clone @{addr:p} fallback rw named={named_rw2}");
            }
        }
    });
}

/// 把真实属性区整块复制到进程私有匿名映射，并让 bionic 的 context 节点指向副本。
///
/// 原始 `/dev/__properties__/*` 映射**不做任何改动**（权限形态保持 `r--s`）。
/// 副本用 MAP_PRIVATE|MAP_ANONYMOUS：seal 后呈 `r--p`，再视内核能力命名成
/// `[anon:linker_alloc]`（≥5.17）或保持无名（空路径，与 stock 无名 `r--p` 同形）。
/// （不用 MAP_SHARED|ANON——那会被内核挂成 `/dev/zero (deleted)`，反而非典型；
/// 也不 bind-mount 伪路径——maple 等 mountinfo 扫描直接命中。）
pub(super) fn create_area_clone(
    mapping: &PropAreaMapping,
    backend: &PropBackend,
) -> anyhow::Result<usize> {
    let size = mapping.end - mapping.start;
    let mut map = memmap2::MmapMut::map_anon(size)?;
    // SAFETY: proc maps supplies a live readable region and the owned destination has this exact size.
    unsafe { std::ptr::copy_nonoverlapping(mapping.start as *const u8, map.as_mut_ptr(), size) };
    let area = MmapPropArea::new(map)?;
    build_area_clone(area, mapping, size, backend)
}

fn build_area_clone(
    area: MmapPropArea,
    mapping: &PropAreaMapping,
    size: usize,
    backend: &PropBackend,
) -> anyhow::Result<usize> {
    let clone = area.as_ptr() as usize;
    let nodes = backend.repoint_area_and_relabel(mapping.start, clone, readable_context_label());
    if nodes == 0 {
        // 没有任何 context 节点引用这个区（正常情况下已被 `references_area`
        // 预检挡掉），迁移无意义 —— 回滚，避免留下一个无人使用的私有映射。
        anyhow::bail!("no context node references {:#x}", mapping.start);
    }

    AREA_CLONES.with(|c| {
        c.borrow_mut().push(AreaClone {
            orig_start: mapping.start,
            clone_start: clone,
            clone_end: clone + size,
            owner: Owner::new(area),
        })
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
