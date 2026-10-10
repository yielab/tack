#!/usr/bin/env python3
"""Runs prill as a Tack runner verifier and writes the pack Tack reads.

The runner starts the program named in its `[verify]` table as
`<program> <args…> --evidence <dir> --workspace <dir> --output <dir>/mrp.json`
with `PATH` as its only environment variable (`crates/tack-runner/src/verify.rs`).
prill's own command line is `prill verify --repo DIR --evidence FILE --brief FILE
--out DIR`, it reads commits only, and the pack it writes (`mrp/v1`, camelCase,
decisions `reject` / `human-review` / `auto-merge-eligible`) is not the pack Tack
parses (`docs/contracts/mrp-v1`, snake_case, decisions `merge` / `review` /
`reject`). This program is the seam between the two:

1. `prill snapshot` commits what the agent left in the workspace, without
   touching its index or HEAD; the runner's own markers (`.tack-runner/`,
   `.tack-attempt`, `.tack-checkout`) are kept out of that commit.
2. Tack's `evidence.json` (evidence-v1) becomes an `attempt-evidence/v0`
   document, and `brief.json` (brief-v1), when present, a `briefing/v1` one.
3. `prill verify` runs, in the container named by `--image` or on the host with
   `--no-sandbox`, and its `mrp.json` is translated into Tack's mrp-v1 and written
   to `--output`. prill's own `mrp.json` and `mrp.md` stay beside it, under
   `<evidence dir>/prill/`, for a person who wants the untranslated pack.

Exit 0 whenever prill emitted a pack (its exit 1 means `reject`, which the pack
itself says); exit 2 when it could not, with prill's error on stderr, which the
runner records as an `attempt.verify_failed` event.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

RUNNER_MARKERS = (".tack-runner/", ".tack-attempt", ".tack-checkout")

DECISIONS = {"auto-merge-eligible": "merge", "human-review": "review", "reject": "reject"}
RISK_BY_DECISION = {"auto-merge-eligible": "low", "human-review": "medium", "reject": "high"}
CRITERION_STATUS = {"pass": "passed", "fail": "failed", "manual": "manual", "not_run": "skipped"}


def fail(message: str, code: int = 2) -> None:
    print(f"prill-tack-verify: {message}", file=sys.stderr)
    sys.exit(code)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sandbox = parser.add_mutually_exclusive_group()
    sandbox.add_argument("--image", help="sandbox image prill runs the change in")
    sandbox.add_argument("--no-sandbox", action="store_true", help="run on the host instead")
    parser.add_argument("--db", help="prill store to record the pack in")
    parser.add_argument("--evidence", required=True, help="the runner's staged evidence directory")
    parser.add_argument("--workspace", required=True, help="the attempt's workspace")
    parser.add_argument("--output", required=True, help="where the Tack-shaped mrp.json goes")
    return parser.parse_args(argv)


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_bytes())
    except (OSError, ValueError) as error:
        fail(f"{path} could not be read as JSON: {error}")


def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def measured(value: Any) -> Any:
    """A Tack `Measurement` carries its value only when it was measured."""
    if isinstance(value, dict) and value.get("source") == "measured":
        return value.get("value")
    return None


def run(args: list[str], *, env: dict[str, str], cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, env=env, cwd=cwd, capture_output=True, text=True, check=False)


def snapshot(workspace: Path, env: dict[str, str]) -> dict[str, Any]:
    """`prill snapshot` with the runner's markers excluded through git's own
    per-process configuration, never a file written into the user's repository."""
    with tempfile.TemporaryDirectory(prefix="prill-tack-") as scratch:
        excludes = Path(scratch) / "excludes"
        excludes.write_text("".join(f"{marker}\n" for marker in RUNNER_MARKERS), encoding="utf-8")
        git_env = {
            **env,
            "GIT_CONFIG_COUNT": "1",
            "GIT_CONFIG_KEY_0": "core.excludesFile",
            "GIT_CONFIG_VALUE_0": str(excludes),
        }
        result = run(["prill", "snapshot", "--repo", str(workspace), "--json"], env=git_env)
    if result.returncode != 0:
        fail(f"prill snapshot exited {result.returncode}: {result.stderr.strip()[:512]}")
    try:
        return json.loads(result.stdout)
    except ValueError:
        fail(f"prill snapshot printed no document: {result.stdout[:200]!r}")


