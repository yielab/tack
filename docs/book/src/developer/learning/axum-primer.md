# Axum — HTTP Without Magic

Axum is the HTTP framework Tack uses for its API server. If you come from Express, FastAPI, or Spring MVC, the concepts map clearly — but the amount of "magic" is different in each case.

---

## What Axum is (and is not)

Axum is a Rust HTTP framework built on Tokio (async runtime) and Tower (middleware stack). Its job is routing requests to handler functions. That is roughly where it stops.

Compare the scope:

| Framework | Routing | DI container | Validation | ORM | Auth | Templating |
|-----------|---------|-------------|-----------|-----|------|-----------|
| Spring Boot | Yes | Yes (full) | Yes | Yes | Yes | Yes |
| FastAPI | Yes | Partial | Yes (Pydantic) | No | No | No |
| Express | Yes | No | No | No | No | No |
| **Axum** | Yes | No | No | No | No | No |

Axum is closer to Express in philosophy: it gives you routing and a composable middleware system, then gets out of the way. Everything else you compose yourself. In Tack:

- Database access: `sqlx` + the Repository pattern
- Validation: `validator` crate on request DTOs
- Auth: a custom `require_token` middleware
- JSON serialization: `serde_json`

There is no hidden dependency injection container. Shared state is passed explicitly.

---

## Routing

In Express:
```js
app.get('/api/projects', listProjects)
app.post('/api/projects', createProject)
app.get('/api/projects/:id', getProject)
app.patch('/api/projects/:id', updateProject)
app.delete('/api/projects/:id', deleteProject)
```

In Axum (from `crates/tack-api/src/router.rs`):
```rust
Router::new()
    .route("/projects",     get(list_projects).post(create_project))
    .route("/projects/{id}", get(get_project).patch(update_project).delete(delete_project))
```

The method functions (`get`, `post`, `patch`, `delete`) are Axum's equivalents of Express's `app.get`, `app.post`, etc. Multiple methods on the same path chain with `.method(handler)`.

Notice `/projects/{id}` uses curly braces — that is Axum's path parameter syntax. FastAPI also uses `{id}`; Express uses `:id`.

Routes are nested under `/api` using `.nest("/api", api_router)`. This is equivalent to Express's `app.use('/api', router)` or Spring's `@RequestMapping("/api")` on a controller class.

---

## Handlers and extractors

A handler is just an `async fn`. Its parameters are *extractors* — types that know how to pull information out of an incoming HTTP request.

```rust
// From crates/tack-api/src/handlers/items.rs

pub async fn create_item(
    State(state): State<AppState>,       // shared application state
    Path(project_id): Path<Uuid>,        // URL path parameter
    Json(input): Json<CreateItem>,       // request body, deserialized from JSON
) -> ApiResult<Json<serde_json::Value>> {
    // ...
}
```

Compare to the same handler in other frameworks:

**Express:**
```js
async function createItem(req, res) {
    const state = req.app.locals;           // State
    const project_id = req.params.id;      // Path
    const input = req.body;                // Json (after body-parser middleware)
}
```

**FastAPI:**
```python
async def create_item(
    project_id: UUID,                      # Path (from route)
    input: CreateItem,                     # Json (Pydantic model, auto-validated)
    db: Session = Depends(get_db),        # State (dependency injection)
):
```

**Spring MVC:**
```java
@PostMapping("/projects/{id}/items")
public ResponseEntity<Item> createItem(
    @PathVariable UUID projectId,          // Path
    @RequestBody CreateItem input,         // Json
    // State typically injected via @Autowired at class level
) { }
```

The key Axum extractors you will see throughout Tack:

| Extractor | What it extracts | Analogy |
|-----------|-----------------|---------|
| `State(state): State<AppState>` | Shared application state | Express `req.app.locals`, FastAPI `Depends(get_state)` |
| `Path(id): Path<Uuid>` | URL path segment, parsed | Express `req.params.id`, FastAPI path parameter |
| `Json(body): Json<CreateItem>` | Request body, deserialized from JSON | Express `req.body`, FastAPI `@RequestBody` |
| `Query(params): Query<ItemFilter>` | Query string, deserialized | Express `req.query`, FastAPI query parameters |

If extraction fails (e.g. invalid JSON, missing required path parameter, body exceeds size limit), Axum rejects the request with 400 or 422 automatically before your handler code runs.

---

## Responses

Handlers return `impl IntoResponse` — anything that implements the `IntoResponse` trait. The most common patterns:

```rust
// 200 OK with JSON body
Ok(Json(item))

// 201 Created with JSON body
Ok((StatusCode::CREATED, Json(item)))

// 204 No Content
Ok(StatusCode::NO_CONTENT)

// Error (handled by ApiError's IntoResponse implementation)
Err(ApiError::NotFound("Item 42 not found".into()))
```

