//! Owned mapping views. Foreign bionic mappings are never manufactured into Rust owners.
use prop_rs_android::mmap_prop_area::MmapPropArea;
use std::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    rc::Rc,
};

pub(super) struct Owner {
    area: UnsafeCell<ManuallyDrop<MmapPropArea>>,
    borrowed: Cell<bool>,
}
impl Owner {
    pub(super) fn new(area: MmapPropArea) -> Rc<Self> {
        // The mapping must outlive the module: bionic retains its prop_info/context pointers.
        Rc::new(Self {
            area: UnsafeCell::new(ManuallyDrop::new(area)),
            borrowed: Cell::new(false),
        })
    }
    pub(super) fn borrow(owner: Rc<Self>) -> anyhow::Result<View> {
        if owner.borrowed.replace(true) {
            anyhow::bail!("property area is already borrowed");
        }
        Ok(View(owner))
    }
}
pub(super) struct View(Rc<Owner>);
impl Deref for View {
    type Target = MmapPropArea;
    fn deref(&self) -> &Self::Target {
        // SAFETY: construction takes the only view on this thread; Rc prevents cross-thread use.
        unsafe { &*self.0.area.get() }
    }
}
impl DerefMut for View {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: the borrow flag remains held until this guard is dropped.
        unsafe { &mut *self.0.area.get() }
    }
}
impl Drop for View {
    fn drop(&mut self) {
        self.0.borrowed.set(false);
    }
}
