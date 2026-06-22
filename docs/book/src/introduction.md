# Tack

[![CI](https://github.com/yielab/tack/actions/workflows/ci.yml/badge.svg?branch=develop)](https://github.com/yielab/tack/actions/workflows/ci.yml)

**Local-first, single-binary project management for solo developers and small teams.**

Tack tracks work for any domain — software sprints, a kitchen renovation, thesis chapters, a
maintenance schedule — through fully configurable vocabulary and workflow columns. No accounts.
No cloud. No subscriptions. One binary, one SQLite file.

---

## How this documentation is organized

| Section | For whom |
|---------|----------|
| **User Guide** | Anyone running Tack: setup, views, CLI, configuration |
| **Developer Guide** | Contributors and people extending the codebase |
| **Learning Path** | Developers new to Rust, Axum, or SolidJS; explains the stack with analogies |
| **Roadmap** | Planned work and known gaps |

## Quick links

- [Quick Start](user-guide/quick-start.md) — up and running in five minutes
- [Architecture Overview](developer/README.md) — the mental model behind the codebase
- [Rust Primer](developer/learning/rust-primer.md) — start here if Rust is new to you
- [API Reference](developer/api-reference.md) — all 34 REST endpoints

## Keeping docs current

These docs live alongside the code in `docs/book/src/`. Every push to `develop` runs
`mdbook build` and `mdbook test` in CI — code examples in the book are compiled and executed
as tests. If an API changes and a doc example stops compiling, the build fails.

For prose changes (user-facing descriptions, learning explanations), update the relevant
`.md` file in the same PR as your code change. The "Edit this page" link at the top of each
page opens the file directly on GitHub.

```
docs/book/src/
├── user-guide/        ← user-facing docs
├── developer/         ← contributor docs
│   └── learning/      ← stack explanation with analogies
└── roadmap.md
```
