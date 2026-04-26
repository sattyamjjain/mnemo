"""v0.4.0-rc3 (Task Q3) — DPDPA "data passport" PDF builder.

Under DPDPA Section 11 (right to portability) and Section 12 (right
to access), an Indian data fiduciary must hand a data principal a
human-readable summary of every personal data point the fiduciary
holds about them. Mnemo's compliance crate already produces a signed
JSONL audit log; this module renders the same data as a single-page
PDF "passport" suitable for emailing to a subject or attaching to a
DPB inquiry response.

The PDF is hand-rolled (no third-party PDF dep) so the artifact is
reproducible byte-for-byte from the same input — which matters when
the same passport is later re-issued for a follow-up access request
and the subject expects identical content.

Usage::

    from mnemo.dpdpa_passport import PassportEntry, build_passport_pdf

    entries = [
        PassportEntry(
            collected_at="2026-04-26T12:00:00Z",
            scope="clinical-notes",
            content="Patient reports persistent fatigue ...",
            consent_token_sha256="abcd...",
        ),
    ]
    pdf_bytes = build_passport_pdf(
        subject_id="subject-42",
        fiduciary_name="Acme Health",
        entries=entries,
    )
    Path("passport.pdf").write_bytes(pdf_bytes)
"""

from __future__ import annotations

import datetime as _dt
import io
import textwrap
from dataclasses import dataclass, field
from typing import Sequence


@dataclass(frozen=True)
class PassportEntry:
    """One row in the passport.

    Mirrors the fields the Mannsetu adapter (B4) lands in the audit
    trail per write: timestamp, scope, the content the operator
    persisted, and the consent-token hash that authorized the write.
    """

    collected_at: str
    scope: str
    content: str
    consent_token_sha256: str


