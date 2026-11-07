# prill visto desde Tack: qué ajustar en Tack, qué pedirle a prill y qué investigar

**Fecha:** 2026-11-06. **Para:** el mantenedor de prill y el de Tack (hoy la misma persona). **Estado:**
notas; ninguna tarjeta de Tack espera por nada de aquí, y prill nunca bloquea una (Tack negocia y, si
falta algo, asume lo más estricto).

**De dónde sale:** un inventario de lectura de prill en `6b95423` (`~/Sites/prill`), hecho por un agente
con citas `archivo:línea`, cruzado con lo que Tack hace en `crates/tack-runner/src/verify/prill.rs`,
`crates/tack-api/src/prill.rs` y `prill_store.rs`. Lo marcado **[prill]** es del inventario y no lo
volví a abrir; lo marcado **[tack✓]** lo comprobé en el código de Tack. No ejecuté nada.

**Documentos hermanos:** la auditoría (`docs/audits/evals-y-guardrails-2026-11-06.es.md`) y el plan
(`docs/plans/evals-y-guardrails-2026-11-06.es.md`, Phase 94 en el roadmap). Este documento es la
respuesta a la pregunta 11 de su §11.

---

## 1. Veredicto en una pantalla

prill hace más de lo que Tack le deja mostrar. Detecta tests tautológicos, tests borrados y cambios en la
configuración de tests o del linter; firma el pack entero; calcula su propio acuerdo con el destino del
PR. Tack aplana casi todo eso a líneas de texto, no le manda la etiqueta más valiosa que tiene (la
revisión de la persona), y en tres de los cuatro harnesses no le dice de qué familia es el modelo que
hizo el trabajo, con lo que, si hay juez, **todo sale `human-review`**.

| | Lo que prill ya da | Lo que Tack hace con ello |
| --- | --- | --- |
| Trampas en los tests | `tautological_test`, `tests_deleted`, `test_config_changed`, `analysis_suppressed`, `dependencies_changed`, `config_changed_in_head`, todos flags de revisión [prill] | Texto en `risk.reasons`; el tablero no los lee [tack✓] |
| Firma | in-toto sobre el MRP entero, atado al `headCommit` [prill] | Verifica la firma y luego lee la decisión de otro archivo [tack✓] |
| Acuerdo | `agreement`, `escapedDefects`, `qualified` a partir de `record outcome` (merged, closed, reverted, red_ci) [prill] | Le manda los outcomes del PR; **no** le manda la revisión humana, que prill no acepta [tack✓] |
| Familia del modelo | `same_family` y `family_undeclared` degradan el juez [prill] | Solo docket escribe `family` en los pasos; Claude Code, Codex y opencode no [tack✓] |
| Juez | `yes`, `no`, `unparseable`; ciego al razonamiento del agente [prill] | `unparseable` se muestra como Fail, igual que `no` [tack✓] |
| Análisis estático | Por herramienta y por árbol [prill] | `static_analysis: None` siempre [tack✓] |

---

## 2. Ajustes del lado de Tack (no necesitan a prill)

Son pequeños, de Sonnet o Haiku, y entran en Phase 94 donde se indica.

| # | Ajuste | Dónde | Entra en |
| --- | --- | --- | --- |
| T1 | **Leer los `flags` como estructura** y llevarlos a `Hold::ChecksChanged`; la lista propia de Tack queda para los runs sin pack | `verify/prill.rs:630-640`, `tack-protocol/src/mrp.rs`, `tack-core/src/autonomy.rs` | GR9 |
| T2 | **La decisión se lee del predicado firmado** (prill firma el MRP entero); si no coincide con el pack traducido, retener | `tack-api/src/prill.rs::attest_verify`, `tack-app/src/autonomy.rs:300-360` | GR10 (a) |
| T3 | **La familia del modelo en la evidencia de los cuatro harnesses.** Hoy solo `docket/steps.rs:51` la escribe; `verify/prill.rs:465-475` la busca en el último paso del implementador y, si no está, prill marca `family_undeclared` y el juez queda `degraded` → `human-review` siempre. Claude Code devuelve el modelo en su línea `result`; Codex y opencode también lo saben. Escribir `family` en los pasos de los tres (o en `generator.family` directamente) | `harness/{claude_code,codex,opencode}.rs`, `evidence.rs` | Phase 94, tarjeta P1 (ronda 1) |
| T4 | **`unparseable` no es `no`.** Mostrarlo como "sin respuesta" y no como Fail del criterio | `verify/prill.rs:714-740` | GR9 o C0-1 (es una línea) |
| T5 | **`staticAnalysis` al pack**, al menos los totales por herramienta, en vez de `None` | `verify/prill.rs:801` | Opcional; cuando alguien lo necesite en pantalla |
| T6 | **Mandar la revisión humana como outcome** cuando prill la acepte (P1 de §3); hasta entonces, el acuerdo pack × revisión lo calcula Tack en sus métricas y prill sigue con el suyo por PR. Dos cifras distintas, con nombres distintos, sin duplicar: Tack mide contra la persona, prill contra el PR | `prill_store.rs`, EDD-2 | EDD-2 |
| T7 | **Nombres de etapa.** Las etapas exigidas por proyecto (`settings.required_stages`) tienen que coincidir con los nombres que prill emite (`checkout`, `criteria`, `anti_tautology`, `suite`, `coverage`, `mutation`, `static_analysis`, `complexity`, `judge`, `risk`). Comprobar que la UI de Settings → Automation ofrece esos y no otros, y que `required_stage_missing` trata `not_applicable` como hoy (cuenta como no corrida) | `handlers/autonomy.rs`, `tack-protocol/src/mrp.rs:243-352` | Investigar (I4) y, si hace falta, C0-1 |

