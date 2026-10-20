# Construir Tack desde cero

Una guía para quien quiere aprender arquitectura, Rust e ingeniería con IA construyendo un
programa real. El programa es Tack. La guía no describe cómo se construyó Tack (eso está en el
historial y tuvo sus vueltas); describe **cómo se construiría hoy, sabiendo adónde se quiere
llegar**, en el orden en que cada pieza tiene ya debajo lo que necesita.

Acompaña a la [guía de la Fase 79](phase-79-guia.md), que explica cómo llevar el Tack de hoy a
esta misma arquitectura. Si algún concepto (puerto, adaptador, raíz de composición, suite de
contrato) no te suena, la Parte I de esa guía lo explica; aquí se usa sin repetirlo.

---

## Cómo leer esta guía

Cada etapa tiene la misma forma:

- **Qué se construye** y para qué sirve al producto.
- **Por capas, de adentro hacia afuera:** dominio → persistencia → aplicación → HTTP → binario
  → interfaz. Siempre en ese orden, porque cada capa solo depende de las que ya existen.
- **Las pruebas de cada capa**, escritas en el mismo paso que el código, nunca después.
- **Hecho cuando:** comandos que dicen si la etapa terminó. Si no hay comando, no hay "hecho".
- **Por qué así:** la decisión de diseño y lo que pasa si se hace de otra forma. Muchas veces el
  "otra forma" es lo que Tack hizo de verdad y luego tuvo que corregir.

No saltes etapas. La tentación de empezar por la parte divertida (los agentes de IA) es fuerte,
pero un runner sin un tablero sólido debajo es un runner que no sabe qué ejecutar.

---

## Parte A — El destino

### A.1 Qué hace Tack cuando está terminado

Tack es un gestor de proyectos en un solo binario que, además, puede delegar tareas a agentes
de IA con control humano y trazabilidad. Sus funcionalidades se agrupan en tres **contextos**
(zonas del producto con su propio vocabulario y sus propias reglas):

| Contexto | Qué hace | Funcionalidades |
|---|---|---|
| **Planificación** | Organizar el trabajo | Proyectos con tipo (software, construcción, personal…); un modelo único de ítem (épica, tarea, bug… son el mismo `Item` con distinto `item_type`); workflows con columnas, límites WIP y transiciones (presets scrum, kanban, simple, construcción); vocabulario propio por proyecto; dependencias sin ciclos; sprints; roles; comentarios; adjuntos; campos personalizados; brief del ítem (criterios de aceptación, restricciones, riesgo); recurrencia; búsqueda de texto completo; vistas (tablero, lista, tabla, calendario, línea de tiempo, sprints, panel); plantillas; el proyecto entero en un archivo (importar y exportar) |
| **Ejecución** | Mandar una tarea a un agente | Perfiles de agente; runners que se enrolan con un token de un solo uso; flotas; un planificador que elige runner; solicitudes e intentos con lease y fencing; eventos en vivo; decisiones (el agente pregunta, la persona responde); cancelación; recuperación tras un corte; límites en tokens; uso medido por modelo; el runner como proceso aparte, con sus propias credenciales; adaptadores para Claude Code, Codex, OpenCode y docket |
| **Entrega** | Llevar lo que hizo el agente hasta el código | Evidencia del cambio; un verificador externo que escribe un paquete de "listo para merge" (MRP); revisión humana; niveles de autonomía con un techo por equipo; merge automático y degradación; ramas y pull requests; arranque automático de tareas |

Y alrededor, los **bordes**: CLI, servidor MCP para que otras IA manejen el tablero, importación
desde GitHub y Linear, sincronización con GitHub, copias de seguridad locales y en S3, webhooks,
tiempo real por websocket, app de escritorio que supervisa el servicio, instalación como
servicio del sistema.

### A.2 La arquitectura a la que se llega

Nueve crates, cada uno con un motivo para existir (ver los tres criterios en la guía de la
Fase 79, Parte I §3):

```
                         ┌──────────────┐
                         │ tack-protocol│  contrato servidor↔runner; no depende de nada
                         └──────┬───────┘
              ┌─────────────────┼─────────────────────┐
              ▼                 ▼                     ▼
        ┌───────────┐     ┌───────────┐         ┌────────────┐
        │ tack-core │     │ tack-orch │         │ tack-runner│  proceso aparte
        │ (dominio) │◄────│(scheduler,│         │ (puertos + │
        └─────┬─────┘     │ jobs)     │         │ adaptadores│
              │           └─────┬─────┘         │ de agentes)│
              ▼                 │               └────────────┘
        ┌───────────┐           │
        │  tack-db  │◄──────────┤
        │ (SQLite)  │           │
        └─────┬─────┘           │
              ▼                 ▼
        ┌──────────────────────────┐
        │        tack-app          │  casos de uso + ticks + puertos
        └────────────┬─────────────┘
                     ▼
        ┌──────────────────────────┐
        │        tack-api          │  HTTP: rutas, auth, DTOs, OpenAPI
        └────────────┬─────────────┘
                     ▼
        ┌──────────────────────────┐
        │   tack-cli  (binario)    │  raíz de composición: `tack serve`, CLI, MCP
        └──────────────────────────┘
   tack-test-support: fakes, semillas, suites de contrato (solo en dev-dependencies)
   tack-desktop: Tauri, fuera del workspace (arrastra GTK/WebKit)
```

La regla que lo sostiene todo: **el servidor y el runner solo comparten `tack-protocol`**. El
runner nunca compila una base de datos.

### A.3 Los cuatro flujos que tienen que funcionar al final

1. **Planificar:** crear un proyecto → crear ítems → moverlos por columnas respetando el
   workflow → ver el cambio en tiempo real en otra pestaña.
2. **Ejecutar:** "Run with agent" sobre un ítem → se encola una solicitud → el planificador
   elige runner → el runner reclama con lease → prepara una copia del repo → lanza el agente →
   los eventos llegan a la UI → el agente pregunta → la persona responde → el agente termina.
3. **Entregar:** el runner calcula la evidencia → el verificador escribe el MRP → la persona lo
   revisa → según la autonomía del proyecto, se hace merge solo o se espera.
4. **Recuperarse:** el runner muere a mitad de un intento → al volver, lee su journal → el
   servidor decide si reencolar o pedir intervención; nada se ejecuta dos veces.

Cada etapa de esta guía hace funcionar un trozo de uno de estos flujos, de punta a punta.

---

## Parte B — Principios que no se negocian

