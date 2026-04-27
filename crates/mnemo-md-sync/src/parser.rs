//! Markdown frontmatter + body parser (v0.4.0 P2-6).
//!
//! Frontmatter shape (YAML-style, but parsed without a full YAML
//! dependency — we only support the four keys we care about):
//!
//! ```markdown
//! ---
//! mnemo_id: 0190abcd-...
//! tags: [project-x, retrospective]
//! expires_at: 2026-12-31T00:00:00Z
//! agent_id: prod-runner
//! ---
//!
//! # Heading
//!
//! Body...
//! ```
//!
//! Anything that doesn't match the expected key/value shape is
//! ignored; an unrecognized key on a future Wuphf-flavoured frontmatter
//! does not fail the parse.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedMarkdown {
    pub mnemo_id: Option<Uuid>,
    pub agent_id: Option<String>,
    pub tags: Vec<String>,
    pub expires_at: Option<String>,
    pub body: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    #[error("frontmatter is not closed with `---`")]
    UnterminatedFrontmatter,
    #[error("invalid `mnemo_id`: {0}")]
    InvalidId(String),
}

pub fn parse_markdown(input: &str) -> Result<ParsedMarkdown, ParseError> {
    let mut mnemo_id = None;
    let mut agent_id = None;
    let mut tags = Vec::new();
    let mut expires_at = None;

    let trimmed = input.trim_start_matches('\u{FEFF}'); // strip BOM
    let body = if let Some(rest) = trimmed.strip_prefix("---\n") {
        let close = rest.find("\n---\n").or_else(|| rest.find("\n---"));
        let Some(close_idx) = close else {
            return Err(ParseError::UnterminatedFrontmatter);
        };
        let header = &rest[..close_idx];
        for line in header.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some((k, v)) = line.split_once(':') else {
                continue;
            };
            let k = k.trim();
            let v = v.trim();
            match k {
                "mnemo_id" => {
                    if !v.is_empty() {
                        mnemo_id = Some(
                            Uuid::parse_str(v).map_err(|e| ParseError::InvalidId(e.to_string()))?,
                        );
                    }
                }
                "agent_id" => {
                    if !v.is_empty() {
                        agent_id = Some(v.to_string());
                    }
                }
                "tags" => {
                    tags = parse_tag_list(v);
                }
                "expires_at" => {
                    if !v.is_empty() {
                        expires_at = Some(v.to_string());
                    }
                }
                _ => {}
            }
        }
        // Body starts after the closing `---\n`. Try the
        // newline-prefixed close first, then the bare close at end of
        // file.
        let body_start = if rest[close_idx..].starts_with("\n---\n") {
            close_idx + "\n---\n".len()
        } else {
            close_idx + "\n---".len()
        };
        rest.get(body_start..).unwrap_or("").to_string()
    } else {
        input.to_string()
    };

    Ok(ParsedMarkdown {
        mnemo_id,
        agent_id,
        tags,
        expires_at,
        body: body.trim_start_matches('\n').to_string(),
    })
}

fn parse_tag_list(raw: &str) -> Vec<String> {
    let s = raw.trim();
    let s = s.strip_prefix('[').unwrap_or(s);
    let s = s.strip_suffix(']').unwrap_or(s);
    s.split(',')
        .map(|t| t.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_frontmatter_returns_full_body() {
        let p = parse_markdown("# Heading\n\nbody text").unwrap();
        assert_eq!(p.mnemo_id, None);
        assert_eq!(p.tags, Vec::<String>::new());
        assert_eq!(p.body, "# Heading\n\nbody text");
    }

    #[test]
    fn frontmatter_with_all_fields_parses() {
        let id = Uuid::now_v7();
        let input = format!(
            "---\nmnemo_id: {id}\nagent_id: prod-runner\ntags: [a, b, c]\nexpires_at: 2026-12-31T00:00:00Z\n---\n# H\n\nbody\n"
        );
        let p = parse_markdown(&input).unwrap();
        assert_eq!(p.mnemo_id, Some(id));
        assert_eq!(p.agent_id.as_deref(), Some("prod-runner"));
        assert_eq!(p.tags, vec!["a", "b", "c"]);
        assert_eq!(p.expires_at.as_deref(), Some("2026-12-31T00:00:00Z"));
        assert_eq!(p.body, "# H\n\nbody\n");
    }

    #[test]
    fn unterminated_frontmatter_errors() {
        let err = parse_markdown("---\nmnemo_id: x\nbody but no close").unwrap_err();
        assert_eq!(err, ParseError::UnterminatedFrontmatter);
    }

    #[test]
    fn invalid_mnemo_id_errors() {
        let err = parse_markdown("---\nmnemo_id: not-a-uuid\n---\nbody").unwrap_err();
        assert!(matches!(err, ParseError::InvalidId(_)));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let input = "---\nfutureWuphfKey: value\ntags: [x]\n---\nbody";
        let p = parse_markdown(input).unwrap();
        assert_eq!(p.tags, vec!["x"]);
        assert_eq!(p.body, "body");
    }

    #[test]
    fn quoted_tags_strip_quotes() {
        let p = parse_markdown(
            r#"---
tags: ["a", 'b', c]
---
body"#,
        )
        .unwrap();
        assert_eq!(p.tags, vec!["a", "b", "c"]);
    }
}
