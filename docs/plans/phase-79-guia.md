# Guía de la Fase 79, para quien está aprendiendo

> **Nota (2026-10-25):** el plan fue revisado tras una auditoría. Ya no lleva puertos sobre el
> almacén, fakes en memoria ni suite de contrato: los casos de uso reciben `&Repository` y
> `&dyn Clock`, y cada test sigue corriendo sobre SQLite en memoria. Las secciones de esta guía
> que explican esos conceptos siguen siendo válidas como enseñanza, pero describen el primer
> borrador, no lo que se va a construir. Lo que manda es [`phase-79.md`](phase-79.md), §7.

Esta guía acompaña al plan operativo [`phase-79.md`](phase-79.md) y a la decisión
[ADR 0087](../adr/0087-the-protocol-is-a-crate-and-the-sql-has-one-home.md). El plan dice *qué
comandos correr*. Esta guía explica *por qué* cada paso existe, qué concepto de arquitectura,
de Rust o de ingeniería con IA hay detrás, y cómo saber que quedó bien. Si ya conoces los
conceptos, salta al plan; si estás aprendiendo, lee esto primero y ten el plan al lado.

Si prefieres ver la arquitectura de destino construida desde cero, en orden de capas, en vez de
como un refactor del código actual, lee [Construir Tack desde cero](phase-79-desde-cero.md).

---

## Parte I — Los conceptos que vas a usar

### 1. Qué es una arquitectura y por qué nos importa la "dirección"

Una arquitectura es la respuesta a una pregunta: **¿qué puede saber de qué?** No es dónde están
los archivos, sino qué parte del programa tiene permiso para depender de otra.

Imagina tres piezas: las *reglas del negocio* (qué es una tarea, cuándo puede pasar de "en
curso" a "terminada"), la *base de datos* (cómo guardamos eso en SQLite) y la *web* (cómo lo
mostramos por HTTP). Si las reglas dependen de la base de datos, no puedes probar una regla sin
levantar una base de datos, y cambiar de SQLite a otra cosa te obliga a tocar las reglas. Si en
cambio la base de datos depende de las reglas, pasa lo contrario: las reglas se prueban solas, y
la base de datos es un detalle que se puede cambiar.

Por eso se dice que las dependencias deben **apuntar hacia adentro**: hacia lo que cambia menos
y vale más, que son las reglas. Afuera quedan los detalles: la base de datos, HTTP, el sistema
de archivos.

### 2. Arquitectura hexagonal, en una frase

La arquitectura hexagonal (también llamada "puertos y adaptadores") es una forma de conseguir
esa dirección. El **núcleo** (las reglas) define **puertos**: contratos que dicen "necesito algo
que sepa guardar una tarea" sin decir cómo. Los **adaptadores** son las implementaciones de esos
contratos: una con SQLite, otra en memoria para pruebas. El núcleo nunca sabe cuál está usando.

En Rust, un puerto es un **trait** y un adaptador es un **tipo que implementa ese trait**.

```rust
// El puerto: lo define el núcleo. Dice qué necesita, no cómo se hace.
pub trait RunStore: Send + Sync {
    async fn enqueue(&self, request: NewRequest) -> Result<RequestId, StoreError>;
}

// Un adaptador: lo implementa la capa de base de datos.
impl RunStore for SqliteRepository { /* SQL aquí */ }

// Otro adaptador: lo implementan las pruebas.
impl RunStore for FakeRunStore { /* un BTreeMap en memoria */ }
```

Tack **no es hexagonal hoy**, y la Fase 79 tampoco lo convierte del todo. Lo que hace es
arreglar la dirección de las dependencias donde está al revés y poner puertos solo donde hace
falta probar algo sin base de datos. Eso es un criterio importante que aprenderás aquí: **una
abstracción se paga; solo se añade cuando compra algo concreto** (una prueba más rápida, un
binario más pequeño, una frontera de seguridad).

### 3. Crates, workspaces y por qué un crate es una decisión

En Rust, un **crate** es la unidad de compilación: una biblioteca o un binario. Un
**workspace** es un conjunto de crates que comparten un `Cargo.lock` y una carpeta `target/`.
Tack es un workspace con siete crates en `crates/`.

Cuando un crate `A` declara en su `Cargo.toml` que depende de `B`, **todo lo que `B` arrastra lo
arrastra `A`**. Esto es lo que hoy duele en Tack: `tack-runner` (el proceso que ejecuta agentes
y guarda credenciales) depende de `tack-orch`, que depende de `tack-db`, que trae `sqlx` y
SQLite. El runner compila una base de datos que nunca abre. Puedes verlo tú mismo:

```bash
cargo tree -p tack-runner -e normal --prefix none | grep -E '^(sqlx|libsqlite3-sys|tack-db) '
```

Si imprime líneas, el problema existe. Al terminar la Fase 1 debe imprimir nada.

Regla que usamos para decidir si algo merece ser un crate propio. Debe cumplir al menos una:

- (a) tiene un **conjunto de dependencias distinto** (ej.: solo él necesita `sqlx`);
- (b) es una **unidad de despliegue o frontera de confianza distinta** (ej.: el runner);
- (c) impone una **dirección de dependencia** que importa (ej.: el contrato que dos procesos
  comparten y que ninguno de los dos debe poder contaminar).

Si no cumple ninguna, es un módulo dentro de un crate, no un crate. Por eso el plan añade solo
dos crates y no siete.

### 4. Máquinas de estado y por qué "una sola fuente de verdad"