Antes de la primera línea, estas reglas. Cada una existe porque romperla cuesta caro después.

| Principio | En la práctica | Lo que pasa si no |
|---|---|---|
| Las dependencias apuntan hacia adentro | El dominio no conoce SQLite ni HTTP | Una regla de negocio solo se prueba levantando un servidor |
| Una regla que no se verifica es un deseo | Script del grafo y `cargo deny` desde el día uno | El grafo deriva en silencio (Tack: `orch → db`) |
| Una sola fuente de verdad por concepto | Un `enum` de estados, un `Clock`, un tipo por mensaje | Tres copias de la máquina de estados que divergen |
| Una abstracción se gana con una prueba | Un trait solo si un fake lo usa | Ceremonia sin beneficio |
| Lecturas y comandos son distintos | `queries/` para listados, `repo/` para cambios | Entidades de dominio cargadas para pintar una tabla |
| El error de infraestructura no cruza capas | `StoreError` → `AppError` → `ApiError` | El handler HTTP necesita saber cómo falla SQLite |
| Lo generado se genera | `openapi.json` desde el código; cliente TS desde `openapi.json` | Dos descripciones de la API que no coinciden |
| Medir en tokens, no en dólares | Los límites y el uso se cuentan en tokens | Precios que cambian y cifras inventadas |
| Nada se ejecuta dos veces | Claves de idempotencia y fencing tokens en todo lo que muta estado de ejecución | Un reintento de red lanza dos agentes |
| Los tests van con el código | `foo.rs` + `foo/tests.rs`, en el mismo commit | Pruebas viejas y nuevas conviviendo |

---

## Etapa 0 — El taller

**Qué se construye.** Ningún código del producto. El workspace, las herramientas y las reglas
que vigilan todo lo que venga después. Es la etapa que todo el mundo se salta, y por eso es la
primera.

### 0.1 El workspace

```toml
# Cargo.toml (raíz)
[workspace]
resolver = "2"
members = [
    "crates/tack-protocol",
    "crates/tack-core",
    "crates/tack-db",
    "crates/tack-orch",
    "crates/tack-app",
    "crates/tack-api",
    "crates/tack-cli",
    "crates/tack-runner",
    "crates/tack-test-support",
]
exclude = ["crates/tack-desktop"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.94"
license = "MIT"

[workspace.lints.clippy]
too_many_lines = "warn"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "2"
# ...el resto se añade en la etapa que lo necesita, no antes
```

Crea los nueve crates vacíos ahora (`cargo new --lib crates/tack-protocol`, etc.), aunque la
mayoría no tenga nada todavía. Así el grafo existe desde el principio y el script del 0.3 tiene
algo que vigilar.

**Por qué `[workspace.dependencies]`:** una sola versión de cada dependencia para todo el
workspace. Si dos crates piden `tokio` en versiones distintas, compilas dos `tokio`.

**Por qué `rust-toolchain.toml`:** fija la versión del compilador para todos. Sin él, "en mi
máquina compila" es literal.

### 0.2 Las herramientas

| Herramienta | Para qué | Desde cuándo |
|---|---|---|
| `cargo nextest` | Correr las pruebas, con salida corta y filtersets (`rdeps(crate)`) | Ahora; nunca `cargo test` |
| `cargo clippy -D warnings` | Lints como errores | Ahora |
| `cargo fmt --check` | Formato | Ahora |
| `cargo deny` | Licencias, advertencias de seguridad y qué crate puede depender de qué | Ahora |
| `cargo machete` | Dependencias declaradas que nadie usa | Ahora |
| `ast-grep` | Buscar y reemplazar entendiendo la sintaxis de Rust | Cuando haga falta |

### 0.3 El grafo vigilado

El script `scripts/check-crate-graph.sh` de la Fase 79 (está completo en el plan), con la
**tabla final** de §A.2 desde el primer día. Cualquier `path` dependency que no esté en la tabla
hace fallar el push.

Y en `deny.toml`:

```toml
[[bans.deny]]
crate = "sqlx"
wrappers = ["tack-db", "tack-test-support"]

[[bans.deny]]
crate = "axum"
wrappers = ["tack-api", "tack-cli"]
```

Esto dice: `sqlx` solo puede aparecer como dependencia de `tack-db` y `tack-test-support`;
`axum`, solo de `tack-api` y `tack-cli`. Si mañana alguien escribe SQL en un handler, no
compila el `cargo deny`.

### 0.4 La compuerta

`.githooks/pre-push`, activado con `git config core.hooksPath .githooks`:

```bash
#!/usr/bin/env bash
set -euo pipefail
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
scripts/check-crate-graph.sh
cargo deny check
cargo nextest run --workspace
```

Al principio corre en segundos. Cuando la suite crezca, se partirá: lo rápido en el hook, lo
completo en CI. Pero la regla no cambia: **lo que el hook no deja pasar no sale de la máquina**.

**Hecho cuando:**

```bash
cargo build --workspace && scripts/check-crate-graph.sh && cargo deny check && .githooks/pre-push
```

Todo sale con 0, con nueve crates vacíos.

---

## Etapa 1 — El esqueleto andante

**Qué se construye.** Un proyecto y un ítem, de punta a punta: se crean por HTTP, se guardan en
SQLite, se ven en el navegador. Nada más. Ni workflow, ni sprints, ni agentes.

**Por qué empezar así.** Esta técnica se llama *walking skeleton*: la versión más delgada posible
que atraviesa **todas** las capas. Su valor no es la funcionalidad (crear un ítem es trivial),
sino que obliga a tomar todas las decisiones de estructura con un caso pequeño: cómo viaja un
error desde SQLite hasta HTTP, dónde se genera el id, quién conoce la hora, cómo se genera el
cliente del frontend. Tomarlas con un ítem es barato; tomarlas con cuarenta tablas es imposible.

### 1.1 Dominio — `tack-core`

```rust
// crates/tack-core/src/planning/item.rs
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemId(pub Uuid);

#[derive(Debug, Clone)]
pub struct Item {
    pub id: ItemId,
    pub project_id: ProjectId,
    pub title: Title,
    pub item_type: ItemType,
    pub status: String,           // la columna; el workflow (etapa 2) decide cuáles valen
    pub created_at: DateTime<Utc>,
}

/// Un título no vacío y de longitud razonable. Si existe un `Title`, es válido.
#[derive(Debug, Clone)]
pub struct Title(String);

impl Title {
    pub fn parse(raw: &str) -> Result<Self, CoreError> {
        let t = raw.trim();
        if t.is_empty() || t.len() > 500 {
            return Err(CoreError::InvalidTitle);
        }
        Ok(Self(t.to_owned()))
    }
}
```