@dataclass
class PassportMetadata:
    subject_id: str
    fiduciary_name: str
    issued_at: str = field(
        default_factory=lambda: _dt.datetime.now(_dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def build_passport_pdf(
    *,
    subject_id: str,
    fiduciary_name: str,
    entries: Sequence[PassportEntry],
    issued_at: str | None = None,
) -> bytes:
    """Render a passport PDF as raw bytes.

    Layout: A4, 12pt Helvetica. One header block with subject /
    fiduciary / issued-at, then a numbered list of entries. Pages
    auto-flow when entries exceed the first page.
    """
    meta = PassportMetadata(
        subject_id=subject_id,
        fiduciary_name=fiduciary_name,
        issued_at=issued_at
        or _dt.datetime.now(_dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z"),
    )
    pages = _layout_pages(meta, entries)
    return _serialize_pdf(pages)


# ---------------------------------------------------------------------------
# Layout: turn the (metadata, entries) into a list of pages, each page is a
# list of (x, y, text) tuples ready for the content stream.
# ---------------------------------------------------------------------------

# A4 in PDF points (72 dpi). PDF origin is bottom-left.
_PAGE_WIDTH = 595.0
_PAGE_HEIGHT = 842.0
_LEFT = 50.0
_RIGHT = 545.0
_TOP = 800.0
_BOTTOM = 60.0
_LEAD = 14.0  # line height
_FONT_SIZE = 11
_HEADER_FONT_SIZE = 16


def _layout_pages(
    meta: PassportMetadata, entries: Sequence[PassportEntry]
) -> list[list[tuple[float, float, int, str]]]:
    """Return [(x, y, font_size, escaped_text), ...] per page."""
    pages: list[list[tuple[float, float, int, str]]] = []
    current: list[tuple[float, float, int, str]] = []
    y = _TOP

    def flush_page() -> None:
        nonlocal current, y
        if current:
            pages.append(current)
        current = []
        y = _TOP

    def push(text: str, *, font_size: int = _FONT_SIZE) -> None:
        nonlocal y
        if y < _BOTTOM:
            flush_page()
        current.append((_LEFT, y, font_size, _pdf_escape(text)))
        y -= _LEAD

    push("DPDPA Data Passport", font_size=_HEADER_FONT_SIZE)
    y -= 6
    push(f"Subject ID:       {meta.subject_id}")
    push(f"Data fiduciary:   {meta.fiduciary_name}")
    push(f"Issued at (UTC):  {meta.issued_at}")
    push(f"Records:          {len(entries)}")
    y -= 8
    push("-" * 80)
    y -= 4

    if not entries:
        push("(no records held for this subject)")
    else:
        # Wrap content to ~85 chars so it fits 11pt Helvetica between
        # _LEFT and _RIGHT.
        for i, e in enumerate(entries, start=1):
            push(f"#{i}  collected_at: {e.collected_at}    scope: {e.scope}")
            push(f"     consent_token_sha256: {e.consent_token_sha256}")
            for line in textwrap.wrap(
                e.content, width=85, break_long_words=False, break_on_hyphens=False
            ):
                push(f"     {line}")
            y -= 4

    flush_page()
    if not pages:
        pages = [[]]
    return pages


# ---------------------------------------------------------------------------
# Serialization: walk a tiny PDF object graph and emit valid bytes.
# ---------------------------------------------------------------------------


def _pdf_escape(text: str) -> str:
    """Escape text for a PDF string literal."""
    out = []
    for ch in text:
        if ch in ("(", ")", "\\"):
            out.append("\\" + ch)
        elif ord(ch) < 0x20 or ord(ch) > 0x7E:
            # Replace non-ASCII with '?' — Helvetica is WinAnsi only.
            # Operators can swap fonts via a follow-up tweak; for now
            # we keep the artifact zero-dep.
            out.append("?")
        else:
            out.append(ch)
    return "".join(out)


def _serialize_pdf(pages: list[list[tuple[float, float, int, str]]]) -> bytes:
    """Emit a minimal valid PDF 1.4 with the given pages."""
    objects: list[bytes] = []  # 1-indexed; objects[0] is unused

    def add(payload: bytes) -> int:
        objects.append(payload)
        return len(objects)

    # Reserve catalog + pages root first; we'll back-fill kids when we
    # know the page object IDs.
    catalog_id = add(b"")
    pages_id = add(b"")
    font_id = add(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\n")

    page_ids: list[int] = []
    for content in pages:
        # Build the content stream.
        stream_lines: list[str] = []
        for x, y, fs, text in content:
            stream_lines.append("BT")
            stream_lines.append(f"/F1 {fs} Tf")
            stream_lines.append(f"{x:.2f} {y:.2f} Td")
            stream_lines.append(f"({text}) Tj")
            stream_lines.append("ET")
        stream_body = ("\n".join(stream_lines) + "\n").encode("latin-1", "replace")
        stream_id = add(
            (
                f"<< /Length {len(stream_body)} >>\n"
                f"stream\n"
            ).encode("latin-1")
            + stream_body
            + b"endstream\n"
        )
        page_obj = (
            f"<< /Type /Page /Parent {pages_id} 0 R "
            f"/MediaBox [0 0 {_PAGE_WIDTH:.0f} {_PAGE_HEIGHT:.0f}] "
            f"/Resources << /Font << /F1 {font_id} 0 R >> >> "
            f"/Contents {stream_id} 0 R >>\n"
        ).encode("latin-1")
        page_ids.append(add(page_obj))

    kids = " ".join(f"{pid} 0 R" for pid in page_ids)
    objects[pages_id - 1] = (
        f"<< /Type /Pages /Kids [{kids}] /Count {len(page_ids)} >>\n"
    ).encode("latin-1")
    objects[catalog_id - 1] = (
        f"<< /Type /Catalog /Pages {pages_id} 0 R >>\n".encode("latin-1")
    )

    # Now lay out the bytes and remember each object's offset for the
    # xref table.
    out = io.BytesIO()
    out.write(b"%PDF-1.4\n%\xc2\xa5\xc2\xb1\xc3\xab\n")  # binary marker
    offsets: list[int] = [0]
    for i, body in enumerate(objects, start=1):
        offsets.append(out.tell())
        out.write(f"{i} 0 obj\n".encode("latin-1"))
        out.write(body)
        out.write(b"endobj\n")

    xref_offset = out.tell()
    out.write(f"xref\n0 {len(objects) + 1}\n".encode("latin-1"))
    out.write(b"0000000000 65535 f \n")
    for off in offsets[1:]:
        out.write(f"{off:010d} 00000 n \n".encode("latin-1"))
    out.write(b"trailer\n")
    out.write(
        (
            f"<< /Size {len(objects) + 1} /Root {catalog_id} 0 R >>\n"
        ).encode("latin-1")
    )
    out.write(b"startxref\n")
    out.write(f"{xref_offset}\n".encode("latin-1"))
    out.write(b"%%EOF\n")
    return out.getvalue()


__all__ = [
    "PassportEntry",
    "PassportMetadata",
    "build_passport_pdf",
]
