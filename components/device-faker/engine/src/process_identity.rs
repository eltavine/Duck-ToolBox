//! Linux process identity includes start time so PID reuse cannot retain a global session.
use std::{io::Read, path::Path};
pub fn start_at(root: &Path, pid: u32) -> Option<u64> {
    if pid < 2 {
        return None;
    }
    let file = std::fs::File::open(root.join(format!("proc/{pid}/stat"))).ok()?;
    let mut text = String::new();
    file.take(4097).read_to_string(&mut text).ok()?;
    if text.len() > 4096 {
        return None;
    }
    parse(&text)
}
fn parse(text: &str) -> Option<u64> {
    let mut fields = text.rsplit_once(')')?.1.split_whitespace();
    if matches!(fields.next()?, "Z" | "X" | "x") {
        return None;
    }
    fields.nth(18)?.parse().ok()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_names_can_contain_parentheses() {
        let mut fields = vec!["S"; 20];
        fields[19] = "54321";
        assert_eq!(
            parse(&format!("123 (an (odd) name) {}", fields.join(" "))),
            Some(54321)
        );
        assert!(parse("123 (incomplete)").is_none());
        fields[0] = "Z";
        assert!(parse(&format!("123 (zombie) {}", fields.join(" "))).is_none());
    }
}