Tres ideas de Rust y de diseño en estas líneas:

- **Newtypes para ids** (`ItemId(Uuid)`, `ProjectId(Uuid)`). El compilador impide pasar el id de
  un proyecto donde se espera el de un ítem. Cuesta una línea y evita una clase entera de bugs.
- **"Parse, don't validate".** `Title::parse` devuelve un `Title` o un error. Una vez tienes un
  `Title`, sabes que es válido; no hay que volver a comprobarlo en cada capa. La validación
  ocurre una vez, en el borde.
- **El dominio no genera ids ni lee la hora.** Recibe `id` y `created_at` como argumentos. Así las
  pruebas son deterministas. Quien genera el id y lee el reloj es la capa de aplicación.

`tack-core` no depende de `serde` todavía. **El tipo de dominio no es el JSON de la API.** Si
mañana cambias el nombre de un campo en el JSON, el dominio no se entera, y al revés.

**Pruebas de esta capa:** unitarias puras, en `item/tests.rs`. `Title::parse("")` falla,
`Title::parse("  x  ")` da `"x"`. Milisegundos, sin I/O.

### 1.2 Persistencia — `tack-db`

```rust
// crates/tack-db/src/lib.rs
pub async fn init_pool(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(url)?
        .foreign_keys(true)              // por conexión: hay que ponerlo en las opciones
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    SqlitePoolOptions::new().max_connections(5).connect_with(options).await
}
```

Tres decisiones que conviene entender:

- **`foreign_keys(true)` en las opciones de conexión, no con un `PRAGMA` suelto.** En SQLite es
  un ajuste *por conexión*. Un `PRAGMA` ejecutado una vez solo arma la primera conexión del pool;
  las otras cuatro aceptarían filas huérfanas. Tack lo aprendió así.
- **WAL** permite lectores concurrentes con un escritor. Es lo correcto para un servidor.
- **Migraciones con checksum y orden estricto.** Cada migración tiene un nombre `NNN_descripcion`,
  se aplica en una transacción y se registra con su hash. Si alguien edita una migración ya
  aplicada, el arranque falla en vez de dejar la base en un estado desconocido. Escríbelas como
  archivos `.sql` en `crates/tack-db/migrations/` desde el principio; son más fáciles de revisar
  que cadenas dentro de Rust.

El error propio:

```rust
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}
```

Y el repositorio, organizado por **agregado** (un agregado es un grupo de datos que cambia junto:
un proyecto con su configuración; un ítem con sus etiquetas):

```
crates/tack-db/src/
├── lib.rs           init_pool, migraciones, StoreError
├── repo/
│   ├── projects.rs  comandos: create_project, update_project
│   └── items.rs     comandos: create_item, move_item
└── queries/
    └── items.rs     lecturas: items_of_project, item_by_id
```

**La conversión fila → dominio vive aquí.** `tack-db` tiene sus structs `#[derive(sqlx::FromRow)]`
y los convierte a `tack_core::Item`. El dominio no sabe que existe `FromRow`.

**Pruebas de esta capa:** contra SQLite en memoria (`sqlite::memory:`), con las migraciones
aplicadas. Un helper en `tack-test-support` (`setup_test_db()`) lo prepara. Una prueba por
comando y por consulta.

### 1.3 Aplicación — `tack-app`

```rust
// crates/tack-app/src/planning/create_item.rs
pub async fn create_item(
    repo: &Repository,
    clock: &impl Clock,
    project_id: ProjectId,
    title: &str,
    item_type: ItemType,
) -> Result<Item, AppError> {
    let title = Title::parse(title)?;                       // dominio valida
    let project = repo.project(project_id).await?;          // existe o NotFound
    let status = project.workflow.initial_status().to_owned();
    let item = Item {
        id: ItemId(Uuid::new_v4()),                          // la aplicación genera el id
        project_id, title, item_type, status,
        created_at: clock.now(),                             // y lee el reloj
    };
    repo.create_item(&item).await?;
    Ok(item)
}
```

Fíjate en qué hace cada capa: el **dominio** dice si el título vale; la **persistencia** guarda;
la **aplicación** orquesta, genera el id, lee la hora. Ninguna sabe de HTTP.

¿Por qué `&Repository` concreto y no un trait? Porque para planificación no hay ninguna prueba
que necesite un fake: probar contra SQLite en memoria es rápido y más fiel. El trait llegará en
la etapa 4, cuando el planificador de ejecución necesite simular cientos de escenarios de
concurrencia. **No antes.**

`AppError` agrupa los errores con significado de negocio:

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Store(StoreError),
}
impl From<CoreError> for AppError { /* InvalidTitle → Invalid(...) */ }
impl From<StoreError> for AppError { /* NotFound → NotFound, el resto → Store */ }
```

**Pruebas de esta capa:** contra SQLite en memoria y un `FixedClock`. Se prueba la orquestación:
que el ítem nace en la columna inicial del workflow, que un proyecto inexistente da `NotFound`.

### 1.4 HTTP — `tack-api`

```rust
// crates/tack-api/src/routes/items.rs
#[derive(Deserialize, ToSchema)]
pub struct CreateItemBody { pub title: String, pub item_type: ItemTypeDto }

#[derive(Serialize, ToSchema)]
pub struct ItemDto { pub id: Uuid, pub title: String, pub status: String, /* ... */ }

#[utoipa::path(post, path = "/api/projects/{id}/items", request_body = CreateItemBody,
               responses((status = 201, body = ItemDto)))]
pub async fn create(
    State(s): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(body): Json<CreateItemBody>,
) -> Result<(StatusCode, Json<ItemDto>), ApiError> {
    let item = tack_app::planning::create_item(
        &s.repo, &s.clock, ProjectId(project_id), &body.title, body.item_type.into(),
    ).await?;
    Ok((StatusCode::CREATED, Json(item.into())))
}
```

El handler hace **tres cosas y nada más**: extraer, llamar, responder. Si un handler crece más
de veinte líneas, tiene una regla dentro que pertenece a `tack-app`.

Los **DTOs** (`CreateItemBody`, `ItemDto`) viven aquí, no en el dominio. Llevan `serde` y
`utoipa::ToSchema`. La conversión `Item → ItemDto` es explícita (`impl From<Item> for ItemDto`).

`ApiError` traduce:

```rust
impl From<AppError> for ApiError { /* NotFound → 404, Invalid → 422, Conflict → 409, Store → 500 */ }
```

Y responde siempre con **un sobre de error estable**: `{"error": {"code": "not_found", "message":
"...", "request_id": "..."}}`. El `code` es una cadena de un conjunto cerrado y documentado; el
`message` es para humanos y puede cambiar. Un cliente decide por `code`, nunca por `message`.

**OpenAPI generado.** `utoipa` lee los `#[utoipa::path]` y los `ToSchema` y produce
`docs/openapi.json`. Un script lo regenera; CI falla si el archivo commiteado no coincide con el
generado. Nadie escribe ese JSON a mano.

