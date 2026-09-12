#!/usr/bin/env python3
"""Renders docs/openapi.json into docs/book/src/developer/api-reference.md.

docs/openapi.json is the spec of record (generated from the Rust handlers by
the openapi_contract test). This script is a pure function of that file: same
input, same output, nothing hand-maintained in between. Run it after the spec
is confirmed current — `./scripts/regen-generated.sh` does both in order.

Usage: python3 scripts/gen-api-reference.py
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SPEC_PATH = ROOT / "docs" / "openapi.json"
OUT_PATH = ROOT / "docs" / "book" / "src" / "developer" / "api-reference.md"

METHODS = ("get", "put", "post", "delete", "options", "head", "patch", "trace")


def schema_ref(schema: dict | None) -> str:
    """Names a schema without inlining it: a $ref's bare name, an array of
    one, or a short fallback for the inline shapes the spec has a few of."""
    if not schema:
        return "—"
    if "$ref" in schema:
        return "`" + schema["$ref"].rsplit("/", 1)[-1] + "`"
    if schema.get("type") == "array":
        inner = schema_ref(schema.get("items"))
        return f"{inner}[]" if inner != "—" else "array"
    for combinator in ("oneOf", "anyOf", "allOf"):
        if combinator in schema:
            return " | ".join(schema_ref(s) for s in schema[combinator])
    return f"`{schema.get('type', 'object')}`"


def request_schema(op: dict) -> str:
    body = op.get("requestBody")
    if not body:
        return "—"
    content = body.get("content", {})
    for media in ("application/json", "multipart/form-data", "text/csv"):
        if media in content:
            return schema_ref(content[media].get("schema"))
    return "—"


def response_rows(op: dict) -> list[tuple[str, str, str]]:
    rows = []
    for status, resp in sorted(op.get("responses", {}).items()):
        content = resp.get("content", {})
        schema = "—"
        for media in ("application/json", "text/csv", "application/octet-stream"):
            if media in content:
                schema = schema_ref(content[media].get("schema"))
                break
        rows.append((status, resp.get("description", "").split("\n")[0], schema))
    return rows


def param_rows(op: dict) -> list[tuple[str, str, str, str, str]]:
    rows = []
    for p in op.get("parameters", []):
        schema = p.get("schema", {})
        rows.append((
            p.get("name", ""),
            p.get("in", ""),
            schema_ref(schema) if "$ref" in schema or "type" in schema else "—",
            "yes" if p.get("required") else "no",
            (p.get("description") or "").split("\n")[0],
        ))
    return rows


# Handlers often lead their doc comment with the route itself, e.g.
# "`GET /api/approvals` — the fleet-wide inbox..." — the heading already
# shows the route, so drop that redundant lead-in when present.
_ROUTE_PREFIX = re.compile(
    r"^`?(GET|PUT|POST|DELETE|PATCH|HEAD|OPTIONS) \S+`?\s*[—-]\s*"
)


def operation_summary(op: dict) -> str:
    text = (op.get("summary") or op.get("description") or "").strip().split("\n")[0]
    return _ROUTE_PREFIX.sub("", text)


def render_operation(method: str, path: str, op: dict) -> str:
    lines = [f"#### `{method.upper()} {path}`", ""]
    summary = operation_summary(op)
    if summary:
        lines += [summary, ""]
    params = param_rows(op)
    if params:
        lines.append("| Param | In | Type | Required | Description |")
        lines.append("|---|---|---|---|---|")
        for name, loc, ptype, required, desc in params:
            lines.append(f"| `{name}` | {loc} | {ptype} | {required} | {desc} |")
        lines.append("")
    req = request_schema(op)
    if req != "—":
        lines += [f"**Request body:** {req}", ""]
    responses = response_rows(op)
    if responses:
        lines.append("| Status | Meaning | Schema |")
        lines.append("|---|---|---|")
        for status, desc, schema in responses:
            lines.append(f"| {status} | {desc} | {schema} |")
        lines.append("")
    return "\n".join(lines)


def main() -> int:
    spec = json.loads(SPEC_PATH.read_text())
    paths: dict = spec["paths"]

    tag_order = [t["name"] for t in spec.get("tags", [])]
    tag_desc = {t["name"]: t.get("description", "") for t in spec.get("tags", [])}

    by_tag: dict[str, list[tuple[str, str, dict]]] = {}
    for path, item in paths.items():
        for method in METHODS:
            if method not in item:
                continue
            op = item[method]
            tag = (op.get("tags") or ["untagged"])[0]
            by_tag.setdefault(tag, []).append((path, method, op))

    for ops in by_tag.values():
        ops.sort(key=lambda t: (t[0], t[1]))

    ordered_tags = [t for t in tag_order if t in by_tag]
    ordered_tags += sorted(t for t in by_tag if t not in tag_order)

    op_count = sum(len(v) for v in by_tag.values())
    out = [
        "# API Reference",
        "",
        f"Generated from [`docs/openapi.json`](../../../openapi.json) "
        f"({len(paths)} paths, {op_count} operations) by "
        "`scripts/gen-api-reference.py` — do not hand-edit. Regenerate with "
        "`./scripts/regen-generated.sh` after the spec changes.",
        "",
        "This page lists every path, method, parameter and request/response "
        "schema name. It does not inline schema bodies — load "
        "[`docs/openapi.json`](../../../openapi.json) into an OpenAPI viewer "
        "(Redocly, Scalar, Swagger Editor) for the full definitions, or read "
        "them directly in the spec file.",
        "",
        "Two authentication surfaces, the WebSocket endpoint (not in this "
        "spec), and worked examples are in "
        "[`docs/API-REFERENCE.md`](../../../API-REFERENCE.md).",
        "",
        "---",
        "",
    ]

    for tag in ordered_tags:
        title = tag.replace("-", " ").title()
        out.append(f"## {title}")
        out.append("")
        if tag_desc.get(tag):
            out.append(tag_desc[tag].split("\n")[0])
            out.append("")
        for path, method, op in by_tag[tag]:
            out.append(render_operation(method, path, op))
        out.append("---")
        out.append("")

    OUT_PATH.write_text("\n".join(out).rstrip() + "\n")
    print(f"wrote {OUT_PATH.relative_to(ROOT)} ({op_count} operations, {len(paths)} paths)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