---

## 3. Pedidos a prill (ajustar), por valor para Tack

Ninguno bloquea. Cada uno dice qué haría Tack cuando exista y cómo lo detecta (negociado, como con
docket).

| # | Pedido | Por qué | Qué hace Tack cuando exista |
| --- | --- | --- | --- |
| P1 | **Un outcome con la revisión de la persona**: `kind: accepted \| rejected`, con `reason` y `source: tack-review`, además de los cuatro de hoy. Y en `metrics/v1`, **falsa aprobación** y **falso rechazo** como cifras separadas (hoy `agreement` cuenta auto-merge→merged y reject→closed como acuerdo, y `escapedDefects` lo demás) | Es la etiqueta que llega **antes** del merge y la única que dice por qué. Sin ella prill solo se mide contra el PR, que llega tarde y sin motivo | `prill_store.rs` manda la revisión como outcome la primera vez que `prill schema outcome` acepte el `kind`; la página de métricas muestra las dos cifras de prill junto a las de Tack |
| P2 | **Una sonda de capacidades**: `prill features --json` (o `capabilities`) con la versión, los `kind` de outcome, los códigos de flag y las etapas que emite | Tack negocia con docket por `features` y con prill no puede: hoy adivina por la versión y por `prill schema NAME` | El probe del runner la lee al arrancar; lo que falta se muestra como "este prill no lo da" en vez de fallar |
| P3 | **`record mrp --attestation` verifica la firma**, no solo la forma del envelope [prill, `app/record.py:42-45`] | Un pack firmado por otra clave entra en el store como si fuera válido | Nada que cambiar en Tack; es una garantía del store |
| P4 | **Un tercer valor del juez, `unknown`** (se abstiene), distinto de `no` y de `unparseable`, con su propio efecto (`degraded`, no `failed`) | Es la práctica actual para jueces (sí / no / no sé); hoy una duda se lee como un no | T4 lo muestra como "sin respuesta" |
| P5 | **Opción de correr los criterios con los tests del commit base**: `criteria.tests_from: base`, igual que el overlay que ya usa `anti_tautology` (código de head, tests de base) | Hoy todo criterio corre sobre el árbol de head: un agente que edita un test o un script de `package.json` cambia su propio examen. prill ya tiene la mitad de la maquinaria | Tack lo pide en `prill.yaml` de sus proyectos de prueba y el suite `verifier` mide la diferencia de falsa aprobación |
| P6 | **Que algunos flags puedan bloquear por configuración**: `block: [tautological_test, tests_deleted, test_config_changed]` | Hoy los quince flags son revisión; en un proyecto en L4 una trampa detectada no debería dejar `auto-merge-eligible` ni siquiera cuando el resto está verde (Tack lo retiene por su lado con GR9, pero la dueña natural es prill) | Si existe, Tack deja de duplicar la retención y solo muestra la de prill |
| P7 | **El flag `prepare_ran_head_code`** que la ADR 0013 de prill deja abierto: `prepare.command` corre código de head | Es el mismo problema que Tack tiene con su instalación (GR6): código del agente corriendo antes de verificarlo | Se muestra como un riesgo más del pack |
| P8 | **Un juez con proveedor Anthropic nativo**, o documentar el shim: hoy el juez habla solo con un endpoint `/chat/completions` compatible con OpenAI [prill, `edges/judge.py:69-93`] | Tack corre sus evals con Claude Haiku 5.5 por la API de Anthropic; para que el juez de prill sea de **otra familia** que el implementador y a la vez use la clave que ya hay, hace falta un camino directo | Se configura en `prill.yaml` de los proyectos de prueba |
| P9 | **Versiones y etiquetas**: `0.1.0` en `pyproject.toml`, sin tags en git, con un `Unreleased` M6 y specs con su propia numeración ("since 0.5.0") | Tack nombra a prill por versión en sus docs y no puede fijar un mínimo honesto | Cuando haya tag, Tack escribe "prill `X` o posterior" como hace con docket |

---

## 4. Investigar (preguntas abiertas, cada una con cómo responderla)