**Pruebas de esta capa:** de integración HTTP, con `tower::ServiceExt::oneshot` sobre el router
(sin abrir un puerto). Se prueba solo lo HTTP: que devuelve 201, que un cuerpo sin `title` da
422, que el sobre de error tiene la forma correcta. La regla "el ítem nace en la columna inicial"
**no** se vuelve a probar aquí; ya la probó `tack-app`.

### 1.5 Composición — `tack-cli`

```rust
// crates/tack-cli/src/serve.rs
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let pool = tack_db::init_pool(&config.database_url).await?;
    tack_db::migrate(&pool).await?;
    let state = AppState {
        repo: Repository::new(pool),
        clock: SystemClock,
        config: config.api.clone(),
    };
    let listener = TcpListener::bind(config.bind).await?;
    axum::serve(listener, tack_api::router(state)).await?;
    Ok(())
}
```

Esta es la **raíz de composición**: el único lugar que sabe qué implementación concreta va en
cada sitio (`SystemClock` y no `FixedClock`, SQLite y no un fake). Cuando lleguen los trabajos de
fondo (etapa 4), sus `tokio::spawn` también vivirán aquí, nunca dentro de `tack-api`.

`tack` sin argumentos = `tack serve`. El producto es la interfaz web; la CLI es secundaria.

**Configuración:** un `tack.toml` opcional y variables `TACK_*` que lo sobrescriben, cargados
**una vez** aquí y pasados como valores. Ningún crate de adentro lee variables de entorno.

### 1.6 Interfaz — `frontend/`

SolidJS + Vite + TypeScript. Desde el primer día:

```
frontend/src/
├── app/          App, Layout, rutas
├── features/
│   └── board/    la vista de tablero (todavía una columna)
└── shared/
    ├── api/      schema.gen.ts (generado desde openapi.json) + funciones finas
    └── ui/       botones, modales, tokens de diseño
```

- **El cliente se genera** de `docs/openapi.json` (`openapi-typescript` o similar) a
  `shared/api/schema.gen.ts`. Si el backend cambia un campo, el frontend deja de compilar.
- **Regla de dirección:** `features/*` importa de `shared/*`, nunca de otra feature; `shared/*`
  nunca importa de `features/*`. Un test (`architecture.test.ts`) recorre los archivos y lo
  comprueba en ambas direcciones, incluidos los alias `@/`.
- **Tokens de diseño desde el principio:** todos los colores son variables CSS (`--color-*`);
  ningún componente usa hex directo. Cambiar de tema o de paleta después cuesta un archivo, no
  cien.

**Pruebas:** Vitest para componentes y lógica; un Playwright e2e que crea un ítem y lo ve en el
tablero.

**Hecho cuando (etapa 1):**

```bash
cargo nextest run --workspace                    # dominio, store, app, HTTP
cd frontend && npm test && npx playwright test   # componentes + un e2e
scripts/check-crate-graph.sh && cargo deny check
```

Y a mano: `tack serve`, abrir el navegador, crear un ítem, recargar, sigue ahí.

---

## Etapa 2 — Planificación completa

**Qué se construye.** Todo el contexto de planificación sobre el esqueleto. Cada funcionalidad
recorre las mismas capas que el ítem de la etapa 1, en el mismo orden. Lo que cambia aquí es el
**orden entre funcionalidades**: cada una necesita que existan las anteriores.

| Orden | Funcionalidad | Dominio (`tack-core`) | Lo que hay que saber |
|---|---|---|---|
| 2.1 | **Workflow** | `WorkflowConfig { columns, transitions }`, `validate_transition(from, to)`, `check_wip(column, count)`, presets `scrum()`, `kanban()`, `simple()`, `construction()` | Es el corazón: todo movimiento de ítem pasa por aquí. Función pura, probada exhaustivamente |
| 2.2 | **Tipos de proyecto y vocabulario** | `ProjectType` elige workflow y `VocabularyMap` por defecto (`"task" → "Work Order"`) | El vocabulario es presentación: el dominio usa siempre los términos canónicos; la UI traduce |
| 2.3 | **Mover ítems** | `move_item` usa 2.1 | Lee y escribe → transacción `BEGIN IMMEDIATE` en el store (ver nota abajo) |
| 2.4 | **Jerarquía y dependencias** | `parent_id`; `DependencyGraph` con detección de ciclos (DFS) | Validar **antes** de insertar la arista; el grafo se construye desde las filas del proyecto |
| 2.5 | **Sprints, roles, comentarios** | Entidades simples | CRUD por las capas; nada nuevo de diseño |
| 2.6 | **Brief del ítem** | `ItemBrief { acceptance_criteria, constraints, definition_of_done, risk }` | Es lo que el agente leerá en la etapa 4. Hazlo bien tipado aquí |
| 2.7 | **Campos personalizados** | `CustomFieldDefinition` + valores tipados | Validar el valor contra el tipo del campo en el dominio |
| 2.8 | **Adjuntos** | Metadatos en la base; bytes en disco | Puerto `BlobStore` (filesystem hoy, quizá S3 mañana): aquí **sí** hay un trait, porque hay dos implementaciones reales |
| 2.9 | **Búsqueda** | — | FTS5 de SQLite con triggers que mantienen el índice. Es una lectura: vive en `queries/search.rs` |
| 2.10 | **Recurrencia** | `Recurrence::next_due(after)` pura | Cerrar una tarea recurrente crea la siguiente en el **mismo** comando del store |
| 2.11 | **Tiempo real** | — | Un `broadcast::Sender<BoardEvent>` en `tack-app`; cada caso de uso que cambia el tablero publica **después** de que la transacción se confirme. El websocket de `tack-api` solo reenvía |
| 2.12 | **Vistas** (lista, tabla, calendario, línea de tiempo, sprints, panel) | — | Todas son lecturas del mismo conjunto de ítems. **Un solo modelo de filtro y orden** en `shared/` del frontend, usado por las seis vistas |
| 2.13 | **El proyecto en un archivo** | `outline::Bundle` (tipos), `check(bundle, board) → Plan`, `to_bundle(board)` | Importar es: parsear → `check` puro que dice qué se crea, qué se cambia y qué se borra → aplicar en **una** transacción. Exportar es el inverso. Una plantilla es el mismo archivo sin ítems |

