# Load / performance tests

HTTP-level load tests for the flexpm-api server, written for [k6](https://k6.io).

These establish a **performance baseline** before launch so regressions are
visible. They are intentionally **not** part of the default CI run (they need a
running server and are time-consuming) — run them on demand.

## Install k6

```bash
# Debian/Ubuntu
sudo gpg -k && \
  sudo gpg --no-default-keyring --keyring /usr/share/keyrings/k6-archive-keyring.gpg \
    --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69 && \
  echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" | \
    sudo tee /etc/apt/sources.list.d/k6.list && \
  sudo apt-get update && sudo apt-get install k6

# macOS
brew install k6
```

## Run

Start the API first (a throwaway DB is recommended):

```bash
FLEXPM_DATABASE_URL='sqlite:load.db?mode=rwc' cargo run -p flexpm-api --release
```

Then, from the repo root:

```bash
make load                 # default: read-heavy profile against localhost:3210
# or directly:
k6 run tests/load/smoke.js
BASE_URL=http://localhost:3210 k6 run tests/load/smoke.js
```

## What it checks

`smoke.js` ramps to 50 virtual users exercising the hot read path
(`GET /api/projects` and a project's items) plus a lighter write path
(`PATCH` an item). The thresholds encode the launch budget:

| Metric | Threshold | Why |
|--------|-----------|-----|
| `http_req_failed` | < 1% | correctness under concurrency |
| `http_req_duration` p95 | < 500 ms | interactive read latency |
| `writes` p95 | < 1 s | SQLite is single-writer — this is where it bottlenecks |

If the write p95 blows past the threshold under load, that's the documented
SQLite single-writer ceiling — the signal to consider WAL tuning or a connection
queue before scaling users.
