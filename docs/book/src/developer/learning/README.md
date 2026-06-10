# Learning Path

This section explains the FlexPM stack for developers coming from other backend and frontend backgrounds. It is not a comprehensive Rust or SolidJS tutorial — it is enough context to read, understand, and contribute to this specific codebase without drowning in language novelty.

You do not need to read all of it. Pick the chapters that match where you are.

---

## Suggested reading order

**Coming from Node.js / Express / TypeScript**

Start with [Rust for Backend Developers](rust-primer.md) to understand the type system and ownership model. Then [Async/Await in Rust](async-in-rust.md) — the concepts transfer directly, the mechanics differ slightly. [Axum — HTTP Without Magic](axum-primer.md) will feel familiar: it is close to Express in philosophy. Read [The Data Layer](data-layer.md) last if you work on database queries or migrations. If you touch the frontend, [SolidJS for Frontend Developers](solidjs-primer.md) shows how SolidJS differs from React.

**Coming from Python / Django / FastAPI**

Start with [Rust for Backend Developers](rust-primer.md) — the ownership section is the most important thing to internalize. [Async/Await in Rust](async-in-rust.md) is worth reading because Python's `asyncio` and Tokio have similar structure but different failure modes. [Axum — HTTP Without Magic](axum-primer.md) maps to FastAPI concepts well (both are typed, extractor-based). [The Data Layer](data-layer.md) will feel very different from SQLAlchemy/Django ORM — read it carefully.

**Coming from Java / Spring**

Start with [Rust for Backend Developers](rust-primer.md). The structs-and-traits model maps loosely to interfaces-and-classes, but the differences matter. [Axum — HTTP Without Magic](axum-primer.md) shows how Spring MVC concepts (controllers, dependency injection, request mapping) translate. Spring Boot does a lot more magic than Axum — the chapter explains what you have to do explicitly. [The Data Layer](data-layer.md) maps to plain JDBC + a DAO layer, not Hibernate.

---

## What each chapter covers

**[Rust for Backend Developers](rust-primer.md)** — Ownership, borrowing, structs, enums with data (`Option`, `Result`), traits, the module system, and error handling. Uses FlexPM models and error types as examples. This is the densest chapter; take it slow if ownership feels confusing.

**[Async/Await in Rust](async-in-rust.md)** — How Rust's async model compares to JavaScript Promises, Python asyncio, and Java's CompletableFuture. Covers Tokio (the runtime FlexPM uses), spawning tasks, broadcast channels, and the patterns you will see in every Axum handler.

**[Axum — HTTP Without Magic](axum-primer.md)** — How FlexPM's HTTP layer works. Routing, extractors (how request data flows into handler functions), shared state (`AppState`), response types, error mapping, and middleware. Concrete before/after comparisons to Express and FastAPI.

**[The Data Layer (sqlx & Repository Pattern)](data-layer.md)** — sqlx is not an ORM. It checks your SQL at compile time and maps rows to structs. Covers the Repository struct, migrations, JSON fields in SQLite, FTS5 search, and the auto-complete parent status logic.

**[SolidJS for Frontend Developers](solidjs-primer.md)** — For developers who know React (or Vue/Angular). SolidJS looks like React but never re-renders components. Covers signals, derived state, effects, control flow primitives (`<Show>`, `<For>`), context, routing, and what that means when reading FlexPM's frontend code.

---

## What this section does not cover

These chapters do not replace a full language tutorial. If you want to go deeper:

- Rust: [The Rust Book](https://doc.rust-lang.org/book/) is the canonical reference
- Async Rust: [Tokio's tutorial](https://tokio.rs/tokio/tutorial) covers tasks, channels, and the runtime in detail
- Axum: the [axum examples](https://github.com/tokio-rs/axum/tree/main/examples) repo is the best reference
- SolidJS: the [official tutorial](https://www.solidjs.com/tutorial) is interactive and covers everything in under an hour
