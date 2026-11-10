use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(super) fn resolve(configured: &str) -> String {
    resolve_with(configured, |key| std::env::var(key).ok(), gh_token)
}

fn resolve_with(
    configured: &str,
    lookup: impl Fn(&str) -> Option<String>,
    gh: impl FnOnce() -> Option<String>,
) -> String {
    if !configured.trim().is_empty() {
        return configured.trim().to_owned();
    }
    for key in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Some(value) = lookup(key).filter(|value| !value.trim().is_empty()) {
            return value.trim().to_owned();
        }
    }
    gh().unwrap_or_default().trim().to_owned()
}

fn gh_token() -> Option<String> {
    let mut command = Command::new("gh");
    command.args(["auth", "token", "--hostname", "github.com"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW for the desktop app.
    }
    token_from_command(command, Duration::from_secs(2))
}

fn token_from_command(mut command: Command, timeout: Duration) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut output = String::new();
                child
                    .stdout
                    .take()?
                    .take(8193)
                    .read_to_string(&mut output)
                    .ok()?;
                let token = output.trim();
                if output.len() > 8192 || token.is_empty() {
                    return None;
                }
                reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")).ok()?;
                return Some(token.to_owned());
            }
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_token_precedes_environment_and_gh() {
        assert_eq!(
            resolve_with(
                " configured ",
                |_| Some("environment".into()),
                || panic!("gh must not run")
            ),
            "configured"
        );
    }

    #[test]
    fn environment_works_without_gh() {
        assert_eq!(
            resolve_with(
                "",
                |key| (key == "GITHUB_TOKEN").then(|| " environment \n".into()),
                || panic!("gh must not run")
            ),
            "environment"
        );
    }

    #[test]
    fn gh_token_precedes_github_token() {
        assert_eq!(
            resolve_with("", |key| Some(key.to_owned()), || panic!("gh must not run")),
            "GH_TOKEN"
        );
    }

    #[test]
    fn gh_is_optional_and_blank_environment_is_ignored() {
        assert_eq!(
            resolve_with(" ", |_| Some(" \n".into()), || Some(" gh-token\n".into())),
            "gh-token"
        );
        assert!(resolve_with("", |_| None, || None).is_empty());
    }
    #[test]
    fn missing_gh_falls_back_without_error() {
        let command = Command::new("snout-nonexistent-gh-auth-fixture");
        assert!(token_from_command(command, Duration::from_millis(100)).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn gh_output_is_trimmed_and_failed_output_is_discarded() {
        let mut success = Command::new("sh");
        success.args(["-c", "printf ' fixture-token\\n'"]);
        assert_eq!(
            token_from_command(success, Duration::from_secs(1)),
            Some("fixture-token".into())
        );
        let mut failure = Command::new("sh");
        failure.args(["-c", "printf 'fixture-token'; exit 1"]);
        assert!(token_from_command(failure, Duration::from_secs(1)).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn gh_lookup_has_a_bounded_timeout() {
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 10"]);
        let start = Instant::now();
        assert!(token_from_command(command, Duration::from_millis(40)).is_none());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
