//! What gsudo says of itself, for `sudo -n`, `sudo -l` and `cash doctor`.
//!
//! `gsudo status --json` reports, among other things, whether its credentials cache is
//! open for the process that asks (`CacheAvailable`) and how many cache sessions are open
//! (`CacheSessionsCount`). Asking never elevates and never prompts.

use std::path::Path;

/// The value of `key` in what `gsudo status --json` prints, as its text (`true`, `3`),
/// or `None` when gsudo cannot be run or does not report it.
///
/// The cache gsudo reports on is the one for its caller: started by cash, the shell
/// that asks.
#[must_use]
pub fn status(gsudo: &Path, key: &str) -> Option<String> {
    let output = std::process::Command::new(gsudo)
        .args(["status", "--json"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    json_value(&String::from_utf8_lossy(&output.stdout), key)
}

/// Whether gsudo's credentials cache is open for this process: a command it elevates
/// now runs without a UAC prompt.
#[must_use]
pub fn cache_open(gsudo: &Path) -> Option<bool> {
    match status(gsudo, "CacheAvailable")?.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Closes every gsudo credentials cache session (`gsudo -k`), quietly, as `sudo -k` does;
/// whether gsudo did.
pub fn reset(gsudo: &Path) -> bool {
    std::process::Command::new(gsudo)
        .arg("-k")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The scalar value of the top-level `key` in gsudo's flat JSON: what follows
/// `"key":` up to the next `,`, `}` or line end, its quotes removed.
fn json_value(json: &str, key: &str) -> Option<String> {
    let quoted = format!("\"{key}\"");
    let at = json.find(&quoted)? + quoted.len();
    let rest = json.get(at..)?.trim_start().strip_prefix(':')?.trim_start();
    let end = rest.find([',', '}', '\n', '\r']).unwrap_or(rest.len());
    Some(rest.get(..end)?.trim().trim_matches('"').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_is_read_from_gsudo_status() {
        let json = "{\n \"CallerPid\":36216,\n \"UserName\":\"PC\\\\tom\",\n \
                    \"CacheAvailable\":false,\n \"CacheSessionsCount\":2,\n}";
        assert_eq!(json_value(json, "CacheAvailable").as_deref(), Some("false"));
        assert_eq!(json_value(json, "CacheSessionsCount").as_deref(), Some("2"));
        assert_eq!(json_value(json, "CallerPid").as_deref(), Some("36216"));
        assert_eq!(json_value(json, "Missing"), None);
    }
}