def to_prill_evidence(tack: dict[str, Any], head_commit: str) -> dict[str, Any]:
    usage = tack.get("usage") if isinstance(tack.get("usage"), dict) else {}
    document: dict[str, Any] = {
        "contract": "attempt-evidence/v0",
        "attemptId": tack["attempt_id"],
        "harness": {"name": tack.get("harness_kind") or "tack-runner"},
        "baseCommit": tack["base_commit"],
        "headCommit": head_commit,
    }
    version = usage.get("harness_version")
    if isinstance(version, str) and version:
        document["harness"]["version"] = version
    model = usage.get("model_id")
    provider = usage.get("model_provider")
    if isinstance(model, str) and model:
        generator: dict[str, Any] = {"model": model}
        # prill compares the judge by declared family; the provider the
        # runner recorded is the nearest fact the evidence has.
        if isinstance(provider, str) and provider:
            generator["family"] = provider
        document["generator"] = generator
    tokens_in = measured(usage.get("tokens_in"))
    tokens_out = measured(usage.get("tokens_out"))
    if isinstance(tokens_in, int) and isinstance(tokens_out, int):
        prill_usage: dict[str, Any] = {"inputTokens": tokens_in, "outputTokens": tokens_out}
        duration_ms = measured(usage.get("duration_ms"))
        if isinstance(duration_ms, int):
            prill_usage["durationS"] = duration_ms / 1000
        document["usage"] = prill_usage
    return document


def to_prill_brief(brief: dict[str, Any]) -> dict[str, Any]:
    """brief-v1 → briefing/v1. A metric criterion and a max_changed_files
    constraint have no prill shape; they are named in `riskHints` rather than
    dropped silently."""
    acceptance: list[dict[str, Any]] = []
    hints: list[str] = []
    for criterion in brief.get("acceptance", []):
        kind = criterion.get("kind")
        base = {"id": criterion["id"], "description": criterion.get("title") or criterion["id"]}
        if kind == "command":
            acceptance.append({**base, "type": "command", "command": criterion["run"], "expectExitCode": int(criterion.get("expect_exit", 0))})
        elif kind == "test":
            acceptance.append({**base, "type": "test", "name": criterion["name"]})
        elif kind in ("file", "absent"):
            acceptance.append({**base, "type": kind, "path": criterion["path"]})
        elif kind == "manual":
            acceptance.append({"id": criterion["id"], "type": "manual", "question": criterion.get("title") or criterion["id"], "rubric": criterion.get("text") or ""})
        elif kind == "metric":
            hints.append(f"metric criterion {criterion['id']} ({criterion.get('title', '')}) has no prill equivalent and was not checked")
    forbidden: list[str] = []
    allowed: list[str] = []
    for constraint in brief.get("constraints", []):
        kind = constraint.get("kind")
        if kind == "forbidden_path":
            forbidden.append(constraint["glob"])
        elif kind == "allowed_dependency":
            allowed.append(constraint["name"])
        elif kind == "max_changed_files":
            hints.append(f"at most {constraint['n']} changed files (not enforced by prill)")
        elif kind == "note":
            hints.append(constraint["text"])
    constraints: list[dict[str, Any]] = []
    if forbidden:
        constraints.append({"type": "forbidden_paths", "globs": forbidden})
    if allowed:
        constraints.append({"type": "allowed_dependencies", "names": allowed})
    document: dict[str, Any] = {
        "contract": "briefing/v1",
        "id": str(brief.get("item_id") or "tack-item"),
        "objective": brief.get("definition_of_done") or f"Item {brief.get('item_id', '')}".strip(),
        "acceptance": acceptance,
    }
    if constraints:
        document["constraints"] = constraints
    if hints:
        document["riskHints"] = hints
    return document


