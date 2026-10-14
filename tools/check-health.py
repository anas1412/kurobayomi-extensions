#!/usr/bin/env python3
"""Checks that each source's site answers, and keeps health.toml (read by make-index.py) up to date.

Each source folder may hold a health.toml:  url = "https://site/"  expect = "text the page has"  cloudflare = true
The check fetches the url like a browser would and sorts the answer:
- ok: HTTP 200 and the expected text is there.
- challenge: Cloudflare's browser check answered (cf-mitigated: challenge). This runner cannot pass it, but the app does, so
  nothing is known: the source keeps whatever status it had. A source with `cloudflare = true` is only ever checked this way.
- fail: no answer, an HTTP error, or the page without its expected text.
A source turns "broken" only after FAILS_TO_BREAK failures in a row (one bad moment is not a verdict) and active again at
the next ok. status.toml, written by hand, overrides all of this in make-index.py.

Usage: python3 tools/check-health.py [--state PATH] [--out PATH]
State (consecutive failures per source) is kept in health-state.json so the next run can count on it.
"""

import argparse
import json
import sys
import tomllib
import urllib.error
import urllib.request
from pathlib import Path

FAILS_TO_BREAK = 2
USER_AGENT = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0"


def classify(status, headers, body, expect):
    """ok, challenge or fail for one answer. `status` is None when there was none."""
    if status is None:
        return "fail"
    if headers.get("cf-mitigated", "").lower() == "challenge":
        return "challenge"
    if status == 200 and (not expect or expect.lower() in body.lower()):
        return "ok"
    return "fail"


def next_state(previous, result):
    """(status, consecutive failures) after a check. `previous` is (status, failures); status is active or broken."""
    status, fails = previous
    if result == "ok":
        return "active", 0
    if result == "challenge":
        return status, fails
    fails += 1
    return ("broken" if fails >= FAILS_TO_BREAK else status), fails


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept": "*/*"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status, {k.lower(): v for k, v in response.headers.items()}, response.read(2_000_000).decode("utf-8", "replace")
    except urllib.error.HTTPError as error:
        return error.code, {k.lower(): v for k, v in error.headers.items()}, error.read(200_000).decode("utf-8", "replace")
    except (urllib.error.URLError, TimeoutError, OSError):
        return None, {}, ""


def main():
    root = Path(__file__).resolve().parent.parent
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--state", type=Path, default=root / "health-state.json")
    parser.add_argument("--out", type=Path, default=root / "health.toml")
    args = parser.parse_args()
    state = json.loads(args.state.read_text()) if args.state.is_file() else {}
    kept = {}
    for folder in sorted((root / "sources").iterdir()):
        config = folder / "health.toml"
        if not config.is_file():
            continue
        check = tomllib.loads(config.read_text(encoding="utf-8"))
        status, headers, body = fetch(check["url"])
        result = classify(status, headers, body, check.get("expect", ""))
        if check.get("cloudflare") and result == "fail" and status in (403, 503):
            result = "challenge"  # a Cloudflare page this runner cannot read as one
        previous = state.get(folder.name, {"status": "active", "fails": 0})
        new_status, fails = next_state((previous["status"], previous["fails"]), result)
        kept[folder.name] = {"status": new_status, "fails": fails}
        print(f"{folder.name}: HTTP {status} -> {result}; {new_status} ({fails} in a row)")
    args.state.write_text(json.dumps(kept, indent=1, sort_keys=True) + "\n")
    lines = [
        "# Written by tools/check-health.py: do not edit (status.toml overrides it by hand).",
        *(f'{name} = "{value["status"]}"' for name, value in sorted(kept.items()) if value["status"] != "active"),
    ]
    args.out.write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    sys.exit(main())
