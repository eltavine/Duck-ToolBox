//! A separately owned serial scratch area; real global property notifications remain intact.
use super::{area::readonly_area, mappings::PropAreaMapping};
use prop_rs_android::mmap_prop_area::MmapPropArea;
pub(super) fn prepare_serial_area(mappings: &[PropAreaMapping]) -> anyhow::Result<MmapPropArea> {
    let mapping = mappings
        .iter()
        .find(|entry| entry.path.ends_with("/properties_serial"))
        .ok_or_else(|| anyhow::anyhow!("properties_serial mapping not found"))?;
    let source =
        readonly_area(mapping).ok_or_else(|| anyhow::anyhow!("serial area is unreadable"))?;
    Ok(MmapPropArea::new_anon_from(&source)?)
}