def command_note(evidence: dict[str, Any] | None) -> str | None:
    if not isinstance(evidence, dict):
        return None
    if evidence.get("kind") == "command":
        if evidence.get("timedOut"):
            return "timed out"
        return f"exit {evidence.get('exitCode')}, expected {evidence.get('expectedExitCode', 0)}"
    if evidence.get("kind") == "file":
        return "exists" if evidence.get("exists") else "missing"
    return None


def stage_tokens(pack: dict[str, Any]) -> tuple[int, int]:
    tokens_in = tokens_out = 0
    for stage in pack.get("stages", []):
        tokens = stage.get("tokens")
        if isinstance(tokens, dict):
            tokens_in += int(tokens.get("input") or 0)
            tokens_out += int(tokens.get("output") or 0)
    return tokens_in, tokens_out


def to_tack_pack(pack: dict[str, Any], tack_evidence: dict[str, Any], evidence_sha: str, brief_sha: str | None) -> dict[str, Any]:
    decision = pack["recommendation"]["decision"]
    reasons = [
        f"{reason.get('severity')}: {reason.get('code')} — {reason.get('detail')}"
        for reason in pack["recommendation"].get("reasons", [])
    ]
    flags = [f"flag: {flag.get('code')} — {flag.get('detail')}" for flag in pack.get("flags", [])]
    risk = pack.get("risk") if isinstance(pack.get("risk"), dict) else None
    signals = [
        f"{signal.get('level')}: {signal.get('rule')} {signal.get('detail') or ''}".strip()
        for signal in (risk or {}).get("signals", [])
    ]

    criteria = [
        {
            "id": criterion["id"],
            "kind": criterion["type"],
            "status": CRITERION_STATUS.get(criterion["status"], "skipped"),
            "evidence_ref": None,
            "note": criterion.get("reason") or command_note(criterion.get("evidence")),
        }
        for criterion in pack.get("criteria", [])
    ]

    suite = pack.get("suite") if isinstance(pack.get("suite"), dict) else None
    verify = None
    if suite and isinstance(suite.get("command"), dict):
        command = suite["command"]
        totals = suite.get("totals") or {}
        tail = command.get("outputTail") or ""
        if totals:
            tail = f"{tail}\n[prill] {totals.get('passed', 0)} passed, {totals.get('failed', 0)} failed, {totals.get('errors', 0)} errors, {totals.get('skipped', 0)} skipped".strip()
        verify = {
            "command": command.get("command", ""),
            "exit_code": int(command.get("exitCode") if command.get("exitCode") is not None else -1),
            "output_tail": tail or None,
            "duration_s": command.get("durationS"),
        }

    mutation = pack.get("mutation") if isinstance(pack.get("mutation"), dict) else None
    mutation_section = None
    if mutation and isinstance(mutation.get("totals"), dict):
        totals = mutation["totals"]
        mutation_section = {
            "scope": "changed_files",
            "score": float(mutation.get("score") or 0.0),
            "killed": int(totals.get("killed") or 0),
            "survived": int(totals.get("survived") or 0),
        }

    judge = pack.get("judge") if isinstance(pack.get("judge"), dict) else None
    judge_section = None
    if judge:
        judge_section = {
            "model_family": judge.get("family") or "unknown",
            "blind": True,
            "rubric": [
                {
                    "criterion_id": verdict.get("id", ""),
                    "verdict": "pass" if verdict.get("answer") == "yes" else "fail",
                    "reason": verdict.get("rationale") or (f"answer: {verdict.get('answer')}" if verdict.get("answer") != "yes" else None),
                }
                for verdict in judge.get("verdicts", [])
            ],
        }

    tokens_in, tokens_out = stage_tokens(pack)
    rationale = f"prill: {decision}"
    if reasons:
        rationale += " — " + "; ".join(reasons)
    return {
        "v": "1",
        "attempt_id": tack_evidence["attempt_id"],
        "evidence_sha256": evidence_sha,
        "brief_sha256": brief_sha,
        "criteria": criteria,
        "verify": verify,
        "mutation": mutation_section,
        # prill reports static analysis per tool and per tree, not as one
        # SARIF digest with counts; nothing honest fits Tack's section.
        "static_analysis": None,
        "judge": judge_section,
        "risk": {
            "tier": (risk or {}).get("level") or RISK_BY_DECISION[decision],
            "reasons": reasons + flags + signals,
        },
        "recommendation": {"decision": DECISIONS[decision], "rationale": rationale},
        "usage": {"tokens_in": tokens_in, "tokens_out": tokens_out},
        "signature": {"kind": "none"},
        "produced_by": {"program": "prill", "version": str((pack.get("prill") or {}).get("version") or "unknown")},
    }


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    evidence_dir = Path(args.evidence)
    workspace = Path(args.workspace)
    output = Path(args.output)
    env = {"PATH": os.environ.get("PATH", "")}
    if shutil.which("prill", path=env["PATH"]) is None:
        fail("prill is not on PATH")

    tack_evidence_path = evidence_dir / "evidence.json"
    tack_evidence = read_json(tack_evidence_path)
    if not tack_evidence.get("captured"):
        fail(f"the runner captured no evidence: {tack_evidence.get('reason')}")
    base_commit = tack_evidence.get("base_commit")
    if not isinstance(base_commit, str) or not base_commit:
        fail("evidence.json names no base_commit")

    snap = snapshot(workspace, env)
    head_commit = snap.get("commit")
    if not isinstance(head_commit, str):
        fail("prill snapshot reported no commit")

    out_dir = evidence_dir / "prill"
    out_dir.mkdir(parents=True, exist_ok=True)
    prill_evidence = out_dir / "attempt-evidence.json"
    prill_evidence.write_text(json.dumps(to_prill_evidence(tack_evidence, head_commit), indent=2) + "\n", encoding="utf-8")

    command = ["prill", "verify", "--repo", str(workspace), "--evidence", str(prill_evidence), "--out", str(out_dir), "--json"]
    tack_brief_path = evidence_dir / "brief.json"
    brief_sha = None
    if tack_brief_path.is_file():
        brief_sha = sha256_of(tack_brief_path)
        prill_brief = out_dir / "briefing.json"
        prill_brief.write_text(json.dumps(to_prill_brief(read_json(tack_brief_path)), indent=2) + "\n", encoding="utf-8")
        command += ["--brief", str(prill_brief)]
    if args.image:
        command += ["--image", args.image]
    else:
        command.append("--no-sandbox")
    if args.db:
        command += ["--db", args.db]

    result = run(command, env=env)
    sys.stderr.write(result.stderr)
    if result.returncode not in (0, 1):
        # Exit 2: stdout is an error/v1 document (or nothing). Surface its code.
        try:
            error = json.loads(result.stdout)
            fail(f"prill refused: {error.get('code')}: {error.get('message')}")
        except ValueError:
            fail(f"prill exited {result.returncode} without a pack")
    try:
        pack = json.loads(result.stdout)
    except ValueError:
        fail("prill exited with a pack status but printed none")

    tack_pack = to_tack_pack(pack, tack_evidence, sha256_of(tack_evidence_path), brief_sha)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(tack_pack, indent=2) + "\n", encoding="utf-8")
    print(
        f"prill-tack-verify: {pack['recommendation']['decision']} -> {output} (prill's own pack: {out_dir / 'mrp.json'})",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
