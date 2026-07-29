//! Drift guard: every top-level `mnemo` CLI command must be documented in the
//! README's `## CLI Options` block. Mirrors the `docs_document_exactly_the_
//! registered_tools` MCP-tool drift test (regenerate the surface from the live
//! source of truth, assert the docs cover it) — here the source of truth is the
//! **built binary's own `--help`**, so the check tracks the real clap command
//! tree, not a hand-maintained list. This is what would have caught the README
//! listing only `baseline` / `mcp-server` / `eval` while the binary also shipped
//! `bench` and `compliance`.

use std::path::Path;
use std::process::Command;

/// Top-level command names parsed from `mnemo --help`'s `Commands:` section.
/// A command line is `^  <name>  <desc>` (exactly two leading spaces, then the
/// name); clap wraps descriptions onto more-indented continuation lines, which
/// therefore do not match. The auto-generated `help` command is excluded.
fn cli_commands() -> Vec<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_mnemo"))
        .arg("--help")
        .output()
        .expect("run `mnemo --help`");
    let help = String::from_utf8_lossy(&out.stdout);

    let mut names = Vec::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line.trim_end() == "Commands:" {
            in_commands = true;
            continue;
        }
        if in_commands {
            // A new top-level section (e.g. "Options:") ends the command list.
            if !line.starts_with(' ') && !line.trim().is_empty() {
                break;
            }
            // Command line: exactly two leading spaces, then the name token.
            let bytes = line.as_bytes();
            if bytes.len() > 2
                && &line[..2] == "  "
                && bytes[2] != b' '
                && let Some(name) = line.split_whitespace().next()
                && name != "help"
            {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// The text of the README `## CLI Options` section (up to the next `## `).
fn readme_cli_options_block() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("README.md");
    let readme = std::fs::read_to_string(path).expect("README.md readable");
    let start = readme
        .find("## CLI Options")
        .expect("README must have a `## CLI Options` section");
    let rest = &readme[start + "## CLI Options".len()..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn readme_documents_every_cli_command() {
    let commands = cli_commands();
    assert!(
        commands.len() >= 5,
        "expected to parse >=5 top-level commands from `mnemo --help`, got {commands:?}"
    );
    let block = readme_cli_options_block();

    // A command is documented iff it is the first token of a top-level command
    // line: exactly two leading spaces, then the name (subcommands are indented
    // deeper, prose mentions are not command lines). A loose `contains(cmd)`
    // would false-pass on substrings like "benchmark" or "retention-conformance".
    let documented = |cmd: &str| -> bool {
        block.lines().any(|l| {
            l.strip_prefix("  ").is_some_and(|rest| {
                !rest.starts_with(' ') && rest.split_whitespace().next() == Some(cmd)
            })
        })
    };

    let missing: Vec<&String> = commands.iter().filter(|cmd| !documented(cmd)).collect();

    assert!(
        missing.is_empty(),
        "top-level `mnemo` command(s) not documented in the README `## CLI Options` \
         block: {missing:?}. All commands from `mnemo --help`: {commands:?}"
    );
}