Un intento de ejecución pasa por estados: `queued → leased → preparing → running → succeeded`
(o `failed`, `cancelled`, `waiting_decision`…). Las reglas de qué transición está permitida y
quién puede pedirla forman una **máquina de estados**.

Hoy en Tack esa máquina existe tres veces: una versión tipada en `tack-orch`
(`validate_transition`, con un `enum`), otra como cadenas de texto `"leased" | "preparing"` en
`tack-db`, y llamadas sueltas desde los handlers HTTP. Cuando algo existe tres veces, tarde o
temprano una copia cambia y las otras no, y nadie se entera hasta que falla en producción.

En Rust la herramienta para que esto sea imposible es el **enum**: si añades un estado al `enum`,
el compilador te señala *cada* `match` que no lo contempla. Con cadenas de texto, el compilador
no ve nada. La Fase 2 convierte las cadenas en el enum.

### 5. Los tests viajan con el código

Tack sigue una convención: el módulo `foo.rs` tiene sus pruebas en `foo/tests.rs`, enlazadas
con `#[path = "foo/tests.rs"] mod tests;`. Hay 122 así. Esto importa para el refactor: cuando
movemos `foo.rs` con `git mv`, movemos `foo/tests.rs` en el mismo commit y las pruebas siguen
siendo las mismas, en un solo sitio. **Nunca hay una copia vieja y una nueva conviviendo.**

Otra idea que vas a ver: la **suite de contrato**. Si dos tipos implementan el mismo trait (el
adaptador SQLite y el fake en memoria), deben comportarse igual. En vez de escribir las mismas
pruebas dos veces, se escribe una vez como macro y se instancia sobre cada implementación. Si el
fake pasa y SQLite no, el fake miente; si SQLite pasa y el fake no, el fake está incompleto.

### 6. Ingeniería con IA: cuándo usar un modelo y cuándo un script

Esta es quizá la lección más importante de la guía, porque es la que más dinero y tiempo
ahorra.

Un modelo de lenguaje (un "agente") es caro, lento y no determinista: dos veces la misma
pregunta pueden dar dos respuestas. Un script (`sed`, `git mv`, `cargo check`) es barato,
instantáneo y da siempre lo mismo. La regla:

> Un paso va a un script cuando el compilador (o `npm test`) puede decir si quedó bien.
> Va a un modelo solo cuando alguien tiene que **elegir** algo: un nombre, una agrupación, dónde
> termina una transacción.

Mover un archivo de carpeta, renombrar una ruta `tack_orch::execution` a
`tack_protocol::execution` en 50 archivos, borrar lo que el compilador llama "nunca usado": todo
eso es script. Decidir que estas 30 consultas SQL se llaman `attempts_of_request` y viven en
`queries/execution_attempts.rs`: eso es modelo.

Y cuando sí usamos un modelo, lo hacemos con tres límites:

1. **Recibe una lista que no hizo él.** Un script produce `archivo:línea:verbo:tabla`; el agente
   trabaja sobre eso y nada más. Así no "explora" el repositorio gastando tokens en leer cosas
   que no necesita.
2. **Recibe una receta cerrada.** "Un `SELECT` que devuelve filas va a `queries/<tabla>.rs` como
   una función por consulta." No hay decisiones abiertas que la receta no nombre.
3. **Tiene un "hecho cuando" que es un comando.** `cargo nextest run --workspace -E
   'rdeps(tack-db)'` pasa. No "cuando le parezca bien".

El plan entero tiene **seis** pasos de modelo. Todo lo demás es script. Cuando leas una fase,
fíjate en cómo cada paso de modelo va precedido de un script que le prepara la entrada.

### 7. Cómo correr las pruebas sin correrlo todo

Tack usa `cargo nextest` (nunca `cargo test`, ver `.config/nextest.toml`). Lo útil para un
refactor es el **filterset**: una expresión que selecciona qué pruebas correr.

```bash
cargo nextest run --workspace -E 'rdeps(tack-protocol)'
```

`rdeps(X)` significa "X y todo lo que depende de X". Si tocaste `tack-protocol`, eso es
exactamente lo que puede haberse roto; lo demás es ruido y tiempo. La suite completa se corre
**una vez por fase**, sobre el árbol ya mergeado, como compuerta. Nunca uses `-p`: compila los
crates dos veces con features distintas.

---

## Parte II — Las fases, una por una

Cada fase es un PR (pull request) hacia `develop`. No empieces una fase sin haber cerrado la
anterior: el "árbol esperado" de una es el punto de partida de la siguiente.

Antes de cualquier fase:

```bash
export PATH=$HOME/.cargo/bin:$PATH     # el cargo de rustup, no el de Homebrew
git checkout develop && git pull
git checkout -b fase-79-N              # una rama por fase
```

---

### Fase 0 — Higiene y herramientas

**Qué hacemos.** Nada estructural. Tapamos un agujero (un archivo con un token que podría
subirse al repo), sacamos el estado local de la raíz, instalamos dos herramientas y creamos el
script que vigila el grafo de dependencias.

**Por qué empezar aquí.** Porque todas las fases siguientes van a *estrechar* una tabla de
dependencias permitidas. Para estrechar algo, primero tiene que existir. Y porque un refactor
grande es el peor momento para descubrir que un `git add .` subió un secreto.

**Cómo.**

