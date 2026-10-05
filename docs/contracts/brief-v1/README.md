# brief-v1

A typed brief that travels from the board through the harness to an independent verifier.
An `ItemBrief` contains acceptance criteria, constraints, a definition of done, and a risk assessment,
all shaped to support both human and automated verification.

## Structure

| Field | Type | Purpose |
|-------|------|---------|
| `item_id` | UUID | The item this brief describes |
| `acceptance` | array[AcceptanceCriterion] | Criteria that must be met (max 50) |
| `constraints` | array[Constraint] | Constraints that must be observed (max 100) |
| `definition_of_done` | string or null | Optional explicit definition of done |
| `risk` | `"low"` \| `"medium"` \| `"high"` or null | Risk level assessment |
| `created_at` | ISO 8601 datetime | Timestamp of creation |
| `updated_at` | ISO 8601 datetime | Timestamp of last update |

## Acceptance Criteria

Each criterion has a `kind` (command, test, metric, file, absent, manual), a unique `id` within
the brief, and a human-readable `title`.

- **command**: Runs a shell command with an expected exit code
  - `run`: The command to execute (max 2000 chars)
  - `expect_exit`: Expected exit code (0-255)
  - `cwd`: Optional working directory
- **test**: Runs a named test via a test runner
  - `name`: Test name or pattern
  - `runner`: Optional runner name (e.g., "pytest", "cargo test")
- **metric**: Validates a measurement against a threshold
  - `name`: Metric name
  - `op`: `"lte"`, `"gte"`, or `"eq"`
  - `threshold`: Numeric threshold value
  - `unit`: Optional unit of measurement (e.g., "%", "ms")
- **file**: Verifies a file exists at a path
  - `path`: File path to check
- **absent**: Verifies a file does NOT exist at a path
  - `path`: File path to check
- **manual**: A criterion requiring human judgment
  - `text`: Description of what must be verified

## Constraints

Each constraint restricts what changes are acceptable.

- **forbidden_path**: Prevents changes to paths matching a glob
  - `glob`: Glob pattern (e.g., "src/temp/*")
- **allowed_dependency**: Whitelist a dependency name
  - `name`: Dependency name
- **max_changed_files**: Limits the number of changed files
  - `n`: Maximum number of files
- **note**: A reminder about the item
  - `text`: The note text

## Validation

`schema.json` checks the shape, the two list lengths (≤ 50 criteria, ≤ 100 constraints),
`command.run` (≤ 2 000 characters) and `expect_exit` (0–255, a `u8` in Rust, so a larger
value cannot be represented). The one rule JSON Schema cannot express, criterion ids unique
within the brief, is checked by `tack_core::brief::validate`, which `UpsertItemBrief` runs
on every write together with the run length.

## Example

`example.json` uses every criterion and constraint kind. The test
`crates/tack-core/src/brief/tests.rs::the_example_round_trips_and_its_pin_holds` reads it as
an `ItemBrief`, writes it back byte-for-byte, and pins its FNV-1a 64 hash; change the
example and the type together, then the pin.
