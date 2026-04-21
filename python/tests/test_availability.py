"""Tests for the `mnemo.availability` module and `python -m mnemo doctor`."""

from __future__ import annotations

import io

from mnemo.availability import (
    MnemoClientUnavailable,
    doctor,
    installed_adapters,
    is_native_available,
    native_build_hint,
)


def test_is_native_available_returns_bool() -> None:
    assert isinstance(is_native_available(), bool)


def test_native_build_hint_is_nonempty_and_mentions_maturin() -> None:
    hint = native_build_hint()
    assert isinstance(hint, str) and hint
    assert "maturin" in hint.lower()


def test_installed_adapters_returns_status_dict() -> None:
    adapters = installed_adapters()
    assert isinstance(adapters, dict)
    # The core client entry must always appear, whether available or not.
    assert "MnemoClient" in adapters
    for name, status in adapters.items():
        assert isinstance(name, str)
        assert isinstance(status, str)
        assert status == "available" or status.startswith("missing:")


def test_typed_error_carries_build_hint() -> None:
    err = MnemoClientUnavailable("reason")
    assert "reason" in str(err)
    assert err.hint
    assert "maturin" in err.hint.lower()


def test_doctor_writes_report_and_returns_exit_code() -> None:
    buf = io.StringIO()
    code = doctor(stream=buf)
    output = buf.getvalue()
    assert "mnemo doctor" in output
    assert "Adapter probe:" in output
    assert code in (0, 1)


def test_doctor_exit_code_matches_native_availability() -> None:
    buf = io.StringIO()
    code = doctor(stream=buf)
    if is_native_available():
        assert code == 0, "native available → doctor should exit 0"
    else:
        assert code == 1, "native missing → doctor should exit 1"