1. El token. Hay un archivo `decision-token` en la raíz que no está en `.gitignore`. Compruébalo:
   ```bash
   git check-ignore decision-token; echo "exit: $?"
   ```
   Si dice `exit: 1`, no está ignorado. Añádelo:
   ```bash
   printf '/decision-token\n' >> .gitignore
   git check-ignore decision-token     # ahora imprime el nombre y sale con 0
   ```
   *Lo que aprendes:* `git check-ignore` es la forma de preguntar "¿esto se subiría?". Úsalo
   siempre que un archivo con pinta de secreto aparezca en `git status`.

2. El estado local. `tack.db`, `logs/`, `storage/` están ignorados, pero viven en la raíz junto
   al código. Van a `.local/`. Después busca quién los nombra y apúntalo allí:
   ```bash
   grep -rn 'tack\.db\b\|/logs/\|/storage/' Makefile scripts docs/CONFIG.md CONTRIBUTING.md
   ```

3. Herramientas. `ast-grep` busca y reemplaza entendiendo la sintaxis de Rust (no como `sed`,
   que solo ve texto). `cargo-machete` encuentra dependencias declaradas en `Cargo.toml` que
   nadie usa.
   ```bash
   cargo install ast-grep cargo-machete
   ```
   Ojo: en esta máquina ya existe un comando `sg` que **no** es ast-grep (es el de grupos de
   Linux). Usa siempre `ast-grep`, no `sg`.

4. El script del grafo. Está completo en el plan (§ Fase 0, paso 4). Lo que hace: le pide a
   `cargo metadata` la lista de dependencias entre crates del workspace y la compara con una
   tabla `ALLOW`. Cualquier arista que no esté en la tabla hace fallar el script.

   La primera vez, la tabla **es** el grafo de hoy, con sus defectos incluidos (el runner
   dependiendo de orch, etc.). Eso es deliberado: la fase 0 no arregla nada, solo deja
   constancia. Cada fase posterior quita de la tabla la arista que acaba de eliminar del código.

5. Engancharlo al hook. `.githooks/pre-push` es la compuerta de este repo: lo que corre antes de
   que un cambio pueda salir de la máquina. Añade una línea que llame al script, junto a la de
   `cargo clippy`.

**Cómo verificar.**

```bash
git check-ignore decision-token && scripts/check-crate-graph.sh && .githooks/pre-push
```

Las tres cosas salen con 0. `git status --porcelain` muestra solo `.gitignore`, `scripts/`,
`.githooks/pre-push` y las ediciones de docs del paso 2.

**Qué esperar.** Los crates son los mismos siete. Hay un script nuevo. El hook es un poco más
lento (medio segundo).

**Qué aprendiste.** Que una regla de arquitectura que no se verifica con una herramienta es un
deseo, no una regla. Y que el primer paso de un refactor es hacer visible el estado actual, no
cambiarlo.

---

### Fase 1 — `tack-protocol`: el contrato se vuelve un crate

**Qué hacemos.** Creamos un crate nuevo, `tack-protocol`, y movemos a él los tipos que el
servidor y el runner comparten: los mensajes del protocolo runner-v1, la máquina de estados,
las capacidades, la proveniencia de uso, el paquete de "listo para merge" (MRP) y el paso
`prepare`. También unificamos tres traits `Clock` idénticos en uno.

**Por qué.** Hoy esos tipos viven en `tack-orch`, que depende de `tack-db`. Como el runner los
necesita, el runner depende de `tack-orch` y por tanto de SQLite. Al sacarlos a un crate que no
depende de nada del workspace, el runner depende solo de ese crate, y SQLite desaparece de su
árbol. Es el criterio (b) y (c) de la Parte I: una frontera de confianza y una dirección que
importa.

¿Cómo sabemos que se puede mover sin reescribir? Medimos:

```bash
grep -n '^use' crates/tack-orch/src/execution/*.rs crates/tack-orch/src/usage_provenance.rs
```

Solo importan `std`, `serde`, `chrono` y a sí mismos. Ninguna I/O. Eso significa que el
movimiento es un `git mv` y un `sed`. **Este es el patrón que debes buscar siempre antes de
proponer un refactor: mide si la pieza ya está desacoplada; si lo está, el coste es casi cero.**

**Cómo.** El plan trae los comandos exactos; aquí el sentido de cada bloque.

1. *El `Cargo.toml` del crate nuevo.* Fíjate en dos cosas. Primero, `version.workspace = true`
   y similares: heredan del `[workspace.package]` de la raíz, así no hay versiones
   desincronizadas. Segundo:
   ```toml
   [features]
   openapi = ["dep:utoipa"]
   ```
   Esto es un **feature flag**. `utoipa` genera el esquema OpenAPI, pero el runner no lo
   necesita. Con el feature, `tack-api` lo activa y el runner no lo compila. Ya `tack-core` hace
   lo mismo; estamos copiando un patrón que el repo ya conoce.

2. *Los `git mv`.* Cada archivo con su `tests.rs` hermano. `git mv` en vez de `mv` para que git
   registre un rename (`R`) y no un borrado más un alta; así el historial del archivo se
   conserva y puedes verificar que no perdiste nada con `git diff --stat -M`.

3. *El `sed` de rutas.* Cambiamos `tack_orch::execution` por `tack_protocol::execution` en todos
   los `.rs`. ¿Es seguro usar `sed` (que solo ve texto)? Sí, **porque esos identificadores no
   aparecen con otro significado en ningún sitio**; lo comprobamos con `grep` antes. Si un
   rename fuera ambiguo (por ejemplo, renombrar `Clock` a secas podría tocar un comentario),
   `ast-grep` es la herramienta. Esta distinción —cuándo basta texto y cuándo hace falta
   sintaxis— es parte del oficio.

