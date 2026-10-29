use crate::{fail, model::Identity};
use anyhow::Result;
use std::{fs::File, io, path::Path};
pub fn pidfd(_: u32) -> Result<File> {
    Err(fail(
        "unsupported_environment",
        "Linux PID file descriptors required",
    ))
}
pub fn checked_pidfd(_: &Identity) -> Result<File> {
    pidfd(0)
}
pub fn signal(_: &File, _: i32) -> Result<()> {
    Err(fail("unsupported_environment", "Linux signals required"))
}
pub fn exited(_: &File) -> bool {
    false
}
pub fn isolate() -> Result<bool> {
    Err(fail("unsupported_environment", "Linux namespaces required"))
}
pub fn enter(_: &File, _: Option<&File>) -> Result<()> {
    isolate().map(|_| ())
}
pub struct ChildSetup;
impl ChildSetup {
    pub fn from_root_fd(_: File) -> Result<Self> {
        Self::new(Path::new("/"))
    }
    pub fn new(_: &Path) -> Result<Self> {
        Err(fail("unsupported_environment", "Linux chroot required"))
    }
    pub fn in_scope(self, _: Option<File>) -> Self {
        self
    }
    pub fn apply(&self, _: bool) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Linux chroot required",
        ))
    }
}
pub fn nonblocking(_: i32) -> Result<()> {
    Err(fail("unsupported_environment", "Linux pipes required"))
}
pub fn detach(_: &mut std::process::Command) {}
pub struct Signals;
impl Signals {
    pub fn new() -> Result<Self> {
        Err(fail("unsupported_environment", "Linux signals required"))
    }
    pub fn requested(&self) -> Result<bool> {
        Ok(false)
    }
}
