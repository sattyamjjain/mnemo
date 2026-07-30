//! Structural guards for the committed implicit-association corpus
//! (`bench/locomo/data/implicit_association.jsonl`). These invariants are what
//! make the bench meaningful: if `stored_fact` and `indirect_query` shared a
//! significant token, the "indirect" arm would trivially retrieve by wording and
//! there would be no implicit-association blind spot to measure.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Row {
    id: String,
    domain: String,
    stored_fact: String,
    indirect_query: String,
    direct_query: String,
    bridge: String,
    target_substring: String,
    source_url: String,
    distractors: Vec<String>,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join("implicit_association.jsonl")
}

fn load() -> Vec<Row> {
    let text = std::fs::read_to_string(corpus_path()).expect("corpus readable");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Row>(l).expect("valid corpus row"))
        .collect()
}

const STOP: &[&str] = &[
    "a", "an", "the", "of", "to", "in", "on", "at", "for", "and", "or", "but", "is", "are", "was",
    "were", "be", "been", "being", "by", "with", "from", "as", "into", "over", "under", "this",
    "that", "these", "those", "it", "its", "his", "her", "their", "your", "our", "my", "me", "you",
    "he", "she", "they", "we", "do", "does", "did", "has", "have", "had", "will", "would", "can",
    "could", "should", "may", "might", "who", "whom", "what", "which", "when", "where", "why",
    "how", "not", "no", "yes", "if", "then", "than", "so", "such", "very", "more", "most", "each",
    "any", "all", "some", "one", "two", "three", "near", "around", "across", "between", "about",
];

/// Significant tokens: lowercased `[a-z0-9]+` of length >= 3 that are not stopwords.
fn significant(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            cur.push(c.to_ascii_lowercase());
        } else {
            if cur.len() >= 3 && !STOP.contains(&cur.as_str()) {
                out.insert(cur.clone());
            }
            cur.clear();
        }
    }
    if cur.len() >= 3 && !STOP.contains(&cur.as_str()) {
        out.insert(cur);
    }
    out
}

#[test]
fn corpus_has_exactly_thirty_rows_and_eight_domains() {
    let rows = load();
    assert_eq!(rows.len(), 30, "corpus must have exactly 30 rows");
    let domains: BTreeSet<&str> = rows.iter().map(|r| r.domain.as_str()).collect();
    assert!(
        domains.len() >= 8,
        "corpus must span >= 8 everyday domains, got {}: {domains:?}",
        domains.len()
    );
}

#[test]
fn ids_are_unique() {
    let rows = load();
    let ids: BTreeSet<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids.len(), rows.len(), "row ids must be unique");
}

#[test]
fn stored_fact_and_indirect_query_share_no_significant_token() {
    let rows = load();
    for r in &rows {
        let overlap: Vec<String> = significant(&r.stored_fact)
            .intersection(&significant(&r.indirect_query))
            .cloned()
            .collect();
        assert!(
            overlap.is_empty(),
            "row {}: stored_fact and indirect_query must share no significant token, \
             but share {overlap:?} — the indirect arm would retrieve by wording",
            r.id
        );
    }
}

#[test]
fn target_substring_occurs_in_stored_fact() {
    let rows = load();
    for r in &rows {
        assert!(
            r.stored_fact.contains(&r.target_substring),
            "row {}: target_substring {:?} must occur in stored_fact {:?}",
            r.id,
            r.target_substring,
            r.stored_fact
        );
        // The distiller only captures capitalized entities / UPPER_SNAKE
        // constants; a lowercase target would never surface in the map arm.
        assert!(
            r.target_substring
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase()),
            "row {}: target_substring {:?} must start capitalized",
            r.id,
            r.target_substring
        );
    }
}

#[test]
fn every_source_url_is_a_valid_absolute_https_url() {
    let rows = load();
    for r in &rows {
        let u = &r.source_url;
        assert!(
            u.starts_with("https://")
                && u.len() > "https://".len()
                && !u.contains(char::is_whitespace),
            "row {}: source_url {u:?} must be a syntactically valid absolute https URL",
            r.id
        );
    }
}

#[test]
fn every_row_has_six_distractors_and_nonempty_control() {
    let rows = load();
    for r in &rows {
        assert_eq!(r.distractors.len(), 6, "row {}: needs 6 distractors", r.id);
        assert!(
            !r.direct_query.trim().is_empty(),
            "row {}: empty direct_query",
            r.id
        );
        assert!(!r.bridge.trim().is_empty(), "row {}: empty bridge", r.id);
    }
}