4. *Dentro de los crates que perdieron módulos*, las rutas `crate::execution` pasan a
   `tack_protocol::execution`. Y hay un `grep` de control:
   ```bash
   grep -rn 'crate::\|super::' crates/tack-protocol/src crates/tack-protocol/tests | grep -v '...lista de lo esperado...'
   ```
   Debe imprimir nada. Si imprime algo que apunta a `tack-core` o `tack-db`, **ese módulo no es
   protocolo**: tiene una dependencia hacia el dominio que la medición no vio. Se para y se
   informa. No se "arregla" añadiendo la dependencia, porque entonces `tack-protocol` dejaría de
   ser la hoja del grafo y todo el sentido de la fase se perdería.

5. *Los tres relojes.* `ExecutionClock` (db), `RetentionClock` y `ObservabilityClock` (orch)
   tienen exactamente la misma firma: `fn now(&self) -> DateTime<Utc>`. Tres nombres para una
   idea. Pasan a `tack_protocol::Clock`. El del runner usa `SystemTime` (otro tipo) y **se deja
   en paz**: unificar lo que no es idéntico sería inventar trabajo.

   *Por qué existe un trait `Clock`:* para que una prueba pueda congelar el tiempo. Un test
   que dice "esta lease expira en 30 segundos" no puede esperar 30 segundos reales; inyecta un
   `FixedClock` y avanza la hora a mano. Es el ejemplo más pequeño y claro de "puerto".

6. *Que el compilador haga la lista.*
   ```bash
   cargo check --workspace --all-targets 2>&1 | grep -E '^(error|warning: unused)' | sort | uniq -c
   ```
   Esa salida es **la entrada del único paso de modelo de la fase**. Esperamos pocas líneas:
   algún `use` que falta, un reloj falso en un test que implementaba el nombre viejo, y las
   referencias a los tipos "espejo". Si salen más de ~30 errores, un `sed` tocó algo que no
   debía: `git stash`, repasa, reintenta paso a paso.

**El paso de modelo.** Un agente, con la lista del compilador y una receta:

- Arreglar cada error de la lista. Sin lógica nueva.
- En `tack-protocol`, añadir `#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]` a los
  tipos que `tack-api` hoy copia a mano "solo para documentar". Esas copias se llaman
  `RunnerV1ErrorEnvelope`, `UsdMeasurementSchema`, etc., y están en `handlers/executions.rs` con
  un comentario que dice literalmente "doc-only mirror". Existen porque `tack-orch` no podía
  derivar `ToSchema`. Ahora `tack-protocol` sí puede, así que las copias se borran y las rutas
  apuntan al tipo real.
- Regenerar `docs/openapi.json` con `./scripts/regen-generated.sh` y mirar el diff. Deben
  cambiar **solo nombres** de esquemas. Si aparece o desaparece un campo, la copia a mano no era
  fiel al original: eso es un hallazgo, se reporta, no se tapa.

*Lo que aprendes aquí sobre IA:* el agente no decide qué mover ni dónde; eso ya lo decidió el
script. El agente hace lo que un script no puede: leer un mensaje de error y escribir la línea
`use` correcta. Es trabajo de modelo, pero acotado por una lista finita.

**Cómo verificar.**

```bash
cargo tree -p tack-runner -e normal --prefix none | grep -cE '^(sqlx|libsqlite3-sys|tack-db|tack-orch) '
# esperado: 0  ← el runner ya no compila SQLite
grep -rn 'trait .*Clock' crates --include='*.rs' | grep -v target
# esperado: 2 líneas (protocol/src/clock.rs y runner/src/clock.rs)
grep -rn 'doc-only mirror\|hand-verified mirror' crates/tack-api/src | wc -l
# esperado: 0
cargo nextest list --workspace | wc -l
# esperado: el mismo número que antes de la fase (anótalo antes de empezar)
scripts/check-crate-graph.sh && cargo nextest run --workspace -E 'rdeps(tack-protocol)' && .githooks/pre-push
```

Y en el script del grafo, la fila de `tack-runner` pasa a `"tack-core tack-protocol"`. Si lo
olvidas, el script no fallará (la tabla sigue permitiendo la arista vieja), pero la fase no
habrá cumplido su promesa. **Estrechar la tabla es parte de la fase, no un extra.**

**Qué esperar.** Ocho crates. `crates/tack-orch/src/` conserva solo `scheduler/`,
`model_policy/`, `execution_retention*` y `execution_observability*`. El `Cargo.toml` del runner
ya no menciona `tack-orch`.

**Qué aprendiste.** Que la medición previa (`grep` de los `use`) es lo que convierte un refactor
"de miedo" en un `git mv`. Que un feature flag resuelve el dilema "este crate necesita X pero
aquel no". Y que borrar código duplicado es, muchas veces, consecuencia de poner un tipo en el
sitio correcto, no una tarea aparte.

---

### Fase 2 — El SQL tiene una sola casa (tres PRs)

**Qué hacemos.** Sacamos las ~190 consultas SQL que hoy están en `tack-api` (el crate HTTP) y las
llevamos a `tack-db`. Creamos un error propio, `StoreError`, para que `sqlx::Error` no cruce la
frontera. Y cambiamos los 20 literales de estado por el `enum` de `tack-protocol`.

