use serde_json::{Value, json};
#[derive(Debug)]
pub struct Error {
    pub code: &'static str,
    pub params: Value,
    message: &'static str,
}
impl Error {
    pub fn new(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            params: json!({}),
            message,
        }
    }
    pub fn params(code: &'static str, message: &'static str, params: Value) -> Self {
        Self {
            code,
            params,
            message,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for Error {}
