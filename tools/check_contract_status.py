#!/usr/bin/env python3
"""Per-test gate for Stage 1/2 contract tests.

Pending tests listed in tools/pending-contract-tests.txt must fail because
they are still todo!() / unimplemented!(). Every other test must pass.
Compile errors, assertion failures, unexpected passes, missing pending
tests, or off-list failures all exit non-zero.

Usage:
    python3 tools/check_contract_status.py
    python3 tools/check_contract_status.py --log-file path/to/cargo-test.log
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LIST = Path(__file__).resolve().parent / "pending-contract-tests.txt"

# Cargo / GitHub Actions often colorize with CSI sequences such as
# "\x1b[1m\x1b[92m     Running\x1b[0m tests/foo.rs".
ANSI_RE = re.compile(r"\x1b(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])")

def strip_ansi(text: str) -> str:
    return ANSI_RE.sub("", text)


UNIMPL_RE = re.compile(r"not yet implemented|unimplemented!|not implemented")
COMPILE_RE = re.compile(r"error\[E[0-9]+\]|could not compile")
RUNNING_RE = re.compile(
    r"^\s*Running (?:tests/([A-Za-z0-9_]+)\.rs|unittests ([^\s]+))"
)
DOC_RE = re.compile(r"^\s*Doc-tests\s+(\S+)")
RESULT_RE = re.compile(r"^test (\S+) \.\.\. (ok|FAILED|ignored|ignored, .+)$")
STDOUT_HEADER_RE = re.compile(r"^---- (\S+) stdout ----$")
TEST_RESULT_SUMMARY_RE = re.compile(r"^test result:")


def load_pending(path: Path) -> list[str]:
    names: list[str] = []
    seen: set[str] = set()
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        if "::" not in line:
            raise SystemExit(f"invalid pending entry (expected binary::name): {raw!r}")
        if line in seen:
            raise SystemExit(f"duplicate pending entry: {line}")
        seen.add(line)
        names.append(line)
    if not names:
        raise SystemExit(f"pending list is empty: {path}")
    return names


def current_binary(running_line: str) -> str | None:
    m = RUNNING_RE.match(running_line)
    if m:
        if m.group(1):
            return m.group(1)
        target = m.group(2)
        if target.endswith("src/lib.rs") or target == "src/lib.rs":
            return "lib"
        return Path(target).stem
    m = DOC_RE.match(running_line)
    if m:
        return "doctests"
    return None


THREAD_PANIC_RE = re.compile(
    r"^thread '([^']+)'(?: \([^)]*\))? panicked at"
)


def parse_cargo_test_output(text: str) -> dict[str, dict[str, str]]:
    """Return {binary::name: {status, message}}.

    Accepts both the default libtest summary (``---- name stdout ----``)
    and interleaved / --nocapture panics (``thread 'name' panicked at``).
    ANSI color codes are stripped first so GitHub Actions
    (``CARGO_TERM_COLOR=always``) logs parse the same as local uncolored ones.
    """
    text = strip_ansi(text)
    results: dict[str, dict[str, str]] = {}
    binary: str | None = None
    collecting: str | None = None
    chunks: dict[str, list[str]] = {}

    def key(name: str) -> str:
        if binary is None:
            return name
        return f"{binary}::{name}"

    def append_msg(k: str, line: str) -> None:
        chunks.setdefault(k, []).append(line)

    for line in text.splitlines():
        maybe = current_binary(line)
        if maybe is not None:
            binary = maybe
            collecting = None
            continue
        header = STDOUT_HEADER_RE.match(line)
        if header:
            collecting = key(header.group(1))
            chunks.setdefault(collecting, [])
            continue
        panic = THREAD_PANIC_RE.match(line)
        if panic:
            collecting = key(panic.group(1))
            append_msg(collecting, line)
            continue
        if collecting and (
            TEST_RESULT_SUMMARY_RE.match(line) or line.strip() == "failures:"
        ):
            collecting = None
            continue
        m = RESULT_RE.match(line)
        if m:
            name, status = m.group(1), m.group(2)
            if status.startswith("ignored"):
                status = "ignored"
            k = key(name)
            results[k] = {"status": status, "message": ""}
            # A result line ends an interleaved panic block for that test,
            # but do not drop a stdout block already closed.
            if collecting == k:
                collecting = None
            continue
        if collecting is not None:
            append_msg(collecting, line)

    for k, lines in chunks.items():
        message = "\n".join(lines).strip()
        if k in results:
            results[k]["message"] = message
        else:
            results[k] = {"status": "FAILED", "message": message}
    return results


def is_unimplemented(message: str) -> bool:
    return bool(UNIMPL_RE.search(message))


def evaluate(results: dict[str, dict[str, str]], pending: list[str], log_text: str) -> int:
    errors: list[str] = []
    if COMPILE_RE.search(log_text):
        errors.append("compile error detected in cargo test output")

    pending_set = set(pending)

    for name in pending:
        info = results.get(name)
        if info is None:
            errors.append(f"pending test was not run: {name}")
            continue
        status = info["status"]
        if status == "ok":
            errors.append(f"pending test unexpectedly passed: {name}")
        elif status == "ignored":
            errors.append(f"pending test was ignored (must run and todo!()): {name}")
        elif status == "FAILED":
            if not is_unimplemented(info["message"]):
                errors.append(
                    f"pending test failed for a reason other than todo!()/unimplemented!(): {name}\n"
                    f"--- stdout ---\n{info['message'] or '(empty)'}\n--------------"
                )
        else:
            errors.append(f"pending test has unexpected status {status!r}: {name}")

    for name, info in sorted(results.items()):
        if name in pending_set:
            continue
        status = info["status"]
        if status == "FAILED":
            errors.append(
                f"non-pending test failed (must pass once implemented or be added to the pending list): {name}\n"
                f"--- stdout ---\n{info['message'] or '(empty)'}\n--------------"
            )
        elif status == "ignored":
            errors.append(f"non-pending test was ignored: {name}")
        elif status != "ok":
            errors.append(f"non-pending test has unexpected status {status!r}: {name}")

    failed_pending = [
        name
        for name in pending
        if results.get(name, {}).get("status") == "FAILED"
        and is_unimplemented(results[name]["message"])
    ]
    passed = [name for name, info in results.items() if info["status"] == "ok"]

    print(f"parsed tests: {len(results)}")
    print(f"pending listed: {len(pending)}")
    print(f"pending failed as todo!()/unimplemented!(): {len(failed_pending)}")
    print(f"passed: {len(passed)}")
    if errors:
        print(f"checker FAILED with {len(errors)} problem(s):")
        for i, err in enumerate(errors, 1):
            print(f"\n[{i}] {err}")
        return 1
    print("checker OK: pending tests failed only as unimplemented; all other tests passed.")
    return 0


def run_cargo() -> str:
    proc = subprocess.run(
        ["cargo", "test", "--workspace", "--no-fail-fast"],
        cwd=REPO_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    return proc.stdout


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", type=Path, default=DEFAULT_LIST, help="pending test list")
    parser.add_argument(
        "--log-file",
        type=Path,
        help="evaluate an existing cargo test log instead of running cargo",
    )
    args = parser.parse_args()

    pending = load_pending(args.list)
    if args.log_file:
        log_text = args.log_file.read_text(encoding="utf-8", errors="replace")
    else:
        log_text = run_cargo()
        sys.stdout.write(log_text)
        if not log_text.endswith("\n"):
            sys.stdout.write("\n")

    # Parse and judge the colorless text; keep the original stream for display.
    log_text = strip_ansi(log_text)
    results = parse_cargo_test_output(log_text)
    if COMPILE_RE.search(log_text) and not results:
        print("checker FAILED: cargo test did not compile")
        return 1
    return evaluate(results, pending, log_text)


if __name__ == "__main__":
    sys.exit(main())
