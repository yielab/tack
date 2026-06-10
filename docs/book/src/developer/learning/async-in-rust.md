# Async/Await in Rust

If you have written async code in JavaScript, Python, or Java, the concepts here transfer directly. The mechanics differ in ways that matter. This chapter covers what you need to know to read and write async code in FlexPM.

---

## Same concept, explicit runtime

`async fn` and `.await` work the same way conceptually as in other languages. The key difference: Rust does not ship with an async runtime. You choose one.

| Language | Runtime | Your choice? |
|----------|---------|-------------|
| Node.js | libuv | No — it is baked in |
| Python | asyncio | No — it is in the stdlib |
| Java | ForkJoinPool / virtual threads | Somewhat — you configure it |
| Rust | *none built in* | Yes — FlexPM uses Tokio |

FlexPM uses [Tokio](https://tokio.rs/), the most widely used async runtime in the Rust ecosystem. The entry point of the server annotates `main` with `#[tokio::main]`:

```rust
// crates/flexpm-api/src/main.rs

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::load();
    // ...
    let pool = init_pool(&config.database_url).await?;
    // ...
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}
```

`#[tokio::main]` is a macro that wraps your function in a Tokio runtime initialization. Everything inside `main` runs on the Tokio executor. Without this annotation, calling `.await` anywhere would be a compile error.

---

## Futures — Promises with a different name

An `async fn` returns a `Future<Output = T>`. The analogy to JavaScript is direct:

```
JS Promise<T>  ≈  Rust Future<Output = T>
async function ≈  async fn
await expr     ≈  expr.await
```

The one critical difference: **Futures are lazy**. In JavaScript, creating a `Promise` starts it running immediately. In Rust, a `Future` does nothing until you `.await` it (or pass it to an executor).

```rust
// This does nothing — the future is created but not driven:
let fut = sqlx::query("SELECT 1").execute(&pool);

// This actually runs it:
let fut = sqlx::query("SELECT 1").execute(&pool).await;
```

This laziness is a feature. It means you can construct, compose, and cancel futures before they run. In practice, you almost always just chain `.await` immediately, so this rarely catches you off guard.

---

## Why this matters for FlexPM

Every interaction with the database or network is async. The `sqlx` queries do not block a thread while waiting for disk I/O — they yield control to Tokio, which runs other tasks in the meantime:

```rust
// This does not block a thread for the duration of the disk read:
let item = state.repo.get_item(id).await?;

// While SQLite is reading, Tokio can process other HTTP requests
// on the same thread pool.
```

The Axum HTTP server is also fully async. A server handling 1,000 concurrent connections does not need 1,000 threads — Tokio multiplexes them on a small thread pool (by default, one thread per CPU core).

The WebSocket handler in `crates/flexpm-api/src/handlers/websocket.rs` is the clearest example of the async model in action:

```rust
async fn handle_socket(socket: WebSocket, project_id: Uuid, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to the broadcast channel
    let mut rx = state.broadcast_tx.subscribe();

    // Spawn one task to forward broadcast events to this client
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if event_matches_project(&event, project_id) {
                let msg = serde_json::to_string(&event).unwrap();
                if sender.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // Spawn another task to read messages from the client
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            // handle pings, close frames, etc.
        }
    });

    // Wait for either task to finish, then abort the other
    tokio::select! {
        _ = &mut send_task => { recv_task.abort(); }
        _ = &mut recv_task => { send_task.abort(); }
    }
}
```

Each WebSocket connection spawns two lightweight async tasks that run concurrently. There is no thread-per-connection overhead.

---

## Spawning tasks

`tokio::spawn` runs a future concurrently, similar to `asyncio.create_task()` in Python or `setTimeout(fn, 0)` in JavaScript:

```rust
// Fire-and-forget background work
tokio::spawn(async move {
    let result = do_background_work().await;
    // ...
});
```

The spawned task runs independently of the caller. `tokio::spawn` returns a `JoinHandle<T>` that you can `.await` to get the result, or ignore if you do not care when it finishes. In FlexPM's WebSocket handler, `tokio::select!` waits for the first of two tasks to complete, then cleans up the other.

---

## The broadcast channel

The real-time board updates use Tokio's broadcast channel:

```rust
// Created once at startup in main.rs:
let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);

// Stored in AppState — every handler and WebSocket connection
// shares the same sender:
pub struct AppState {
    pub broadcast_tx: broadcast::Sender<BoardEvent>,
    // ...
}
```

`broadcast::channel(100)` creates a multi-producer, multi-consumer channel with a buffer of 100 messages. Any handler can send an event:

```rust
// In items.rs, after updating an item:
websocket::broadcast_event(
    &state,
    BoardEvent::ItemUpdated {
        project_id: item.project_id,
        item_id: item.id,
        old_status: Some(old_status),
        new_status: item.status.clone(),
    },
);
```

Each WebSocket connection calls `state.broadcast_tx.subscribe()` to get its own receiver. The event is delivered to every active subscriber. The filter in `handle_socket` discards events for other projects.

---

## Common async patterns in this codebase

**Handler signature:**

```rust
#[instrument(skip(state))]
pub async fn create_item(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(input): Json<CreateItem>,
) -> ApiResult<Json<serde_json::Value>> {
    // ...
}
```

Every handler is `async fn`. The `#[instrument(skip(state))]` macro from `tracing` wraps the function in a tracing span — you get timing and structured logging automatically.

**Awaiting a query:**

```rust
let item = state
    .repo
    .get_item(id)
    .await?;   // await the future; ? propagates sqlx::Error → ApiError
```

**Chained async operations with `?`:**

```rust
let project = state.repo.get_project(project_id).await?;
let initial_status = project.workflow.initial_status().map_err(ApiError::Core)?;
let item = state.repo.create_item(project_id, &initial_status, input).await?;
```

Each line can fail independently; `?` short-circuits the entire function on the first error. This is cleaner than nested `try/catch` blocks and equivalent in safety.

**The `move` keyword in async closures:**

```rust
tokio::spawn(async move {
    while let Ok(event) = rx.recv().await { ... }
});
```

`move` means the closure takes ownership of the variables it captures (`rx` in this case). This is required when the closure outlives the current function — which is always true for spawned tasks, since the spawned task may run after the function that spawned it has returned.
