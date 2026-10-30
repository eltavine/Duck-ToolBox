//! Conservative window-focus interpretation: ambiguous displays do not authorize globals.
use std::collections::HashSet;
pub const FILTER: &str = "^[[:space:]]*(Display: mDisplayId=|m(CurrentFocus|FocusedApp)=)";
pub fn parse_dump(text: &str) -> String {
    let lines: Vec<_> = text.lines().map(str::trim).collect();
    let has_displays = lines
        .iter()
        .any(|line| line.starts_with("Display: mDisplayId="));
    let mut display = None;
    let mut current = Vec::new();
    let mut focused = Vec::new();
    for line in lines {
        if let Some(id) = line.strip_prefix("Display: mDisplayId=") {
            display = id
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u32>().ok());
            continue;
        }
        // Density changes target display 0; another display cannot authorize them.
        if has_displays && display != Some(0) {
            continue;
        }
        if let Some(value) = line.strip_prefix("mCurrentFocus=") {
            current.push(value);
        }
        if let Some(value) = line.strip_prefix("mFocusedApp=") {
            focused.push(value);
        }
    }
    let values = if current.is_empty() { focused } else { current };
    let mut targets = HashSet::new();
    for value in values {
        let Some(target) = parse_ref(value) else {
            return "-".into();
        };
        targets.insert(target);
    }
    if targets.len() == 1 {
        targets.into_iter().next().unwrap()
    } else {
        "-".into()
    }
}
fn parse_ref(value: &str) -> Option<String> {
    let user = value
        .split_whitespace()
        .find_map(|token| token.strip_prefix('u')?.parse::<u32>().ok())?;
    let package = value.split_whitespace().find_map(|token| {
        let (package, _) = token.split_once('/')?;
        (package.contains('.')
            && package.split('.').all(|part| {
                !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            }))
        .then_some(package)
    })?;
    Some(format!("{package}@{user}"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_focus_includes_the_android_user() {
        assert_eq!(
            parse_dump(" mCurrentFocus=Window{a u10 com.example.app/.Main}"),
            "com.example.app@10"
        );
        assert_eq!(
            parse_dump("mFocusedApp=ActivityRecord{a u0 com.example.app/.Main t1}"),
            "com.example.app@0"
        );
    }
    #[test]
    fn null_or_ambiguous_focus_does_not_fall_back_to_an_old_activity() {
        assert_eq!(
            parse_dump(
                "mCurrentFocus=null\nmFocusedApp=ActivityRecord{a u0 com.example.app/.Main}"
            ),
            "-"
        );
        assert_eq!(
            parse_dump(
                "mCurrentFocus=Window{a u0 com.example.a/.Main}\nmCurrentFocus=Window{b u10 com.example.b/.Main}"
            ),
            "-"
        );
        assert_eq!(
            parse_dump("mCurrentFocus=Window{a com.example.app/.Main}"),
            "-"
        );
    }
    #[test]
    fn only_the_default_display_can_authorize_global_mutation() {
        assert_eq!(
            parse_dump(
                "Display: mDisplayId=0\nmCurrentFocus=Window{a u0 com.example.a/.Main}\nDisplay: mDisplayId=2\nmCurrentFocus=Window{b u10 com.example.b/.Main}"
            ),
            "com.example.a@0"
        );
        assert_eq!(
            parse_dump("Display: mDisplayId=2\nmCurrentFocus=Window{b u10 com.example.b/.Main}"),
            "-"
        );
    }
    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn large_window_dumps_are_filtered_before_the_output_cap() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("windows");
        std::fs::write(
            &file,
            format!(
                "{}\nDisplay: mDisplayId=0\n  mCurrentFocus=Window{{a u0 com.example.app/.Main}}\n",
                "unrelated window details\n".repeat(10000)
            ),
        )
        .unwrap();
        let mut raw = std::process::Command::new("cat");
        raw.arg(&file);
        assert!(crate::bounded::output(raw).is_err());
        let mut filter = std::process::Command::new("grep");
        filter.args(["-E", FILTER]).arg(&file);
        let result = crate::bounded::output(filter).unwrap();
        assert_eq!(
            parse_dump(&String::from_utf8(result).unwrap()),
            "com.example.app@0"
        );
    }
}
