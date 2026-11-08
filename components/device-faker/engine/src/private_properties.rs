//! Process-local C ABI for consumers of the existing-property COW path.
use std::{collections::HashMap, ffi::CStr};

#[repr(C)]
struct Property {
    name: *const libc::c_char,
    value: *const libc::c_char,
}

/// The caller owns the array and its NUL-terminated strings until this call returns.
#[unsafe(no_mangle)]
unsafe extern "C" fn duck_faker_patch_existing(entries: *const Property, count: usize) -> bool {
    if entries.is_null() || count == 0 || count > 1024 {
        return false;
    }
    let mut properties = HashMap::with_capacity(count);
    for entry in unsafe { std::slice::from_raw_parts(entries, count) } {
        if entry.name.is_null() || entry.value.is_null() {
            return false;
        }
        let (Ok(name), Ok(value)) = (
            unsafe { CStr::from_ptr(entry.name) }.to_str(),
            unsafe { CStr::from_ptr(entry.value) }.to_str(),
        ) else {
            return false;
        };
        if name.is_empty() || name.len() > 128 || value.is_empty() || value.len() > 4096 {
            return false;
        }
        properties.insert(name.to_owned(), value.to_owned());
    }
    crate::cow_props::patch_existing(&properties).unwrap_or(false)
}
