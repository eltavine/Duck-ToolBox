// SPDX-License-Identifier: GPL-3.0-only
//! Pixel identities and launch API levels, independent of the selected release SDK.
//! Sources and availability limits: ../../RULES.md, “增量配置与诊断契约”.
pub const DEFAULT: &str = "bluejay";
pub const DEVICES: &[(&str, &str, u32)] = &[
    ("cubs", "Pixel 11", 37),
    ("grizzly", "Pixel 11 Pro", 37),
    ("kodiak", "Pixel 11 Pro XL", 37),
    ("yogi", "Pixel 11 Pro Fold", 37),
    ("stallion", "Pixel 10a", 36),
    ("frankel", "Pixel 10", 36),
    ("blazer", "Pixel 10 Pro", 36),
    ("mustang", "Pixel 10 Pro XL", 36),
    ("rango", "Pixel 10 Pro Fold", 36),
    ("tegu", "Pixel 9a", 35),
    ("tokay", "Pixel 9", 34),
    ("caiman", "Pixel 9 Pro", 34),
    ("komodo", "Pixel 9 Pro XL", 34),
    ("comet", "Pixel 9 Pro Fold", 34),
    ("akita", "Pixel 8a", 34),
    ("shiba", "Pixel 8", 34),
    ("husky", "Pixel 8 Pro", 34),
    ("felix", "Pixel Fold", 33),
    ("tangorpro", "Pixel Tablet", 33),
    ("lynx", "Pixel 7a", 33),
    ("panther", "Pixel 7", 33),
    ("cheetah", "Pixel 7 Pro", 33),
    ("bluejay", "Pixel 6a", 32),
    ("oriole", "Pixel 6", 31),
    ("raven", "Pixel 6 Pro", 31),
    ("barbet", "Pixel 5a", 30),
    ("redfin", "Pixel 5", 30),
    ("bramble", "Pixel 4a (5G)", 30),
    ("sunfish", "Pixel 4a", 29),
    ("flame", "Pixel 4", 29),
    ("coral", "Pixel 4 XL", 29),
    ("sargo", "Pixel 3a", 28),
    ("bonito", "Pixel 3a XL", 28),
    ("blueline", "Pixel 3", 28),
    ("crosshatch", "Pixel 3 XL", 28),
    ("walleye", "Pixel 2", 26),
    ("taimen", "Pixel 2 XL", 26),
    ("sailfish", "Pixel", 25),
    ("marlin", "Pixel XL", 25),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn identities_are_unique_and_retain_launch_levels() {
        let mut ids = HashSet::new();
        let mut models = HashSet::new();
        for &(id, model, api) in DEVICES {
            assert!(ids.insert(id));
            assert!(models.insert(model));
            assert!(id.bytes().all(|c| c.is_ascii_lowercase()));
            assert!((25..=37).contains(&api));
        }
        assert_eq!(DEVICES.len(), 39);
        for expected in [
            (DEFAULT, "Pixel 6a", 32),
            ("sunfish", "Pixel 4a", 29),
            ("felix", "Pixel Fold", 33),
            ("tangorpro", "Pixel Tablet", 33),
            ("stallion", "Pixel 10a", 36),
            ("yogi", "Pixel 11 Pro Fold", 37),
        ] {
            assert!(DEVICES.contains(&expected));
        }
    }
}
