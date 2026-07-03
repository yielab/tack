# Alexa Voice Integration

Tack can receive commands from an Amazon Alexa custom skill via
`POST /api/alexa`. You can add tasks, list open work, and complete items by
voice — using the same workflow validation (transitions, WIP limits,
auto-parent-completion) as the web UI, and broadcasting the same WebSocket
events so open boards update in real time.

The feature is **off by default**: the endpoint returns `404` until a skill ID
is configured.

## What you can say

| You say | Intent | Effect |
| ------- | ------ | ------ |
| "add a task called buy cement" | `AddTaskIntent` | Creates an item at the workflow's initial status |
| "add a task called buy cement in project casa" | `AddTaskIntent` | Same, in the named project |
| "what are my open tasks" | `ListTasksIntent` | Speaks the open-item count and the first few titles |
| "complete the task buy cement" | `CompleteTaskIntent` | Moves the matching open item to the first Done status |

Notes:

- Without a `project` slot, the **most recently updated** project is used. The
  response always speaks the project name so you know where the action landed.
- Project matching is case-insensitive, exact first, then substring
  ("casa" matches "Casa Nueva").
- Responses use the project's vocabulary — a construction project says
  "Added Work Order buy cement to Casa."
- Responses are **bilingual**: spoken in Spanish for `es-*` request locales,
  English otherwise.
- Completing an item honours explicit workflow transitions and WIP limits; if
  the move is not allowed, Alexa explains why instead of doing it.

## Server configuration

Set the skill ID (found in the Alexa developer console, `amzn1.ask.skill.…`):

```bash
TACK_ALEXA_SKILL_ID=amzn1.ask.skill.xxxx-xxxx cargo run -p tack-cli -- serve
```

or in `tack.toml`:

```toml
alexa_skill_id = "amzn1.ask.skill.xxxx-xxxx"
```

The skill endpoint must be a public **HTTPS** URL that reaches your server,
e.g. `https://your-domain.example/api/alexa` behind the Caddy reverse proxy.

If a `cloudflared.yml` exists in the repo root (gitignored, machine-specific),
`make run` and `make dev` automatically start a Cloudflare named tunnel
alongside the app, so the public endpoint comes up with a single command.
`make tunnel` starts only the tunnel.

## Security model

- **Shared-secret gate (recommended — the real authentication)** — set
  `TACK_ALEXA_SHARED_SECRET` to a random, URL-safe string and append it to the
  skill's endpoint URL as a `?token=` query parameter
  (`https://your-domain.example/api/alexa?token=<secret>`). When configured,
  every request must carry a matching token (compared in constant time) or it
  is rejected with `403`. Alexa always POSTs to *exactly* the URL you configure
  in the developer console, including its query string, and it is the only
  channel Alexa lets you attach a secret to — custom request headers are not
  configurable on an HTTPS skill endpoint. **Set this.** Without it the endpoint
  is authenticated only by the skill ID, which is *not* a secret and is
  therefore forgeable by anyone who knows it.
- **Skill ID verification** — every Alexa request embeds the skill's
  `applicationId`; it is compared against the configured value in constant
  time. Mismatch → `403`. This is a useful sanity check but, on its own, not a
  secret.
- **Timestamp check** — requests older (or newer) than 150 seconds are
  rejected (`400`), preventing replays. This is Alexa's own tolerance window.
- **Bearer-token exemption** — Alexa cannot send an `Authorization` header, so
  `/api/alexa` is exempt from the `TACK_API_TOKEN` gate; the shared-secret
  query parameter is its equivalent.

```bash
# Enable the endpoint AND lock it to a secret only you and the skill know:
TACK_ALEXA_SKILL_ID=amzn1.ask.skill.xxxx-xxxx \
TACK_ALEXA_SHARED_SECRET=$(openssl rand -hex 24) \
  cargo run -p tack-cli -- serve
```

If `TACK_ALEXA_SKILL_ID` is set but `TACK_ALEXA_SHARED_SECRET` is not, the
server logs a prominent security warning at startup and keeps the endpoint open
in the legacy skill-ID-only mode (for backward compatibility) — but that mode is
forgeable and should not be used for anything but throwaway local testing.

### Why a shared secret and not X.509 signature validation?

Amazon's certification program requires validating each request's
`SignatureCertChainUrl` + `Signature` against Amazon's X.509 certificate chain
(RSA-SHA1 over the raw body). Pulling a full pure-Rust X.509/RSA verification
stack into Tack was judged too heavy for the single-binary size budget (~10 MB),
and getting chain-of-trust verification subtly wrong is itself a security risk.
The mandatory shared-secret query parameter is a lightweight, auditable
alternative that makes the endpoint genuinely unforgeable by anyone who does not
know the secret. If you later publish a *certified* store skill, full signature
validation would be the remaining piece to add. Either way, always serve the
endpoint over HTTPS.

## Skill setup (Alexa developer console)

1. Create a **Custom** skill, any invocation name (e.g. "flex manager").
   Avoid single-letter words like "flex p m" — Alexa matches them poorly and
   the skill may simply never be invoked.
2. Under *Endpoint*, choose **HTTPS** and enter
   `https://your-domain.example/api/alexa?token=<your-shared-secret>` (the
   `?token=` must match `TACK_ALEXA_SHARED_SECRET` — see *Security model* above).