`ApiResult<T>` is a type alias for `Result<T, ApiError>`. The `ApiError` type implements `IntoResponse`, which maps each error variant to the correct HTTP status:

```rust
// From crates/tack-api/src/error.rs

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::NotFound(msg)  => (StatusCode::NOT_FOUND, msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::Core(err) => match err {
                CoreError::ItemNotFound(_)       => (StatusCode::NOT_FOUND, ...),
                CoreError::InvalidTransition {..} => (StatusCode::BAD_REQUEST, ...),
                CoreError::WipLimitExceeded {..}  => (StatusCode::BAD_REQUEST, ...),
                // ...
            },
            ApiError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, ...),
        };

        let body = json!({ "error": { "status": status.as_u16(), "message": message } });
        (status, axum::Json(body)).into_response()
    }
}
```

This is the pattern that makes handlers clean: each handler returns `?` on every fallible call, and the single `IntoResponse` impl handles the translation from domain errors to HTTP status codes.

---

## Shared state — AppState

```rust
// From crates/tack-api/src/router.rs

#[derive(Clone)]
pub struct AppState {
    pub repo: Repository,
    pub config: AppConfig,
    pub workspace_id: Uuid,
    pub broadcast_tx: broadcast::Sender<BoardEvent>,
}
```

`AppState` is the equivalent of Express's `app.locals` (or a FastAPI `app.state`). It contains everything handlers need that is not in the request itself: the database connection pool, application config, and the WebSocket broadcast sender.

It is registered once at startup and then cloned into every handler call:

```rust
// main.rs — wire up state at startup:
let state = AppState { repo, config, workspace_id, broadcast_tx };
let app = build_router(state);

// build_router — attach state to the router:
outer.with_state(state)
```

Because `AppState` derives `Clone`, Axum clones it cheaply for each request. `SqlitePool` and `broadcast::Sender` are internally reference-counted, so cloning them is cheap (just incrementing a reference count) — they do not copy the underlying connection pool or channel.

---

## Middleware

Axum uses Tower's middleware model: a stack of layers wrapping the router. Conceptually identical to Express middleware, Spring `HandlerInterceptor`, or FastAPI middleware — code that runs on every request before and/or after the handler.

Tack's middleware stack (bottom of `build_router` in `router.rs`):

```rust
outer
    .layer(DefaultBodyLimit::max(config.max_body_size_bytes))
    .layer(SetResponseHeaderLayer::overriding(/* security headers */))
    .layer(cors)
    .layer(TraceLayer::new_for_http().make_span_with(|req| {
        tracing::info_span!(
            "http_request",
            method = %req.method(),
            uri = %req.uri(),
        )
    }))
    .with_state(state)
```

Layers apply from bottom to top (the last `.layer()` call runs first on each request). In order of execution:

1. `TraceLayer` — creates a tracing span per request (structured logging + timing)
2. `CorsLayer` — handles CORS headers and preflight OPTIONS requests
3. `SetResponseHeaderLayer` — appends security headers (`X-Frame-Options`, `X-Content-Type-Options`, etc.)
4. `DefaultBodyLimit` — rejects request bodies exceeding `max_body_size_bytes`

The auth middleware (`require_token`) is applied specifically to the `/api` sub-router before the global layers, so the health/debug endpoints remain public:

```rust
let api = Router::new()
    .route("/health", get(debug::health))
    // ...all protected routes...
    .layer(middleware::from_fn_with_state(state.clone(), require_token));
```

`middleware::from_fn_with_state` creates a Tower middleware from a plain async function, with access to `AppState`. The `require_token` function reads the `Authorization: Bearer <token>` header and returns 401 if it is missing or wrong.

---

## Putting it together — a full request lifecycle

Here is what happens when `PATCH /api/items/{id}` is called:

1. **TraceLayer** creates a span: `http_request{method=PATCH, uri=/api/items/abc-123}`
2. **CorsLayer** checks the `Origin` header; adds CORS response headers
3. **DefaultBodyLimit** checks the body size; rejects if over limit
4. **require_token** checks `Authorization: Bearer ...`; returns 401 if invalid
5. **Axum router** matches `/api/items/{id}` → `patch(items::update_item)`
6. **Extractors** run: `State` clones AppState; `Path` parses the UUID; `Json` deserializes the body into `UpdateItem`; if any extractor fails, request is rejected before reaching handler
7. **update_item handler** runs: fetches old item, validates workflow transition, checks WIP limit, calls `repo.update_item()`, broadcasts WebSocket event, triggers parent auto-complete
8. Handler returns `Ok(Json(item))` → Axum serializes to JSON, sets `Content-Type: application/json`, returns 200
9. If any step returned `Err(ApiError::...)`, **IntoResponse** converts it to the appropriate status + JSON error body
