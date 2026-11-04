#!/usr/bin/env python3
"""Checks the `constructed` docket fixtures against a real docket, key by key.

`scripts/capture-docket-fixtures.sh` records what docket answers with no model, so the answers
docket sends only while a model runs (a finished task, a task running, a question of a consult
with several parts, a hop's trace events) stay hand-built from its spec. This script runs the
docket checkout's own end-to-end world (`tests/support/e2e.py`: a real `docket start --dispatch`
in a scratch home, a real git repository, and a scripted model on loopback, so nothing is spent),
reads those answers over HTTP and compares the keys of each with the fixture that stands for it:

    docket sends it, the fixture lacks it     (a key to add, or a feature to read)
    the fixture has it, docket no longer does (a stale key)

It writes nothing in the checkout or in ~/.docket, and uses no port but a free one it picks.

Usage (the docket checkout's own python, which has its test dependencies):
    DOCKET_SRC=~/Sites/rack-cli
    PYTHONDONTWRITEBYTECODE=1 $DOCKET_SRC/.venv/bin/python -B scripts/compare-constructed-docket-fixtures.py [--dump DIR]

--dump keeps the real answers in DIR. Exits 1 when any fixture differs.
"""

import json
import os
import pathlib
import shutil
import sys
import tempfile
import threading
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "crates/tack-runner/src/harness/fixtures/docket/v5"
SRC = pathlib.Path(os.environ.get("DOCKET_SRC", pathlib.Path.home() / "Sites/rack-cli")).expanduser()
sys.path.insert(0, str(SRC))

from tests.support.e2e import World  # noqa: E402
from tests.support.model_server import FakeModel, call, hold, say, write_file  # noqa: E402

PIPELINE = """name: board-task
steps:
  - plan: lead
  - implement: implementer
  - review: reviewer
    verdict: [APPROVE, REQUEST-CHANGES]
"""
SOLO = "name: solo\nsteps:\n  - implement: implementer\n"
# The body of a task as Tack posts it, so the keys docket echoes back are there to compare.
BODY = {
    "baseRef": "main",
    "maxTokens": 100000,
    "autonomy": "L3",
    "gate": {"command": "true", "timeoutS": 600},
    "restrictions": {"deniedTools": ["bash"]},
    "task": {"objective": "Make app.txt say fixed", "doneWhen": "app.txt says fixed", "acceptance": ["it says fixed"],
             "constraints": ["none"], "paths": ["app.txt"], "risk": "low", "kind": "Feature",
             "answers": [{"question": "Which file?", "answer": "app.txt"}]},
}
SINCE = "2000-01-01T00:00:00Z"


def question(number):
    return {"message": f"Question {number}?", "kind": "decision",
            "options": [{"id": "a", "label": "A", "description": "a"}, {"id": "b", "label": "B", "description": "b"}],
            "recommendation": {"optionId": "a", "rationale": "because"}}


class Scene:
    """One world with its model, torn down on exit."""

    def __init__(self, name, script, pipeline=PIPELINE, env=None, bind=True):
        self.model = FakeModel(script=script).start()
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix=f"docket-compare-{name}-", dir="/var/tmp"))
        self.world = World(self.dir / "world", self.model, env=env)
        self.world.make_pod(pipeline)
        if bind:
            self.world.docket("pod", "set", "pipeline", str(self.world.pipeline))
        self.pod = self.world.pod
        self.service = None

    def __enter__(self):
        self.context = self.world.service()
        self.service = self.context.__enter__()
        return self

    def __exit__(self, *exc):
        self.context.__exit__(*exc)
        self.model.stop()
        shutil.rmtree(self.dir, ignore_errors=True)

    def post(self, label, **extra):
        body = {"description": "Make app.txt say fixed", "externalRef": f"{label}/attempt-1", "idempotencyKey": label, **BODY, **extra}
        status, created = self.service.post(f"/tasks/{self.pod}", body)
        assert status == 201, created
        return created["task"]

    def wait(self, task, wanted, seconds=60):
        end = time.time() + seconds
        while time.time() < end:
            status, shown = self.service.get(f"/tasks/{self.pod}/{task}")
            if status == 200 and shown["status"] == wanted:
                return shown
            time.sleep(0.3)
        raise SystemExit(f"{task} never became {wanted}: {shown}")

    def get(self, path):
        return self.service.get(path)[1]