| # | Pregunta | Cómo se responde | Si la respuesta es la mala |
| --- | --- | --- | --- |
| I1 | **¿Los runs de Claude Code, Codex y opencode salen siempre `human-review` cuando hay juez?** Por `family_undeclared` (T3) | Correr `prill verify` sobre un pack real de un run de Claude Code del operador y mirar `stages[judge].status` y `reasons` | T3 pasa de "ajuste" a defecto de Phase 94, ronda 1 |
| I2 | **¿Qué sandbox usa prill en la máquina del operador?** La suite live pasa `--image prill-verify-py313:local`; el runner de producción puede no tener la imagen y prill entonces exige `--no-sandbox`, que añade el flag `no_sandbox` (revisión) | Leer `[verify] args` del `runner.toml` del servicio `tack` y `docker images` | Documentar en la guía de administrador que sin imagen todo run es `human-review`, o construir la imagen en `make desktop` |
| I3 | **¿Prill lee la trayectoria que Tack le pasa por URI y digest?** Tack escribe `trajectory` con una URL del tablero y un sha [tack✓]; el pack tiene `attempt.trajectory` y `claims` | Buscar en prill dónde se consume `attempt.trajectory` y si hace la petición HTTP; si la hace, con qué credencial (la URL del tablero exige token) | Mandar el archivo por ruta local en vez de URL, o quitar el campo |
| I4 | **¿Los nombres de etapa que Tack exige coinciden con los de prill?** (T7) | `rg required_stages crates frontend/src` y compararlo con `src/prill/core/stages/registry.py` | Una migración que renombra y una lista cerrada en la UI |
| I5 | **¿El `test` criterion funciona sin `tests.case_command`?** prill lo deja `not_run` sin esa config; Tack traduce `kind: test` sin saber si el proyecto la tiene | Mirar los packs reales: cuántos criterios `test` salieron `not_run` | La skill de onboarding escribe `tests.case_command` en `prill.yaml` cuando el repo lo permite, o Tack convierte `test` en `command` |
| I6 | **¿Cuánto cuesta prill por run, en tokens y segundos?** `verificationTax` lo da prill por repositorio; Tack lo recoge en el impuesto de verificación | Leer `GET /api/metrics` del operador tras una semana | Si el juez pesa más que el implementador, el juez pasa a opcional por proyecto |
| I7 | **¿`qualified` de prill (ventana de 14 días) y la aceptación de Tack cuentan lo mismo?** La ADR 0014 L-6 y `core/metrics.py:155-163` difieren en la letra, aunque coincidan en la práctica [prill] | Comparar las dos cifras sobre los mismos packs en el store del operador | Nombrarlas distinto en la página de métricas y decir qué mide cada una |
| I8 | **¿La redacción de prill y la de Tack coinciden?** prill redacta el diff que manda al juez; Tack redacta la salida del runner. Un secreto que Tack conoce y prill no, o al revés, llega al otro lado | Sembrar un secreto en un repo de prueba y mirar el pack y los eventos | Pasar a prill la lista de valores a tachar por un archivo privado (pedido P10, si hace falta) |
| I9 | **¿Publica docket si encierra a sus agentes?** (pedido a docket, GR8) Tack lo daba por hecho (`sandbox: harness`) y ahora lo pregunta. Se pide un añadido negociado: `isolationPosture` en la lista `features` de `GET /health` y, junto a ella, la clave `isolationPosture` con `jailed` (docket monta la jaula) o `none` (corre sin ella). No bloquea nada: sin el dato, o con `none`, el probe de Tack dice `sandbox: none` con su motivo y el diálogo de ejecución y el arranque automático piden `ask` antes de cada acción [tack✓] | Leer en el código de docket dónde monta la jaula de un agente y qué la desactiva (un pod sin `codebase`, una imagen que falta, un permiso); `curl` a `/health` de un docket real y mirar si lista `isolationPosture` | Mientras docket no lo publique, sus runs van en `ask`. Si docket solo a veces encierra, que publique `none` (lo estricto) |

---

## 5. Lo que **no** se le pide a prill

- Que calibre un juez: las etiquetas humanas están en Tack, y Phase 94 deja el juez calibrado como
  "futuro, no planeado". Si algún día se hace, Tack le pasa las etiquetas; prill no las inventa.
- Que corra en CI de Tack: la regla sigue siendo que la CI de Tack no necesita ni docket ni prill.
- Que mida a los equipos de docket: eso lo mide docket con su propio plan de evals.
- Cifras en dólares.

## 6. Orden sugerido

1. **I1 e I2** (una tarde, sin código): deciden si T3 e I2 son defectos abiertos hoy en la instalación
   del operador.
2. **T1, T2, T3, T4**: entran en Phase 94 por las tarjetas que se indican.
3. **P1 y P2**: son los dos pedidos que cambian lo que Tack puede mostrar. Se abren en prill como
   issues con el texto de §3 y se negocian por versión.
4. El resto, cuando una medición lo pida.