**Por qué.** Hay un `Repository` en `tack-db` hecho para contener el SQL, y aun así los handlers
HTTP ejecutan consultas directamente. Cuando cambias una columna, tienes que buscar SQL en dos
crates. Cuando `sqlx::Error` llega hasta HTTP, el handler necesita saber cómo falla SQLite para
decidir un código de estado. Y las cadenas `"leased"` repartidas son la máquina de estados
duplicada de la Parte I.

**Por qué tres PRs y no uno.** Porque un PR de 190 cambios no lo revisa nadie, y porque un
agente con una lista de 190 sitios se pierde. Se parte por grupo de tablas, que es la línea
natural: cada grupo tiene un dueño, una lista corta y un filterset. En orden: **2a** runners y
perfiles (~27 sitios), **2b** solicitudes, intentos, eventos y decisiones (~94), **2c** el resto
(~69).

**Cómo.** El patrón de cada PR es el mismo:

1. *El script hace la lista.* Un `perl` recorre `crates/tack-api/src`, encuentra cada
   `sqlx::query...("SELECT ... FROM tabla")` y escribe `archivo:línea:verbo:tabla`. Luego se
   filtra por las tablas del PR. Esa lista es lo único que el agente recibe.

   *Lo que aprendes:* no le pidas a un modelo "encuentra todo el SQL". Es una tarea de texto
   regular y determinista; `perl`/`grep` la hacen en un segundo sin error. El modelo entra
   después.

2. *El agente aplica la receta, sitio por sitio.* La receta tiene cuatro casos, y cada sitio cae
   en exactamente uno:

   | Si el sitio es… | Va a… | Cómo |
   |---|---|---|
   | un `SELECT` que devuelve filas para una respuesta, un listado, un conteo | `tack-db/src/queries/<tabla>.rs`, una función por consulta | el SQL se copia **literal**; la función se llama por la pregunta que responde (`attempts_of_request`), no por el handler que la usaba |
   | un `SELECT` que solo existe para luego escribir | dentro del mismo método que la escritura, en **una** transacción | los dos pasos del handler se vuelven una llamada |
   | un `INSERT`/`UPDATE`/`DELETE` | `tack-db/src/repo/<agregado>.rs` como función que devuelve `Result<T, StoreError>` | una transacción por comando; `BEGIN IMMEDIATE` si lee antes de escribir |
   | una cadena de estado `"leased"` (solo en 2b) | el enum `ExecutionState` | la conversión texto↔enum ocurre **una vez**, en el borde del store; ningún handler vuelve a ver la cadena |

   Dos conceptos aquí que conviene entender bien:

   **Modelos de lectura (read models).** No todo SQL necesita pasar por "entidades de dominio".
   Un listado para una pantalla es una pregunta a la base de datos; la respuesta es una fila
   plana. Pasarla por un `Item` completo sería trabajo sin valor. Por eso `queries/` existe
   separado de `repo/`: lecturas por un lado, comandos por otro. (Esto es la idea de CQRS en su
   versión más modesta: separar leer de escribir, sin más maquinaria.)

   **`BEGIN IMMEDIATE`.** SQLite tiene un problema conocido: si dos transacciones empiezan
   leyendo y luego intentan escribir, ambas quieren "subir" de lectora a escritora y SQLite
   bloquea una con `SQLITE_LOCKED`. `BEGIN IMMEDIATE` pide el lock de escritura desde el
   principio. `ARCHITECTURE.md` lo explica y el repo ya lo hace en 16 sitios; la receta lo
   generaliza. Al mover un read-then-write a un solo método del store, el lugar donde esto se
   decide pasa de "cada handler" a "el store".

3. *`StoreError`.* Se escribe en 2a y es pequeño:
   ```rust
   pub enum StoreError { NotFound, Conflict(String), Sql(#[from] sqlx::Error) }
   ```
   `ApiError` deja de tener `From<sqlx::Error>` y pasa a tener `From<StoreError>`: `NotFound`
   → 404, `Conflict` → 409, `Sql` → 500. **Y aquí hay un truco de diseño:** al quitar
   `From<sqlx::Error>` de `ApiError` en 2a, cualquier handler que todavía tenga SQL dentro deja
   de compilar. El compilador se convierte en el que hace cumplir la receta en 2b y 2c. No hace
   falta un revisor que busque SQL olvidado; `cargo check` lo encuentra.

4. *Los puertos, solo dos.* En 2a nace el trait `RunnerStore`; en 2b, `RunStore`. Cada uno lo
   implementa `tack_db::Repository` (SQLite) y un `FakeRunnerStore`/`FakeRunStore` en
   `tack-test-support` (un `Mutex<BTreeMap>`). **Un método entra al trait solo si alguna prueba
   necesita el fake para él.** El resto son métodos normales del `Repository`. 2c no añade
   ningún trait: el CRUD de planificación se prueba contra SQLite y está bien así.

   *Lo que aprendes:* esto es lo contrario de "hexagonal de libro", donde todo es un puerto. Aquí
   un puerto se justifica por una prueba concreta que sin él sería lenta o imposible. Si no hay
   esa prueba, no hay puerto.