def real_answers():
    """Every real answer the comparison needs, by name."""
    found = {}
    team = {"lead": [say("Plan: fix app.txt."), say("Plan: again.")],
            "implementer": [write_file("app.txt", "fixed\n"), say("Done."), write_file("app.txt", "fixed again\n"), say("Done.")],
            "reviewer": [say("APPROVE"), say("APPROVE")]}
    with Scene("done", team) as scene:
        found["health"] = scene.get("/health")
        done = scene.post("done-1")
        found["task-done"] = scene.wait(done, "done")
        time.sleep(2)
        found["traces"] = scene.get(f"/traces/{scene.pod}?since={SINCE}")
        found["retry-done"] = scene.service.post(f"/tasks/{scene.pod}/{done}/retry")[1]
    release = threading.Event()
    held = {"lead": [say("Plan."), say("Plan.")],
            "implementer": [write_file("app.txt", "fixed\n"), say("Done."), hold(release, write_file("app.txt", "again\n")), say("Done.")],
            "reviewer": [say("APPROVE")]}
    with Scene("running", held) as scene:
        first = scene.post("running-1")
        scene.wait(first, "done")
        second = scene.post("running-2", idempotencyKey="running-2")
        end = time.time() + 40
        while time.time() < end:
            shown = scene.get(f"/tasks/{scene.pod}/{second}")
            if shown["status"] == "running" and (shown.get("current") or {}).get("role") == "implementer":
                break
            time.sleep(0.2)
        found["task-running"] = shown
        found["tasks-list"] = scene.get(f"/tasks/{scene.pod}")
        found["tasks-updated-since"] = scene.get(f"/tasks/{scene.pod}?updatedSince={shown['rev'] - 1}")
        release.set()
    asking = {"implementer": [call("consult", "k1", questions=[question(1), question(2)]), write_file("x.txt", "x\n"), say("Done.")]}
    with Scene("consult", asking, SOLO) as scene:
        scene.world.docket("pod", "set", "consultMode", "park", "--pod", scene.pod)
        task = scene.post("consult-1")
        scene.wait(task, "waiting_input")
        found["escalations-consult"] = scene.get("/escalations?since=0")
        first_id = found["escalations-consult"]["escalations"][0]["id"]
        found["escalation-accepted-consult"] = scene.service.post(f"/escalations/{first_id}", {"optionId": "a", "actor": "tack"})[1]
    found.update(conversation_answers())
    return found


def conversation_answers():
    """A conversation task's turns and a consult of two questions, as docket sends them. (Its refusal and
    a method the team cannot staff are captured by capture-docket-fixtures.sh: no model is needed.)"""
    found = {}
    turns = {"implementer": [say("First answer."), say("Second answer."), say("Third answer.")]}
    with Scene("turns", turns, SOLO) as scene:
        task = scene.post("turn-1", conversation={"maxTurns": 2})
        found["task-waiting-turn"] = scene.wait(task, "waiting_input")
        found["escalations-turn"] = scene.get("/escalations?since=0")
        turn_id = found["escalations-turn"]["escalations"][0]["id"]
        found["escalation-accepted-turn"] = scene.service.post(
            f"/escalations/{turn_id}", {"optionId": "reply", "content": {"text": "And now?"}, "actor": "tack"})[1]
        found["task-done-turn"] = scene.wait(task, "done")
    with Scene("close", turns, SOLO) as scene:
        task = scene.post("turn-close", conversation={"maxTurns": 5})
        scene.wait(task, "waiting_input")
        turn_id = scene.get("/escalations?since=0")["escalations"][0]["id"]
        found["escalation-accepted-turn-close"] = scene.service.post(
            f"/escalations/{turn_id}", {"optionId": "close", "actor": "tack"})[1]
        found["task-done-turn-close"] = scene.wait(task, "done")
    asking = {"implementer": [call("consult", "k1", questions=[question(1), question(2)]), write_file("x.txt", "x\n"), say("Done.")]}
    with Scene("parts", asking, SOLO) as scene:
        scene.world.docket("pod", "set", "consultMode", "park", "--pod", scene.pod)
        task = scene.post("parts-1")
        scene.wait(task, "waiting_input")
        found["escalations-consult-parts"] = scene.get("/escalations?since=0")
        first, second = [e["id"] for e in found["escalations-consult-parts"]["escalations"]]
        found["escalation-accepted-consult-part-1"] = scene.service.post(f"/escalations/{first}", {"optionId": "a", "actor": "tack"})[1]
        found["task-after-part-1"] = scene.get(f"/tasks/{scene.pod}/{task}")
        found["escalation-accepted-consult-part-2"] = scene.service.post(f"/escalations/{second}", {"optionId": "b", "actor": "tack"})[1]
        found["task-after-part-2"] = scene.get(f"/tasks/{scene.pod}/{task}")
    return found