**Nota sobre `BEGIN IMMEDIATE` (2.3).** En SQLite, si dos transacciones empiezan leyendo y luego
quieren escribir, ambas piden "subir" de lectora a escritora y una recibe `SQLITE_LOCKED`.
`BEGIN IMMEDIATE` pide el lock de escritura desde el principio. Regla: **todo comando que lee y
luego escribe empieza con `BEGIN IMMEDIATE`**. Y una prueba de concurrencia de esto debe correr
contra un archivo, no en memoria: la base en memoria compartida puede ocultar la carrera.
Comprueba que la prueba es útil quitando el `IMMEDIATE` y viéndola fallar.

**Nota sobre la procedencia (2.x).** Cada ítem lleva `source` (quién lo creó: la UI, una
importación, un agente) y nunca cambia. Cuando un agente lea ítems en la etapa 4, un ítem de
origen no confiable se marca como tal en lo que el agente recibe. Es la frontera contra la
inyección de instrucciones. Ponla ahora, cuando es una columna; añadirla después es una
migración sobre datos ya mezclados.

**Hecho cuando (etapa 2):** el flujo 1 de §A.3 funciona entero, incluida una segunda pestaña que
ve los cambios sin recargar; exportar un proyecto e importarlo en otra base da el mismo tablero
(`diff` de los dos exports = vacío).

---

## Etapa 3 — El contrato de ejecución (`tack-protocol`)

**Qué se construye.** Solo tipos y funciones puras. El idioma que hablarán el servidor y el
runner. Ni una tabla, ni una ruta todavía.

**Por qué antes que el servidor y que el runner.** Porque es lo que ambos comparten, y lo que se
comparte tiene que existir antes que cualquiera de las partes. Y porque si lo escribes dentro de
una de las dos partes, la otra acaba dependiendo de todo lo que esa parte arrastra (es
exactamente el error que la Fase 79 corrige en Tack).

### 3.1 Los estados y sus transiciones

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Queued, Leased, Preparing, Running, WaitingDecision,
    Succeeded, Failed, Cancelled, Lost, NeedsOperator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actor { Scheduler, Operator, LeaseOwner, RecoveryService }

pub fn validate_transition(from: ExecutionState, to: ExecutionState, by: Actor)
    -> Result<(), LifecycleError>
{
    use ExecutionState::*; use Actor::*;
    let ok = matches!((from, to, by),
        (Queued, Leased, Scheduler)
      | (Queued, Cancelled, Operator)
      | (Leased, Preparing | Failed | Cancelled, LeaseOwner)
      | (Preparing, Running | Failed | Cancelled, LeaseOwner)
      | (Running, WaitingDecision | Succeeded | Failed | Cancelled, LeaseOwner)
      | (WaitingDecision, Running | Cancelled, LeaseOwner)
      | (Leased | Preparing | Running, Lost | NeedsOperator, RecoveryService)
      // ...
    );
    if ok { Ok(()) } else { Err(LifecycleError { from, to, by }) }
}

impl ExecutionState {
    pub fn as_str(self) -> &'static str { /* "queued", ... */ }
}
impl FromStr for ExecutionState { /* el inverso */ }
```

Esta es **la única** máquina de estados del programa. `tack-db` la usa para guardar
(`as_str`/`from_str`, en un solo sitio), `tack-app` para validar, el runner para saber qué
puede reportar. Si alguien añade un estado, el compilador lista cada `match` que no lo contempla.

**Quién puede pedir cada transición** (`Actor`) es tan importante como la transición misma: el
runner que tiene la lease puede pasar de `Running` a `Succeeded`; el operador no. Eso evita que
una petición mal dirigida cierre un intento ajeno.

### 3.2 Los mensajes

Un tipo por mensaje del protocolo: `EnrollRequest`, `ClaimResponse`, `HeartbeatRequest`,
`EventBatch`, `DecisionRequest`, `CompletionRequest`… Cada mensaje que muta un intento lleva
**tres cosas**: el id del runner, el id del intento y el **fencing token** actual.

**Qué es un fencing token.** Cuando el planificador entrega una lease, le asigna un número que
crece (1, 2, 3…). Si un runner se cuelga, su lease caduca y el intento se entrega a otro con el
token 2. Si el primer runner despierta y manda un evento con el token 1, el servidor lo rechaza
con `stale_lease` y **no escribe nada**. Sin esto, dos runners podrían escribir en el mismo
intento.

### 3.3 Capacidades

`RunnerCapabilities` describe qué sabe hacer un runner: qué agentes tiene, si su cancelación es
`Supported` o `Advisory`, cuántos intentos concurrentes acepta. El planificador lee esto; nunca
supone.

**Capacidades honestas:** un adaptador que declara `Supported` en cancelación tiene que poder
demostrarlo (por ejemplo, anunciando el grupo de procesos de sus herramientas). Si no, se declara
`Advisory`. Es mejor que el planificador sepa que la cancelación es "best effort" a que crea que
es segura.

### 3.4 Errores estables

Un conjunto cerrado de códigos (`not_found`, `conflict`, `idempotency_conflict`,
`invalid_transition`, `stale_lease`, `runner_revoked`…), cada uno con un `retryable` derivado
del código, nunca puesto a mano.

### 3.5 Fixtures de contrato

Por cada mensaje, un JSON de ejemplo en `docs/contracts/runner-v1/`. Una prueba en
`tack-protocol/tests/` deserializa cada fixture y la vuelve a serializar: debe dar lo mismo.
Estas fixtures son **el contrato**: si un cambio las rompe, rompe a todos los runners
desplegados.

### 3.6 Lo que también va aquí

`Clock` (el único del servidor), `Measurement<T>` (un valor con su fuente: `Measured`,
`Estimated`, `NotMeasured`; nunca un `0` inventado), el paquete MRP (tipos y su render a
Markdown) y el plan de `prepare` (qué comando instala dependencias según el lockfile). Todo puro.

`utoipa` detrás del feature `openapi`, para que `tack-api` documente estos tipos sin copiarlos
y el runner no compile `utoipa`.

**Hecho cuando (etapa 3):**

```bash
cargo nextest run --workspace -E 'rdeps(tack-protocol)'
cargo tree -p tack-protocol -e normal --prefix none | grep -c '^tack-'   # 0: no depende de nada nuestro
```

---

## Etapa 4 — Ejecución, del lado del servidor

**Qué se construye.** Encolar, planificar, conceder leases, recibir eventos, guardar decisiones.
Al final de la etapa, un runner falso (un test) puede hacer el ciclo completo por HTTP.

### 4.1 Dominio — `tack-core::runs` y `tack-orch`

- `run_defaults::resolve(item, roles, profiles) → Resolved`: qué perfil de agente le toca a un
  run y por qué. Pura.
- `tack-orch::scheduler::select(candidates, request) → Selected | NoneQualify(reasons)`: dado un
  conjunto de runners con su salud, capacidad, etiquetas y capacidades, elige uno o explica por
  qué ninguno sirve. Pura. **No concede la lease:** eso es trabajo del store, atómico.
- `batch::schedule(requests)`: orden por prioridad y luego FIFO, para que nada espere para
  siempre.
- Límites en tokens: `limit_for(profile, history)` (pura).

### 4.2 Persistencia — `tack-db`

Las tablas: `agent_profiles`, `runners`, `runner_fleets`, `enrollment_tokens`,
`execution_requests`, `execution_attempts`, `execution_events`, `execution_decisions`,
`execution_artifacts`.

El comando crítico es **claim con fencing**, en una sola transacción:

```sql
BEGIN IMMEDIATE;
-- elegir el intento: lo decidió el scheduler puro con un snapshot leído antes
UPDATE execution_attempts
   SET state = 'leased', runner_id = ?1, fencing_token = fencing_token + 1,
       lease_expires_at = ?2
 WHERE id = ?3 AND state = 'queued'