5. *La suite de contrato.* En `tack-test-support/src/contract.rs`, una macro
   `store_contract!(expr)` que recibe algo que implementa `RunnerStore` y genera la batería de
   pruebas (registrar, revocar, rotar, listar, redimir un token; en 2b, el claim con fencing).
   Se instancia dos veces: sobre SQLite y sobre el fake. Cuando la instancia SQLite pasa, las
   pruebas método a método de `tack-db/tests/repository/` que cubrían lo mismo **se borran en
   el mismo PR**, y el PR las nombra.

   Control de que no se perdió nada:

   ```text
   pruebas_antes − borradas + 2 × casos_del_contrato ≥ pruebas_antes
   ```
   con `cargo nextest list --workspace | wc -l` antes y después.

6. *Las pruebas de `tack-api` que usan SQL directo.* Hay 258 sitios en 31 archivos. Cada uno es
   o una **semilla** (inserta datos para la prueba → pasa a un helper `seed::runner(&pool, …)` en
   `tack-test-support`) o un **assert sobre filas** (→ llama a la función de `queries/` nueva). Al
   final, `grep -rn 'sqlx::' crates/tack-api/tests` debe dar 0.

**Filterset mientras trabajas:** `cargo nextest run --workspace -E 'rdeps(tack-db)'`.

**Estrechar (en 2c).** `scripts/gen-deny-toml.sh` gana una prohibición: `sqlx` solo puede
aparecer en `tack-db` y `tack-test-support`. `cargo deny` es otra herramienta de la familia
"regla verificada": falla el build si alguien añade `sqlx` a otro `Cargo.toml`.

**Cómo verificar (tras 2c).**

```bash
grep -rn 'sqlx::' crates/tack-api/src crates/tack-api/tests crates/tack-orch/src crates/tack-cli/src | wc -l   # 0
grep -rnE '"(leased|preparing|waiting_decision|needs_operator)"' crates/tack-db/src crates/tack-api/src --include='*.rs' | grep -v tests | wc -l   # 0
grep -c 'pub async fn' crates/tack-db/src/repo.rs    # 0: los 27 wrappers que solo reenviaban se fueron
ls crates/tack-db/src/queries/                       # un archivo por tabla con lecturas
make deny                                            # 0, con la prohibición de sqlx activa
cargo nextest run --workspace                        # una vez, sobre el árbol mergeado
.githooks/pre-push
```

**Qué esperar.** `tack-db/src/` tiene `error.rs`, `queries/`, `repo/<agregado>.rs`,
`migrations.rs`. Los handlers de `tack-api` llaman a `state.repo.<método>` o a
`queries::<tabla>::<fn>` y nada más para datos. `tack-test-support` tiene `fake/`, `seed/` y
`contract.rs`.

**Qué aprendiste.** Que el compilador puede hacer cumplir una regla si diseñas los tipos para
ello (quitar un `From` es una decisión de arquitectura). Que lecturas y comandos son cosas
distintas y merecen sitios distintos. Que un puerto se gana con una prueba, no se regala. Y que
"tres PRs con una lista cada uno" es mejor que "un agente con 190 cosas", tanto para humanos
como para modelos.

---

### Fase 3 — `tack-app`: las reglas salen de HTTP

**Qué hacemos.** Creamos `tack-app`, un crate de **casos de uso**: funciones como `StartRun`,
`ClaimAttempt`, `ReportEvents`, `Decide`, `Complete`, que reciben puertos (`RunStore`,
`RunnerStore`, `Clock`) y argumentos planos, y devuelven `Result<T, AppError>`. Los handlers de
`tack-api` quedan en tres líneas: extraer, llamar, responder. Los trabajos de fondo (`merge`,
`auto_start`, `github_sync`) dejan de ser `tokio::spawn` dentro del crate HTTP y pasan a ser
funciones `tick()` que el binario programa.

**Por qué.** Hoy `handlers/executions.rs` tiene 3.023 líneas y `runner_protocol.rs` 2.910. Dentro
hay reglas (qué perfil le toca a un run, si una transición es válida, si una lease está caducada)
mezcladas con HTTP (parsear JSON, devolver 409). Una regla así solo se puede probar levantando un
servidor. Y hay 13 `tokio::spawn` en `tack-api`, lo que significa que la biblioteca HTTP decide
por su cuenta qué hilos de fondo existen: quien la use en un test se lleva los hilos quieran o no.

**El concepto: capa de aplicación.** Entre el dominio puro (`tack-core`, `tack-protocol`) y los
adaptadores (HTTP, SQLite) suele faltar una capa que *orquesta*: "para empezar un run, resuelve
el perfil, valida el brief, encola, y avisa". No es una regla de dominio (no es "qué es un run")
ni es HTTP (no sabe de status codes). Es el caso de uso. En Tack hoy esa capa no existe y por eso
vive en los handlers.

**Cómo.**

1. *Script: lo que ya son servicios se mueve tal cual.* `auto_start.rs`, `autonomy.rs`,
   `merge.rs`, `execution_runtime.rs`, `github_sync.rs`, `rate_card.rs` están en `tack-api` pero
   no son handlers; son servicios que acabaron ahí. `git mv` a `tack-app`, `sed` de rutas,
   `cargo check`. Los errores que salen son de dos tipos: el archivo usa `crate::AppState`, o usa
   un tipo de `axum`. Ambos significan lo mismo: *el servicio está acoplado a HTTP y debe dejar de
   estarlo*. Esa lista es la entrada del agente B.

