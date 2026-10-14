captured
prill 0.1.0 from source (~/Sites/prill @ 631b4cd, M6 wave 1: the contracts of require, nextSteps and coverage, no enforcement yet), `uv sync --extra sign`, sandbox image prill-verify-py313:local (sha256:3bb5d928…).

A scratch repository: base d705833ec4b0ee5a4ec97e5c8bb2a88f0521922f (calc.add and its test, prill.yaml with tests.command), head ad0eea1455de382abb4f7c41129d524a14ddcbf5 on agent/sub (adds calc.sub and test_sub). Brief briefing.json (one command criterion).
`prill attest keygen --out keys` (the private key was not kept); `prill verify --repo repo --base <base> --head <head> --out out --brief brief.json --image <id> --sign keys/prill.key`: exit 0, auto-merge-eligible, signed.
Scratch paths in stdout are replaced by <dir>.
Files: mrp.json, mrp.intoto.json, mrp.md, prill.pub, briefing.json, verify.stdout, verify.exit.