def paths(value, prefix=""):
    """The key paths of a JSON value; a list is `[]`, a map of free keys is one `{}`."""
    found = set()
    if isinstance(value, dict):
        for key, inner in value.items():
            where = f"{prefix}.{key}"
            found.add(where)
            found |= paths(inner, where)
    elif isinstance(value, list):
        for inner in value:
            found |= paths(inner, prefix + "[]")
    return found


# The values under these paths are maps whose keys are data (a model name), not a shape.
FREE = (".usage.byModel", ".variables")


def shape(value):
    return {p for p in paths(value) if not any(free in p and p.split(free)[1].count(".") > 0 for free in FREE)}


def events(page):
    """The lines of a trace page, one example of each event type: its keys, payload keys included."""
    out = {}
    for line in page["events"]:
        event = json.loads(line)
        out.setdefault(event["event_type"], event)
    return out


def fixture(name):
    return json.loads((FIXTURES / f"{name}.json").read_text())


# fixture -> (real answer, the paths left out of both sides: those only some states have, and the
# echoes of what a request sent that this script does not send)
ECHO = {".pipeline", ".restrictions.network", ".tasks[].pipeline", ".tasks[].restrictions.network"}
PAIRS = {
    "task-done": ("task-done", {".escalations", ".consultations", ".blockedReason"} | ECHO),
    "tasks-list": ("tasks-list", ECHO),
    # Its task was started at the CLI, so none of what a request sends is there to echo.
    "tasks-updated-since": ("tasks-updated-since", {f".tasks[].{key}" for key in (
        "pipeline", "baseRef", "externalRef", "idempotencyKey", "gate", "maxTokens", "restrictions", "autonomy", "task")}),
    "escalations-consult": ("escalations-consult", set()),
    "escalation-accepted-consult": ("escalation-accepted-consult", {".resolution.content.note"}),
    # A conversation task between its turns, its last turn, the turn's escalation and the two ways
    # to answer it.
    "escalations-turn": ("escalations-turn", set()),
    "escalation-accepted-turn": ("escalation-accepted-turn", set()),
    "escalation-accepted-turn-close": ("escalation-accepted-turn-close", set()),
    "task-waiting-turn": ("task-waiting-turn", ECHO),
    "task-done-turn": ("task-done-turn", ECHO),
    # A consult of two questions: each is its own record with its part.
    "escalations-consult-parts": ("escalations-consult-parts", set()),
}


def main():
    dump = None
    if "--dump" in sys.argv:
        dump = pathlib.Path(sys.argv[sys.argv.index("--dump") + 1])
        dump.mkdir(parents=True, exist_ok=True)
    real = real_answers()
    if dump:
        for name, body in real.items():
            (dump / f"{name}.json").write_text(json.dumps(body, indent=2) + "\n")
    drift = []
    for name, (answer, skip) in PAIRS.items():
        def left(keys):
            return {key for key in keys if not any(key == out or key.startswith(out + ".") for out in skip)}

        ours, theirs = left(shape(fixture(name))), left(shape(real[answer]))
        for added in sorted(theirs - ours):
            drift.append(f"{name}: docket sends {added}")
        for stale in sorted(ours - theirs):
            drift.append(f"{name}: the fixture has {stale}, docket does not send it")
    lines = events(real["traces"])
    for event_type, fixture_name in (("hop_started", "traces-events"), ("hop_finished", "hop-finished-cached")):
        constructed = fixture(fixture_name)
        constructed = json.loads(constructed["events"][0]) if "events" in constructed else {"payload": constructed}
        ours, theirs = shape(constructed), shape(lines[event_type])
        if fixture_name == "hop-finished-cached":
            ours = {p for p in ours if p.startswith(".payload")}
            theirs = {p for p in theirs if p.startswith(".payload")}
        for added in sorted(theirs - ours):
            drift.append(f"{fixture_name}: docket sends {added}")
        for stale in sorted(ours - theirs):
            drift.append(f"{fixture_name}: the fixture has {stale}, docket does not send it")
    print("\n".join(drift) if drift else "the constructed fixtures have docket's keys")
    sys.exit(1 if drift else 0)


if __name__ == "__main__":
    main()
