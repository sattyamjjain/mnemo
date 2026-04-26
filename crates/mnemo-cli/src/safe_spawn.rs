//! v0.4.0-rc3 (Task B2) — safe-spawn checks that run BEFORE the
//! `MnemoEngine` is constructed, defending against the failure modes
//! the OX-MCP disclosure (2026-04-24) called out:
//!
//! 1. Inherited-secret refusal — if the parent process leaks
//!    `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` (or anything in
//!    [`SENSITIVE_ENV_KEYS`]) into the child env, refuse to start.
//! 2. Manifest-only config — refuse if argv contains `--config`-style
//!    JSON injection vectors. Only `--manifest <path>` is allowed.
//! 3. Untrusted-parent refusal — when stdin is not a TTY, the parent
//!    process basename must be in `manifest.allowed_parents`.
//!
//! Every check is a pure function of `(env, argv, parent, manifest)`
//! so tests can exercise each refusal path deterministically.

use std::collections::BTreeSet;

use thiserror::Error;

pub const SENSITIVE_ENV_KEYS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY",
    "HF_TOKEN",
    "AWS_SECRET_ACCESS_KEY",
    "GITHUB_TOKEN",
    "MNEMO_ENCRYPTION_KEY",
];

#[derive(Debug, Error, PartialEq)]
pub enum SafeSpawnError {
    #[error(
        "refused to start: inherited sensitive env var {key:?} from parent process. \
         Run through a secret-clearing wrapper (`env -i`, systemd `Environment=`). \
         Override at your own risk: MNEMO_REJECT_INHERITED_SECRETS=0."
    )]
    InheritedSecret { key: String },
    #[error(
        "refused to start: command-line carries config-style argument {arg:?}. \
         Use --manifest <path> only; all server config lives in the TOML manifest."
    )]
    ArgsBasedConfig { arg: String },
    #[error(
        "refused to start: parent process {parent:?} is not in manifest.allowed_parents. \
         Add it to the manifest, or run through a TTY (which lifts the parent check)."
    )]
    UnknownParent { parent: String },
}

pub fn check_inherited_secrets(
    env: impl IntoIterator<Item = (String, String)>,
    reject: bool,
) -> Result<(), SafeSpawnError> {
    if !reject {
        return Ok(());
    }
    let envmap: std::collections::HashMap<String, String> = env.into_iter().collect();
    for key in SENSITIVE_ENV_KEYS {
        if envmap.contains_key(*key) {
            return Err(SafeSpawnError::InheritedSecret {
                key: (*key).to_string(),
            });
        }
    }
    Ok(())
}

pub fn check_args_pattern(argv: &[String]) -> Result<(), SafeSpawnError> {
    let banned = [
        "--config",
        "--config-json",
        "--inline-config",
        "-c",
        "--secret",
    ];
    for a in argv {
        if banned
            .iter()
            .any(|b| a == *b || a.starts_with(&format!("{b}=")))
        {
            return Err(SafeSpawnError::ArgsBasedConfig { arg: a.clone() });
        }
    }
    Ok(())
}

pub fn check_parent_process(
    parent_basename: Option<&str>,
    has_tty: bool,
    allowed_parents: &BTreeSet<String>,
) -> Result<(), SafeSpawnError> {
    if has_tty {
        return Ok(());
    }
    let Some(parent) = parent_basename else {
        return Ok(());
    };
    if allowed_parents.is_empty() || allowed_parents.contains(parent) {
        return Ok(());
    }
    Err(SafeSpawnError::UnknownParent {
        parent: parent.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> BTreeSet<String> {
        ["claude", "systemd"]
            .into_iter()
            .map(String::from)
            .collect()
    }

    #[test]
    fn inherited_anthropic_key_rejected() {
        let env = vec![("ANTHROPIC_API_KEY".into(), "x".into())];
        let err = check_inherited_secrets(env, true).unwrap_err();
        assert_eq!(
            err,
            SafeSpawnError::InheritedSecret {
                key: "ANTHROPIC_API_KEY".into()
            }
        );
    }

    #[test]
    fn inherited_secrets_check_can_be_disabled() {
        let env = vec![("ANTHROPIC_API_KEY".into(), "x".into())];
        check_inherited_secrets(env, false).unwrap();
    }

    #[test]
    fn config_arg_rejected() {
        let argv = vec![
            "mnemo-mcp-server".into(),
            "--manifest".into(),
            "m.toml".into(),
            "--config-json".into(),
            "{...}".into(),
        ];
        let err = check_args_pattern(&argv).unwrap_err();
        assert!(matches!(err, SafeSpawnError::ArgsBasedConfig { .. }));
    }

    #[test]
    fn manifest_arg_alone_is_fine() {
        let argv = vec![
            "mnemo-mcp-server".into(),
            "--manifest".into(),
            "m.toml".into(),
        ];
        check_args_pattern(&argv).unwrap();
    }

    #[test]
    fn equals_form_of_banned_arg_rejected() {
        let argv = vec!["mnemo-mcp-server".into(), "--config=foo.json".into()];
        let err = check_args_pattern(&argv).unwrap_err();
        assert!(matches!(err, SafeSpawnError::ArgsBasedConfig { .. }));
    }

    #[test]
    fn allowed_parent_passes() {
        check_parent_process(Some("claude"), false, &allowed()).unwrap();
    }

    #[test]
    fn unknown_parent_rejected_when_no_tty() {
        let err = check_parent_process(Some("evil"), false, &allowed()).unwrap_err();
        assert!(matches!(err, SafeSpawnError::UnknownParent { .. }));
    }

    #[test]
    fn unknown_parent_allowed_when_tty_present() {
        check_parent_process(Some("evil"), true, &allowed()).unwrap();
    }

    #[test]
    fn empty_allowed_parents_means_allow_any() {
        check_parent_process(Some("anything"), false, &BTreeSet::new()).unwrap();
    }
}
