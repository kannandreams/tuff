#!/usr/bin/env python3
"""Fetch basic HTTP status using only the Python standard library."""

from __future__ import annotations

import argparse
import sys
import urllib.error
import urllib.parse
import urllib.request


def main() -> int:
    parser = argparse.ArgumentParser(description="Fetch HTTP status.")
    parser.add_argument("url", help="HTTP or HTTPS URL")
    parser.add_argument("--method", choices=["GET", "HEAD"], default="HEAD")
    parser.add_argument("--timeout-seconds", type=float, default=5.0)
    args = parser.parse_args()

    parsed = urllib.parse.urlparse(args.url)
    if parsed.scheme not in {"http", "https"}:
        print("url must use http or https", file=sys.stderr)
        return 2

    request = urllib.request.Request(args.url, method=args.method)
    try:
        with urllib.request.urlopen(request, timeout=args.timeout_seconds) as response:
            print(f"status={response.status}")
            print(f"reason={response.reason}")
            print(f"content_type={response.headers.get('content-type', '')}")
    except urllib.error.HTTPError as exc:
        print(f"status={exc.code}")
        print(f"reason={exc.reason}")
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