2. *Agente A — los casos de uso del protocolo* (`runner_protocol.rs`, `decisions.rs`). Para cada
   handler de más de 25 líneas (`awk` los lista), escribe una función en
   `tack-app/src/runs/<verbo>.rs` con la lógica, y deja el handler como extraer-llamar-responder.
   Las claves de idempotencia, los chequeos de fencing y las llamadas a `validate_transition` van
   al caso de uso; el mapeo a status HTTP se queda en `tack-api`.

   `AppError` tiene variantes con significado (`InvalidTransition { from, to }`, `StaleLease`,
   `NotFound`, `Conflict`) y `ApiError` gana un `From<AppError>` que las traduce a los quince
   códigos de error congelados del protocolo runner-v1. Hoy esa traducción está repartida por los
   handlers; pasa a estar en un solo `impl`.

3. *Agente B — los casos de uso del operador y los ticks.* Misma receta para `create`,
   `create_as`, `cancel`, `requeue` de `executions.rs`. Los seis servicios movidos pierden
   `AppState`: cada uno expone `pub async fn tick(puertos…)` que hace **una** pasada, y
   `pub async fn run(puertos, intervalo)` que la repite en bucle. Las pruebas llaman a `tick()`
   con un `FixedClock`; nunca más un `sleep` en un test.

   El runner embebido hoy está en tres sitios (1.304 líneas entre `api/handlers/local_runner.rs`,
   `cli/local_runner.rs`, `cli/local_enrollment.rs`). Pasa a **un** módulo en `tack-app`; los
   otros tres conservan solo rutas y pegamento de `clap`.

4. *Ambos: la raíz de composición.* `crates/tack-cli/src/serve.rs` (nuevo, ~60 líneas) construye
   el `Repository`, los puertos, el `AppState`, lanza cada `tick` con `tokio::spawn` y llama a
   `tack_api::serve`. **Es el único sitio del programa que sabe qué implementación concreta se
   usa para cada puerto.** Eso es una raíz de composición: el lugar donde se "enchufa" todo.

**Dónde sí entra un modelo y por qué.** Convertir un handler de 200 líneas en un caso de uso
requiere decidir qué es regla y qué es HTTP, línea por línea. Eso no lo hace un `sed`. Pero el
agente trabaja sobre una lista (`awk` de handlers largos), con una receta (qué va a cada lado) y
un "hecho cuando" (`grep tokio::spawn crates/tack-api/src` → 0; `grep AppState crates/tack-app`
→ 0; el filterset pasa). Dos agentes, no diez, porque las dos mitades (protocolo y operador) son
independientes y cada una cabe en un contexto.

