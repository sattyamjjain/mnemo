//! Byte-stability gate for `reproduction_bench`.
//!
//! The whole premise of the claimed-vs-observed bench is **reproducibility by
//! disclosure**: mnemo publishes a LoCoMo number anyone can re-run and get the
//! *same* bytes. That only holds if the offline path is deterministic — which it
//! is, via an exact brute-force vector index + a neutralised recency lane (see
//! the bin's docs). This test runs the built binary **twice** into a temp dir
//! with a pinned `--date` and asserts the two reports are byte-identical. If a
//! future change reintroduces run-to-run nondeterminism (e.g. reverts to the
//! approximate HNSW index or a wall-clock lane), this fails.

use std::process::Command;

#[test]
fn reproduction_report_is_byte_stable_across_two_runs() {
    let bin = env!("CARGO_BIN_EXE_reproduction_bench");
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().to_str().unwrap().to_string();
    let date = "2026-01-01"; // pinned so the filename + body are stable in CI

    let run = || -> (Vec<u8>, Vec<u8>) {
        let status = Command::new(bin)
            .args(["--date", date, "--out-dir", &out])
            .status()
            .expect("run reproduction_bench");
        assert!(status.success(), "reproduction_bench exited non-zero");
        let md = std::fs::read(format!("{out}/reproduction_{date}.md")).expect("md written");
        let json = std::fs::read(format!("{out}/reproduction_{date}.json")).expect("json written");
        (md, json)
    };

    let (md1, json1) = run();
    let (md2, json2) = run();

    assert_eq!(
        md1, md2,
        "reproduction_{date}.md must be byte-identical across two runs \
         (the reproducibility-by-disclosure premise); nondeterminism crept back in"
    );
    assert_eq!(
        json1, json2,
        "reproduction_{date}.json must be byte-identical across two runs"
    );

    // Sanity: the report is the real thing, not an empty stub.
    let md = String::from_utf8(md1).expect("utf8 report");
    assert!(
        md.contains("recall@1"),
        "report must carry the observed metric"
    );
    assert!(
        md.contains("Wilson 95%"),
        "report must carry the Wilson interval"
    );
    assert!(
        md.contains("NOT re-run in this harness"),
        "report must carry the claimed-vs-observed honesty hedge"
    );
}