RETURNING fencing_token;
-- si no devolvió fila, otro runner se lo llevó: se responde "nada que hacer"
COMMIT;
```

El `WHERE state = 'queued'` es lo que lo hace seguro: si dos runners compiten, uno actualiza la
fila y el otro no encuentra nada. No hace falta un lock externo.

**El estado de la solicitud no se copia en otra tabla.** Se deriva del último intento (vista o
consulta). Copiarlo es tener dos verdades.

**Los tokens de enrolamiento se guardan como hash** (SHA-256). El token en claro se muestra una
vez, al emitirlo, y nunca más.

### 4.3 Aplicación — `tack-app::runs`

Aquí sí nacen los **puertos**, porque las pruebas de concurrencia y recuperación necesitan un
fake que simule en milisegundos lo que con SQLite tardaría segundos:

```rust
pub trait RunStore: Send + Sync {
    fn enqueue(&self, r: NewRequest) -> impl Future<Output = Result<RequestId, StoreError>> + Send;
    fn claim(&self, runner: RunnerId, attempt: AttemptId, ttl: Duration)
        -> impl Future<Output = Result<Option<Lease>, StoreError>> + Send;
    fn transition(&self, attempt: AttemptId, token: FencingToken, to: ExecutionState, by: Actor)
        -> impl Future<Output = Result<(), StoreError>> + Send;
    // ...
}
```

**Detalle de Rust:** `async fn` dentro de un trait funciona en Rust moderno, pero no garantiza
que el futuro sea `Send`, y `tokio::spawn` lo exige. Escribir la firma como
`fn ... -> impl Future<Output = ...> + Send` lo garantiza sin macros externas.

Los casos de uso: `StartRun`, `ClaimAttempt`, `Heartbeat`, `ReportEvents`, `AskDecision`,
`AnswerDecision`, `Complete`, `Cancel`, `Recover`. Cada uno:

1. valida con el dominio (`validate_transition`, `resolve`, `select`);
2. comprueba la **clave de idempotencia**: si ya se procesó, devuelve la respuesta guardada;
3. llama al store con el fencing token;
4. publica el evento en el broadcast tras confirmar.

Y los **ticks**, cada uno como `pub async fn tick(...)` (una pasada) y `pub async fn run(...,
every)` (el bucle):

- `expire_leases`: leases caducadas → `Lost` o reencolar, según el journal reportado;
- `schedule`: solicitudes en cola → asignar a runners con hueco;
- `retention`: borrar eventos y artefactos viejos de intentos terminados;
- `observability`: una foto de salud de la flota **sin ids** (contadores por estado), para no
  convertir las métricas en un registro de actividad.

**Pruebas de esta capa:** contra el fake (`FakeRunStore` en `tack-test-support`), con un
`FixedClock` que se avanza a mano. "Dos runners reclaman el mismo intento: uno gana." "Un evento
con token viejo da `StaleLease` y no escribe." "Una lease caduca a los 30 s: el tick la marca."
Y la **suite de contrato** `run_store_contract!` instanciada sobre el fake y sobre SQLite: así
sabes que el fake no miente.

### 4.4 HTTP — dos superficies

```rust
pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/api", operator_routes().layer(require_operator_token(...)))
        .nest("/api/runner/v1", runner_routes())   // hermano, no hijo: no pasa por el auth de operador
        .with_state(state)
}
```

- **Operador** (`/api`): la UI y la CLI, detrás de un token de operador.
- **Runner** (`/api/runner/v1`): cada handler valida la credencial del runner (bearer, comparada
  por hash). Está montada como **hermana** de `/api`, no como excepción dentro de ella: una lista
  de excepciones se puede ampliar sin querer en una edición futura; una estructura separada, no.

**El principal no se lee de la petición.** Un middleware escribe `x-tack-principal` desde la
configuración del servidor. Si se leyera de la petición, cualquiera podría hacerse pasar por
otro y chocar con sus claves de idempotencia.

**Hecho cuando (etapa 4):** un test de `tack-cli` arranca el servidor real y, con un cliente
HTTP que hace de runner, recorre `enroll → claim → accept → start → events → completion`, y
comprueba que el intento termina `Succeeded` y que un segundo `claim` con el token viejo da
`stale_lease`.

---

## Etapa 5 — El runner

**Qué se construye.** El proceso que ejecuta agentes cerca del código y de las credenciales del
usuario. Depende solo de `tack-protocol` (y de `tack-core` para el brief).

**Por qué *pull* y no *push*.** El runner pregunta al servidor si hay trabajo; el servidor nunca
llama al runner. Así el runner puede estar detrás de un NAT, en un portátil, sin abrir puertos, y
sus credenciales nunca viajan al servidor.

### 5.1 Puertos primero

```rust
pub trait ProtocolClient: Send + Sync { /* claim, heartbeat, events, decisions, complete */ }
pub trait ProcessSupervisor: Send + Sync { /* spawn, kill_group, wait */ }
pub trait Filesystem: Send + Sync { /* lo mínimo que el motor toca */ }
pub trait WorkspaceProvider: Send + Sync { /* crear y borrar la copia aislada */ }
pub trait Verifier: Send + Sync { /* evidencia → MRP */ }
pub trait HarnessAdapter: Send + Sync { /* validate, start, cancel, wait, reconcile */ }
pub trait HarnessProbe: Send + Sync { /* versión y capacidades, sin un intento */ }
```

El runner es el lugar del programa donde la hexagonal completa **sí** se paga: casi todo lo que
toca es efecto (procesos, disco, red, git) y probar el motor con efectos reales sería lento y
frágil.

### 5.2 El motor

`RunnerEngine<P, A, W, C>` genérico sobre sus puertos. Su ciclo, en orden:

1. **Claim** con su credencial.
2. **Journal antes de lanzar nada:** un archivo TOML, solo legible por el dueño, con el intento,
   el token y el estado. Si el proceso muere, al volver lee el journal y sabe exactamente qué
   estaba haciendo.
3. **Workspace:** una copia aislada (worktree) del repo, con `prepare` instalando dependencias
   desde el lockfile (con red solo durante la instalación).
4. **Spawn** del agente, en su propio grupo de procesos, con salida acotada, timeout y entorno
   filtrado (sin secretos que no le correspondan; un `redact` sobre todo lo que sale).
5. **Eventos** por lotes cada dos segundos, con número de secuencia contado en el journal (el
   servidor guarda uno por `(intento, secuencia)`: un reintento no duplica).
6. **Heartbeat** para todos los intentos activos en un solo tick.
7. **Decisiones:** si el agente pregunta, se publica la decisión y se hace poll de la respuesta.
8. **Cierre:** evidencia del cambio → verificador → rama (si está configurado) → `completion`.
   Un fallo del verificador o del push es **un evento**, no cambia el resultado del intento.
9. **Limpieza** del workspace.

Mantén el motor en varios archivos desde el principio (`engine/claim.rs`, `engine/decisions.rs`,
`engine/outcome.rs`, `engine/resume.rs`): un motor de 2.600 líneas en un solo archivo es lo que
Tack tuvo que partir después.

### 5.3 Los agentes, en este orden

1. **Un agente falso** (`fixtures/fake_harness.sh` con modos por variable de entorno: éxito,
   fallo, pregunta, mucha salida, colgado). Con él se prueba todo el motor sin gastar un token.
   Esta es la mejor inversión de la etapa.
2. **El primer agente real** (Claude Code): un `HarnessDescriptor` (datos: nombre, binario,
   versión mínima) + un `HarnessGrammar` (código: línea de comandos, cómo leer su salida, si
   puede conversar por stdio). Pruebas en vivo con `#[ignore]` y comprobación del binario en
   `PATH`: nunca obligatorias en CI.
