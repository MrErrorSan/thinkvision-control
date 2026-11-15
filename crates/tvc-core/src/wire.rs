//! The single-instance channel's wire format. A later launch sends its arguments as one line of JSON (an array of
//! strings); the running copy answers `{"exit": u8, "stdout": String}`.

use serde::{Deserialize, Serialize};

/// What a forwarded launch gets back: its exit code and what it would have printed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reply {
    pub exit: u8,
    pub stdout: String,
}

pub fn encode_request<S: AsRef<str>>(args: &[S]) -> String {
    let args: Vec<&str> = args.iter().map(AsRef::as_ref).collect();
    serde_json::to_string(&args).unwrap_or_else(|_| "[]".into())
}

/// None when `line` isn't a JSON array of strings.
pub fn decode_request(line: &str) -> Option<Vec<String>> {
    serde_json::from_str(line.trim()).ok()
}

pub fn encode_reply(reply: &Reply) -> String {
    serde_json::to_string(reply).unwrap_or_else(|_| r#"{"exit":0,"stdout":""}"#.into())
}

pub fn decode_reply(line: &str) -> Option<Reply> {
    serde_json::from_str(line.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_replies_round_trip() {
        let args = ["--get", "all", "with \"quotes\"\nand a newline"];
        assert_eq!(decode_request(&encode_request(&args)).unwrap(), args);
        assert!(!encode_request(&args).contains('\n'));
        let reply = Reply { exit: 5, stdout: "{\"brightness\": 40}\n".into() };
        assert_eq!(decode_reply(&encode_reply(&reply)), Some(reply));
        assert_eq!(decode_request("--get\nall"), None);
    }
}
