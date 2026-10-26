//! Application protection policy, separate from kernel unload eligibility.

/// KernelSU's official Kbuild target is kernelsu.ko. Removing it can remove the
/// root facility used to execute operations and receive their results.
pub fn protected(name: &str) -> bool {
    name == "kernelsu"
}

#[cfg(test)]
mod tests {
    #[test]
    fn protects_the_known_root_core_without_treating_other_names_as_safe() {
        assert!(super::protected("kernelsu"));
        assert!(!super::protected("kernelsu_helper"));
        assert!(!super::protected("sample"));
    }
}