**Las pruebas.** Una prueba de handler que afirma una *regla* ("una transición inválida se
rechaza", "un replay idempotente devuelve lo mismo") se mueve a `tack-app/tests/` contra los
fakes, sin servidor. Una que afirma un *status code* o una forma JSON se queda en `tack-api`.
Nunca en los dos sitios; el PR lista las que movió. Las pruebas e2e de `tack-cli` siguen
levantando el servidor real: son la demostración de que `serve.rs` compone lo mismo que antes.

**Cómo verificar.**

```bash
grep -rn 'tokio::spawn' crates/tack-api/src | grep -v tests | wc -l       # 0
grep -rln 'axum' crates/tack-app crates/tack-db crates/tack-orch | wc -l   # 0
wc -l crates/tack-api/src/handlers/executions.rs crates/tack-api/src/handlers/runner_protocol.rs
# cada uno < 1 000 (hoy 3 023 y 2 910)
scripts/check-crate-graph.sh && make deny && cargo nextest run --workspace && .githooks/pre-push
```

La tabla del grafo pasa a ser la final (§3 del plan) y `cargo deny` prohíbe `axum` fuera de
`tack-api` y `tack-cli`.

**Qué aprendiste.** Que la capa que suele faltar es la de aplicación, y que su ausencia se nota
porque los handlers engordan. Que un trabajo de fondo debe ser una función `tick()` que hace una
pasada, y el bucle y el `spawn` se ponen fuera, en la raíz de composición. Que una prueba de regla
y una prueba de HTTP son cosas distintas y viven en crates distintos.

---

### Fase 4 — Forma y peso muerto

**Qué hacemos.** Partimos los archivos que una persona no puede leer de una sentada, borramos lo
que el compilador llama "nunca usado", quitamos dependencias que nadie usa, y en el frontend
movemos a `features/` lo que estaba disfrazado de `shared/`.

**Por qué al final y no al principio.** Porque partir archivos genera diffs enormes que
estorbarían a las fases 2 y 3, y porque mucho del código muerto *se vuelve* muerto gracias a
esas fases (wrappers que ya nadie llama, helpers de test que ya no se usan). Limpiar antes sería
limpiar dos veces.

**Por qué no hay modelo en esta fase.** Todo lo que se hace aquí tiene un árbitro determinista:
el compilador (`-D warnings`), `cargo machete`, `npx tsc --noEmit`, `npm test`. Un modelo no
aporta nada que esos no den, y sí puede "mejorar" cosas que nadie pidió mejorar.

**Cómo.**

1. *`cli/main.rs` (3.054 líneas).* Los grupos ya existen: son los brazos del `match` sobre
   `Commands`. El script lista las funciones con `ast-grep` y las corta por rango a módulos
   `cmd/board.rs`, `cmd/entities.rs`, `cmd/backup.rs`, `cmd/runs.rs`. `main.rs` conserva el
   `enum Commands`, `main` y el `match`. Meta: < 600 líneas.

2. *`runner/engine.rs` (2.611 líneas).* Mismo método, cortando por bloque `impl`. Meta: < 1.200.

   *Lo que aprendes:* "partir un archivo" no es una decisión de diseño si el archivo ya tiene
   costuras (`match`, `impl`). Es un corte mecánico. Si no las tiene, partirlo sin pensar sería
   peor que dejarlo; por eso el plan dice que lo que no se corte limpio se deja y se anota.

3. *Código muerto.* Hay 50 `#[allow(dead_code)]` / `#[allow(unused…)]`. Cada uno es una promesa
   de "esto se usará" que nunca se cumplió, o un helper de test que sobrevivió a su test. El
   script los borra todos, deja que `clippy` liste lo que ocultaban, y se borra eso. Si algo se
   usa solo en tests, lleva `#[cfg(test)]`, no un `allow`.

4. *Dependencias sin uso.* `cargo machete` las nombra; se borran de los `Cargo.toml`. Si nombra
   una que sí se usa bajo un feature, se declara en `[package.metadata.cargo-machete] ignored`
   (falso positivo conocido).

5. *Frontend.* `shared/runWithAgent` (5.590 líneas), `shared/execution` y `shared/agents` son
   features, no código compartido. `git mv` a `features/runs/`, `sed` de imports,
   `npx tsc --noEmit` dice qué quedó roto. Hay tres archivos en `shared/` que importaban de
   `features/` (la dirección prohibida); se mueven o se invierte la dependencia pasando el valor
   como argumento.

6. *El test de arquitectura del frontend* (`src/architecture.test.ts`) hoy comprueba que una
   feature no importa de otra. Gana dos casos: que `shared/` no importa de `features/`, y que los
   imports con alias `@/` también cuentan. Es la versión frontend del script del grafo.

**Cómo verificar.**

```bash
grep -rc 'allow(dead_code)\|allow(unused' crates --include='*.rs' | grep -v ':0' | wc -l   # 0
cargo machete                                                                              # "no unused dependencies"
wc -l crates/tack-cli/src/main.rs crates/tack-runner/src/engine.rs                        # < 600, < 1 200
grep -rl "features/" frontend/src/shared --include='*.ts*' | wc -l                        # 0
cargo nextest run --workspace && (cd frontend && npm test && npx playwright test) && .githooks/pre-push
```

**Qué aprendiste.** Que la limpieza va al final, cuando el refactor ya ha dejado claro qué sobra.
Que `allow(dead_code)` es deuda con nombre. Y que las reglas del frontend se verifican igual que
las del backend: con un test que lee el árbol de archivos.

---

### Fase 5 — Los documentos dicen lo que dice el árbol

**Qué hacemos.** Reescribimos las secciones de estructura de `docs/ARCHITECTURE.md` y la lista de
crates del Developer Guide, y sustituimos los números escritos a mano ("92 migraciones", "96
rutas") por un script que los calcula.

**Por qué.** `ARCHITECTURE.md` hoy dice que `tack-orch` "depende hacia adentro de core y db", que
es exactamente lo contrario de lo que debería ser un dominio. Y lleva conteos que él mismo
advierte que no son de fiar. Un documento que miente es peor que ninguno: alguien lo lee, confía,
y diseña encima.

**Cómo.** Un script `scripts/arch-facts.sh` imprime crates, rutas, migraciones, pruebas y aristas.
Un agente, en una pasada, reescribe las dos secciones a partir de §3 del plan y de esa salida, y
cambia cada número literal por "ver `scripts/arch-facts.sh`". Borra la frase sobre `tack-orch`.
Añade la línea del `CHANGELOG.md` para la fase entera.

**Cómo verificar.** `scripts/check-comments.sh` pasa, `mdbook build docs/book` pasa, y
`grep -n '92 migrations\|96 documented paths\|100% complete' docs/ARCHITECTURE.md` no encuentra
nada.

**Qué aprendiste.** Que un número en un documento es una promesa de mantenimiento. Si puede
calcularse, que lo calcule un script y el documento diga cómo.

---

## Parte III — Lo que te llevas

Si solo recuerdas cinco cosas de esta fase, que sean estas:

1. **Mide antes de proponer.** El `grep` de los `use` en `tack-orch/execution` convirtió "sacar
   el protocolo a un crate" de un proyecto de semanas en un `git mv`. Casi siempre hay una
   medición de dos minutos que cambia el tamaño del problema.

2. **La dirección de las dependencias es la arquitectura.** Lo demás son carpetas. Y una
   dirección que no verifica un script es un deseo.

3. **Una abstracción se gana con una prueba.** Un trait sin un fake que lo use es ceremonia.
   Un crate que no cambia el conjunto de dependencias ni una frontera es una carpeta con
   `Cargo.toml`.

4. **Script lo que el compilador puede verificar; modelo solo lo que exige elegir.** Y cuando
   uses un modelo: dale una lista que no hizo él, una receta cerrada y un comando que diga
   "hecho". Seis agentes para un refactor de este tamaño es lo normal, no veinte.

5. **Los tests viajan con el código y nunca conviven dos versiones.** Mover con `git mv`, borrar
   lo viejo en el mismo PR que hace pasar lo nuevo, correr solo `rdeps(<crate>)` mientras
   trabajas y la suite completa una vez por fase.

Cuando termines, el árbol tendrá nueve crates, un script en el hook que impide que vuelvan a
cruzarse, y un runner que compila sin saber que existe una base de datos. Eso no es "hexagonal";
es algo más útil: un programa donde cada pieza sabe exactamente de qué puede depender, y hay
una herramienta que lo comprueba cada vez que haces push.
