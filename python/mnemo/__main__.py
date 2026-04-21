"""`python -m mnemo doctor` — availability diagnostic entrypoint.

Examples::

    python -m mnemo doctor
    python -m mnemo --help
"""

from __future__ import annotations

import argparse
import sys

from mnemo.availability import doctor


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m mnemo")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("doctor", help="Print an availability report for the native extension + adapters.")
    args = parser.parse_args(argv)
    if args.command == "doctor":
        return doctor()
    parser.print_help()
    return 2


if __name__ == "__main__":  # pragma: no cover
    sys.exit(main())