3. **SSL certificate type matters.** Check what your certificate actually
   covers: if it lists your exact hostname, pick *"…has a certificate from a
   trusted certificate authority"*; if it covers you via a **wildcard**
   (`*.your-domain.example` — this is the case behind Cloudflare, ngrok, and
   most reverse proxies), you MUST pick *"…is a sub-domain of a domain that
   has a wildcard certificate"*. Choosing the wrong one makes Amazon reject
   the TLS connection — the simulator only shows a generic "I can't reach the
   skill" error, while a real device reports the actual
   "SSL certificate verification failed … uses a wildcard domain name" message.
4. Paste this interaction model in the JSON editor:

```json
{
  "interactionModel": {
    "languageModel": {
      "invocationName": "flex manager",
      "intents": [
        { "name": "AMAZON.CancelIntent", "samples": [] },
        { "name": "AMAZON.HelpIntent", "samples": [] },
        { "name": "AMAZON.StopIntent", "samples": [] },
        { "name": "AMAZON.FallbackIntent", "samples": [] },
        {
          "name": "AddTaskIntent",
          "slots": [
            { "name": "title", "type": "AMAZON.SearchQuery" },
            { "name": "project", "type": "AMAZON.SearchQuery" }
          ],
          "samples": [
            "add a task called {title}",
            "add {title}",
            "create a task called {title}",
            "new task {title}"
          ]
        },
        {
          "name": "ListTasksIntent",
          "slots": [
            { "name": "project", "type": "AMAZON.SearchQuery" }
          ],
          "samples": [
            "what are my open tasks",
            "list my tasks",
            "what's left to do",
            "list tasks in project {project}"
          ]
        },
        {
          "name": "CompleteTaskIntent",
          "slots": [
            { "name": "title", "type": "AMAZON.SearchQuery" },
            { "name": "project", "type": "AMAZON.SearchQuery" }
          ],
          "samples": [
            "complete the task {title}",
            "mark {title} as done",
            "finish {title}"
          ]
        }
      ]
    }
  }
}
```

> Alexa does not allow two `AMAZON.SearchQuery` slots in one sample utterance,
> which is why the add/complete samples carry only `{title}`. To target a
> project by voice, either add a custom slot type with your project names, or
> rely on the most-recently-updated default.

5. Build the model (wait for "Build Successful"), then test in the *Test*
   tab: "ask flex manager to add a task called water the plants", or
   "open flex manager" followed by "add a task called water the plants".

### Spanish interaction model (es-ES / es-MX / es-US)

To use the skill in Spanish, add a Spanish language to the skill first
(*Build → Language settings → Add new language*), then paste this model into
the JSON editor **while that locale is selected** and build it:

```json
{
  "interactionModel": {
    "languageModel": {
      "invocationName": "flex manager",
      "intents": [
        { "name": "AMAZON.CancelIntent", "samples": [] },
        { "name": "AMAZON.HelpIntent", "samples": [] },
        { "name": "AMAZON.StopIntent", "samples": [] },
        { "name": "AMAZON.FallbackIntent", "samples": [] },
        {
          "name": "AddTaskIntent",
          "slots": [
            { "name": "title", "type": "AMAZON.SearchQuery" },
            { "name": "project", "type": "AMAZON.SearchQuery" }
          ],
          "samples": [
            "agrega una tarea llamada {title}",
            "añade una tarea llamada {title}",
            "crea una tarea llamada {title}",
            "nueva tarea {title}",
            "agrega {title}"
          ]
        },
        {
          "name": "ListTasksIntent",
          "slots": [
            { "name": "project", "type": "AMAZON.SearchQuery" }
          ],
          "samples": [
            "cuáles son mis tareas pendientes",
            "lista mis tareas",
            "qué tareas tengo abiertas",
            "qué me falta por hacer",
            "lista las tareas del proyecto {project}"
          ]
        },
        {
          "name": "CompleteTaskIntent",
          "slots": [
            { "name": "title", "type": "AMAZON.SearchQuery" },
            { "name": "project", "type": "AMAZON.SearchQuery" }
          ],
          "samples": [
            "completa la tarea {title}",
            "marca {title} como hecha",
            "marca como terminada {title}",
            "termina la tarea {title}"
          ]
        }
      ]
    }
  }
}
```

Example invocations: "Alexa, pídele a flex manager que agregue una tarea
llamada comprar cemento", "Alexa, pregúntale a flex manager cuáles son mis
tareas pendientes".

> **Note:** responses follow the request's `locale` field — any `es-*` locale
> is answered in Spanish ("Agregué Tarea comprar cemento a Casa."), everything
> else in English. No configuration needed; build the model in both languages
> and each device answers in its own.

## Trying it without a device

You can exercise the endpoint with `curl` (timestamp must be current):

```bash
curl -s https://tack.test/api/alexa \
  -H 'Content-Type: application/json' \
  -d '{
    "version": "1.0",
    "session": { "application": { "applicationId": "amzn1.ask.skill.xxxx-xxxx" } },
    "request": {
      "type": "IntentRequest",
      "requestId": "req-1",
      "timestamp": "'"$(date -u +%Y-%m-%dT%H:%M:%SZ)"'",
      "intent": {
        "name": "AddTaskIntent",
        "slots": { "title": { "name": "title", "value": "water the plants" } }
      }
    }
  }'
```

Response:

```json
{
  "version": "1.0",
  "response": {
    "outputSpeech": { "type": "PlainText", "text": "Added Task water the plants to Casa." },
    "shouldEndSession": true
  }
}
```