3. **Los demás CLI** (Codex, OpenCode): un módulo y una línea en `DESCRIPTORS` cada uno.
4. **docket**, que es distinto: un daemon con API HTTP, no un proceso por run. Su adaptador
   implementa `HarnessAdapter` directamente sobre la API. **Su versión se negocia al primer uso**
   (`GET /status.json`), no se fija en una dependencia: si docket no habla la versión esperada,
   el runner lo dice y no envía nada. Se prueba contra un fake en proceso de su API.

**Hecho cuando (etapa 5):** el flujo 2 de §A.3 funciona con el agente falso, de punta a punta,
contra el servidor real. Y el flujo 4: matar el runner con `kill -9` a mitad de un intento,
reiniciarlo, y ver que el intento se recupera según el journal, sin ejecutarse dos veces.

```bash
cargo tree -p tack-runner -e normal --prefix none | grep -cE '^(sqlx|libsqlite3-sys|tack-db) '   # 0
```

---

## Etapa 6 — Entrega

**Qué se construye.** Lo que pasa después de que el agente termina.

| Orden | Pieza | Capa | Lo esencial |
|---|---|---|---|
| 6.1 | **Evidencia** | runner | El diff entre el commit base y el resultado, con estadísticas. Para todo agente |
| 6.2 | **Verificador** | runner (puerto `Verifier`) | Un adaptador por verificador externo (prill es uno). El puerto existe para que ninguno sea una dependencia del producto. Su salida es un MRP (`tack-protocol::mrp`) |
| 6.3 | **Revisión** | app + api + frontend | La persona abre el MRP (se registra que lo vio) y da un veredicto; el primero gana |
| 6.4 | **Autonomía** | dominio (`tack-core::runs::autonomy`) | `evaluate(Facts) → Merge | Hold(reason)`, pura. Niveles por proyecto, con el techo del equipo que ejecutó |
| 6.5 | **Merge** | app (tick) | Lee los hechos, llama a `evaluate`, hace merge o degrada. El techo se lee de la capacidad **viva** del runner, en el momento del merge |
| 6.6 | **Arranque automático** | dominio (`pick`) + app (tick) | Elige qué tareas en espera arrancan, bajo un principal propio (`auto_start`) para que quede claro quién lo hizo |
| 6.7 | **Ramas y PRs** | runner (git) + integraciones | Push de la rama del intento; el PR se registra contra el ítem |

**Hecho cuando (etapa 6):** el flujo 3 de §A.3 funciona con el agente falso y un verificador
falso: un MRP que pasa produce un merge automático en un proyecto con autonomía alta, y queda en
espera en uno con autonomía baja.

---

## Etapa 7 — Los bordes

Todo lo que conecta Tack con otros sistemas. Cada uno es un **adaptador** sobre casos de uso que
ya existen. Si para añadir uno necesitas cambiar el dominio, algo de las etapas anteriores quedó
incompleto.

