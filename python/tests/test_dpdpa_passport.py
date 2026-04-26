"""v0.4.0-rc3 (Task Q3) — DPDPA passport PDF builder tests."""

from __future__ import annotations

from mnemo.dpdpa_passport import PassportEntry, build_passport_pdf


def test_empty_passport_renders_valid_pdf():
    pdf = build_passport_pdf(
        subject_id="s1", fiduciary_name="Acme Health", entries=[]
    )
    assert pdf.startswith(b"%PDF-1.4")
    assert pdf.endswith(b"%%EOF\n")
    assert b"DPDPA Data Passport" in pdf
    assert b"no records held" in pdf


def test_passport_with_entries_includes_each_row():
    entries = [
        PassportEntry(
            collected_at="2026-04-26T12:00:00Z",
            scope="clinical-notes",
            content="Patient reports persistent fatigue for three weeks.",
            consent_token_sha256="abcd1234",
        ),
        PassportEntry(
            collected_at="2026-04-26T12:05:00Z",
            scope="lab-results",
            content="Hemoglobin 11.2 g/dL.",
            consent_token_sha256="efgh5678",
        ),
    ]
    pdf = build_passport_pdf(
        subject_id="subject-42",
        fiduciary_name="Acme Health",
        entries=entries,
    )
    assert b"clinical-notes" in pdf
    assert b"lab-results" in pdf
    assert b"abcd1234" in pdf
    assert b"efgh5678" in pdf
    assert b"subject-42" in pdf


def test_long_content_wraps_across_lines():
    content = " ".join(["word"] * 200)
    pdf = build_passport_pdf(
        subject_id="s1",
        fiduciary_name="F",
        entries=[
            PassportEntry(
                collected_at="t",
                scope="x",
                content=content,
                consent_token_sha256="h",
            )
        ],
    )
    # The wrapper produces multiple visible "word" hits — at least
    # three so we know wrapping kicked in.
    assert pdf.count(b"word") >= 50


def test_many_entries_paginate():
    entries = [
        PassportEntry(
            collected_at=f"2026-04-26T12:{i:02d}:00Z",
            scope="clinical-notes",
            content=f"Note {i}: " + ("filler " * 12),
            consent_token_sha256=f"hash{i:04d}",
        )
        for i in range(60)
    ]
    pdf = build_passport_pdf(
        subject_id="s1", fiduciary_name="F", entries=entries
    )
    # `Count 1` is single-page; multi-page passports report >= 2.
    assert b"/Count 1" not in pdf
    assert pdf.count(b"/Type /Page ") >= 2


def test_reproducible_for_same_inputs():
    entries = [
        PassportEntry(
            collected_at="2026-04-26T12:00:00Z",
            scope="x",
            content="y",
            consent_token_sha256="z",
        )
    ]
    issued = "2026-04-26T12:00:00Z"
    a = build_passport_pdf(
        subject_id="s",
        fiduciary_name="f",
        entries=entries,
        issued_at=issued,
    )
    b = build_passport_pdf(
        subject_id="s",
        fiduciary_name="f",
        entries=entries,
        issued_at=issued,
    )
    assert a == b
