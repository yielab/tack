"""The exam of a planning task.

A Planner changes no file of the repository, so the tree it leaves must equal the seed. The
plan itself reaches the harness as an artifact, not as a file of the tree, and the harness's
own plan check judges it there; when a plan file is in the tree (the reference solution has
one), it is held to the same shape here so a broken reference fails.
"""
import hashlib
import json
import os
import sys

SEED = json.load(open("hidden/seed.json"))
WANT = json.load(open("hidden/want.json"))


def fail(message):
    print("FAIL:", message)
    sys.exit(1)


seen = []
for root, dirs, files in os.walk("."):
    dirs[:] = [d for d in dirs if d not in ("hidden", "__pycache__")]
    for name in files:
        if name.startswith("."):
            continue
        path = os.path.relpath(os.path.join(root, name), ".")
        if path == "tack-plan.json":
            continue
        seen.append(path)
        digest = hashlib.sha256(open(path, "rb").read()).hexdigest()
        if SEED.get(path) != digest:
            fail(f"{path} is not part of the seed or was changed")
for path in SEED:
    if path not in seen:
        fail(f"{path} is gone")

if not os.path.exists("tack-plan.json"):
    sys.exit(0)

plan = json.load(open("tack-plan.json"))
if plan.get("v") != "1":
    fail('the plan is not plan-v1 ("v": "1")')
tasks = plan.get("subtasks")
if not isinstance(tasks, list) or not WANT["min"] <= len(tasks) <= WANT["max"]:
    fail(f"the plan needs {WANT['min']} to {WANT['max']} subtasks")
for index, task in enumerate(tasks):
    if not str(task.get("title", "")).strip():
        fail(f"subtask {index} has no title")
    if not isinstance(task.get("description"), str):
        fail(f"subtask {index} has no description (plan-v1 requires one)")
    if not task.get("acceptance"):
        fail(f"subtask {index} has no acceptance check")
    for dep in task.get("depends_on", []):
        if not isinstance(dep, int) or not 0 <= dep < len(tasks) or dep == index:
            fail(f"subtask {index} depends on {dep}")
    parent = task.get("parent")
    if parent is not None and not (isinstance(parent, int) and 0 <= parent < index):
        fail(f"subtask {index} has a parent that is not an earlier subtask")
if WANT.get("structure") and not any(t.get("parent") is not None or t.get("depends_on") for t in tasks):
    fail("no subtask is nested or ordered after another")
if WANT.get("role") and any(t.get("role") != WANT["role"] for t in tasks):
    fail(f"every subtask must name the role {WANT['role']}")