| Pieza | Dónde | Nota |
|---|---|---|
| **CLI** (`tack add`, `tack list`, `tack move`…) | `tack-cli`, un módulo por grupo de comandos | Habla con el servidor por HTTP, **nunca** abre la base directamente: así las reglas del workflow se aplican igual |
| **MCP** (`tack mcp`) | `tack-cli` | Servidor por stdio que traduce herramientas MCP a llamadas HTTP al servidor. Mismo motivo |
| **Importar de GitHub y Linear** | `tack-app::integrations` | Paginación por cursor, filtro por etiquetas; los ítems importados llevan `source` no confiable |
| **Sincronizar con GitHub** | tick en `tack-app` | Estado y comentarios hacia la issue en cada cambio; de vuelta por poll |
| **Copias de seguridad** | `tack-app` + adaptador S3 | `VACUUM INTO` para una copia consistente; restaurar es preparar y aplicar al reiniciar, nunca en caliente |
| **Webhooks** | `tack-app` | Firmados con un secreto |
| **Exportación** | ya existe (etapa 2.13) | JSON, YAML, CSV |

---

## Etapa 8 — El producto

| Pieza | Nota |
|---|---|
| **Servicio del sistema** (`tack service install`) | Unidad de systemd de usuario en Linux, agente de launchd en macOS |
| **Runner embebido** (`tack serve --with-runner`) | Un solo módulo en `tack-app`; el servidor y el runner siguen hablando por el protocolo aunque estén en el mismo proceso |
| **App de escritorio** (`tack-desktop`) | Tauri, **fuera del workspace** (arrastra GTK y WebKit); supervisa `tack` como sidecar, no lo reimplementa |
| **Empaquetado y release** | Un ensayo del workflow de release antes de etiquetar; los comandos de instalación del README se comprueban contra la release publicada |
| **Documentación** | Tres guías: Usuario, Administrador, Desarrollador. Las dos primeras sin rutas de código ni números de fase. Los números (rutas, migraciones, tests) los calcula un script; el documento dice cómo |
| **CHANGELOG** | `[Unreleased]` escrito a mano en el mismo cambio que lo hace verdad |

---

## Parte C — Las trampas que Tack ya pisó

Lo que la Fase 79 corrige, para que tú no lo construyas así:

| Trampa | Cómo apareció | Cómo se evita en esta guía |
|---|---|---|
| Los tipos compartidos dentro de un crate con base de datos | El runner compila SQLite | `tack-protocol` existe antes que servidor y runner (etapa 3) |
| SQL en los handlers | ~190 consultas en `tack-api` | `cargo deny` prohíbe `sqlx` fuera de `tack-db` desde la etapa 0 |
| La máquina de estados en tres sitios | `enum` + literales + llamadas sueltas | Un `enum` en `tack-protocol`; conversión a texto en un solo sitio |
| Estado copiado en dos tablas | La solicitud espejaba al intento | Se deriva, no se copia (4.2) |
| Tipos "espejo" para documentar | 18 structs mantenidos a mano | `utoipa` tras un feature en el crate dueño del tipo |
| Tres traits `Clock` iguales | Cada crate inventó el suyo | Un `Clock` en `tack-protocol` (3.6) |
| Handlers de 3.000 líneas | No había capa de aplicación | `tack-app` desde la etapa 1; un handler de más de 20 líneas es una alarma |
| `tokio::spawn` dentro de la biblioteca HTTP | Los jobs vivían en `tack-api` | `tick()` en `tack-app`; los `spawn`, en la raíz de composición |
| Todas las pruebas necesitan SQLite | No había fakes | Fakes + suite de contrato desde la etapa 4 |
| `main.rs` y `engine.rs` gigantes | Crecieron sin costuras | Módulos por grupo de comandos y por fase del motor desde el principio |
| `shared/` del frontend con features dentro | Se movió lo "común" sin regla | Test de dirección en ambos sentidos desde la etapa 1 |
| Documentos con números que se pudren | Conteos escritos a mano | Un script calcula; el documento enlaza |
| Un token en la raíz sin ignorar | Archivo local al lado del código | Estado local en `.local/`, ignorado desde la etapa 0 |

---

## Parte D — Dónde usar IA al construir

La misma regla de la guía de la Fase 79, aplicada a construir en vez de a refactorizar: **lo que
el compilador o un test pueden verificar, se escribe con una plantilla o un script; el modelo se
usa para lo que exige elegir.**

| Tarea | Script / plantilla | Modelo | Por qué |
|---|---|---|---|
| Crear los nueve crates, el workspace, el hook | ✓ | | Determinista |
| `openapi.json` y el cliente TS | ✓ (`utoipa`, `openapi-typescript`) | | Generado, nunca a mano |
| Un CRUD más (sprints, roles, comentarios) siguiendo el de ítems | plantilla | revisión | El patrón ya existe; el modelo solo rellena nombres y valida contra la plantilla |
| La máquina de estados y quién puede qué | | ✓ con una persona | Es una decisión de producto, no de código |
| Las transacciones de claim y recuperación | | ✓ con una persona, y pruebas de concurrencia contra archivo | Un error aquí ejecuta un agente dos veces |
| Un `HarnessGrammar` nuevo (leer la salida de otro agente) | | ✓ | Hay que leer la documentación del agente y decidir cómo mapear su salida |
| Fixtures de contrato | ✓ (capturar de un run real con un script) | | Grabadas, no inventadas |
| Documentación de usuario | | ✓ | Redacción |

Y cuando el modelo trabaja: una tarea, una lista de archivos, una receta, un "hecho cuando" que
es un comando. Nunca "construye la etapa 4".

---

## Checklist final

Cuando todo esté construido, estas órdenes deben pasar. Son el retrato de un Tack bien hecho:

```bash
# El grafo es el de §A.2 y nadie lo cruza
scripts/check-crate-graph.sh
cargo deny check

# El runner no sabe que existe una base de datos
cargo tree -p tack-runner -e normal --prefix none | grep -cE '^(sqlx|libsqlite3-sys|tack-db) '      # 0

# El SQL tiene una sola casa
grep -rln 'sqlx::' crates --include='*.rs' | grep -vE 'tack-db|tack-test-support' | wc -l          # 0

# Un solo enum de estados, un solo Clock del servidor
grep -rn 'trait .*Clock' crates --include='*.rs'                                                    # protocol + runner
grep -rnE '"(leased|preparing|waiting_decision)"' crates --include='*.rs' | grep -v tack-protocol | grep -v tests | wc -l   # 0

# Nada en la biblioteca HTTP lanza hilos
grep -rn 'tokio::spawn' crates/tack-api/src | wc -l                                                 # 0

# Nada escondido
grep -rc 'allow(dead_code)' crates --include='*.rs' | grep -v ':0' | wc -l                          # 0
cargo machete

# Todo pasa
cargo nextest run --workspace
cd frontend && npm test && npx playwright test
.githooks/pre-push
```

Y a mano, los cuatro flujos de §A.3, en ese orden.
