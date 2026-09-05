# ADR 0063: A harness is credentialed one of two ways, and a provider is a module

**Decide:** approve that every harness on a runner is credentialed in exactly one of two
modes. Either it uses **the person's own subscription** — it logs in by itself, Tack holds
no credential, the plan decides which models exist and there is no per-attempt cost — or it
uses **an API key and an endpoint** that Tack's runner holds and hands it. A gateway
(Vercel AI Gateway, LiteLLM, OpenRouter) and a vendor's own API (`api.anthropic.com`,
`api.openai.com`) are the same mode. Approve further that a provider in that second mode is
**a module behind one trait** — it knows its own endpoints and how to read its own catalog —
so that adding the next one is writing one module, not editing the code that calls them.

**Why now:** VI-B2 is adding the first endpoint-and-key path. Written the obvious way it
spells `vercel` into the config type, the discovery step, the `doctor` output and each
harness adapter — so the second endpoint means a second pass through all four, and the
third means a third. Deciding the shape while there is exactly one implementation is the
difference between writing one module and rewriting a layer.

**If you do nothing:** every future endpoint costs an adapter change per harness; the
snapshot keeps conflating "which program ran", "who paid" and "which model" in one string;
and choosing a model stays guesswork, because Tack holds its id and nothing else — not its
price, not its context window, not whether it can even accept an image.

## The decisions, in short

| # | Decision | Why |
|---|---|---|
| 1 | Two modes, chosen per harness per runner: **subscription** or **key + endpoint**. There is no third. Subscription is the default and is exactly today's behaviour — nobody is migrated. | These are the only two things that actually differ. Everything else is a value inside one of them. |
| 2 | A gateway and a vendor's own API are **one mode, not two**. | `api.anthropic.com` is a base URL with a bearer key, exactly like Vercel's. Treating "direct" as its own case doubles the work for no gain. |
| 3 | A harness declares **which wire it can be pointed at**, never which vendors it supports. | claude-code takes an Anthropic-wire endpoint through environment variables; codex takes an OpenAI-responses endpoint through invocation flags. Those are properties of the harness; vendors are properties of the endpoint. Adding a harness stays one declaration. |
| 4 | **A provider is a module behind one trait.** It knows its own endpoints per wire, and it knows how to read its own catalog into the shape Tack stores — models, prices, limits, whatever else that provider publishes. Adding OpenRouter is a new module and one registry line; no vendor name appears in the machinery that calls the trait. | Providers do not share a catalog format, an auth header or a pricing shape, so a config table cannot describe them and pretending otherwise pushes the differences into the caller. Putting each provider's differences inside its own module is what keeps adding the next one simple. |
| 5 | **The provider's own catalog is the source** of which models it offers, their quoted prices, and their limits — stored per model, as published, parsed by that provider's module into one common shape. | The provider already knows. Anything Tack maintains by hand goes stale, and a hand-kept list is how you promise a model that fails at claim. |
| 6 | **A quoted price is never a measured spend.** They are separate values that never merge, and a quote is never used to fill in a cost that was not measured. | A catalog price is what a vendor advertises; what an attempt cost is what its harness reported. Blurring them turns an estimate into a receipt. |
| 7 | **What an endpoint does not publish is null, not zero and never inferred.** | Measured: 101 of 373 models publish no context window and 21 publish no price. Rendering those as `0` would be a lie in the direction that hurts — a free model with no limit. |
| 8 | **opencode is removed from the tree** — adapter, tests, fixtures and documentation. Tack supports claude-code and codex. | It cannot state which model served a request, it is the only harness needing a written config file rather than per-spawn injection, and it is environment-sensitive in ways the other two are not. Keeping it means carrying code that cannot satisfy decisions 3, 5 and 6. |

If you accept this table, you have accepted the ADR — record the date at the bottom.
Everything past this point is supporting detail for whoever implements or later audits one
of these calls; nothing above depends on anything below it.

---

- **Status:** accepted 2026-09-05 — the user approved the two credential modes and
  directed the provider-as-a-module shape that replaced decision 4. Recorded in the
  amendment of that date at the bottom of this file.
- **Date:** 2026-09-04
- **Relationship to earlier ADRs:** refines ADR 0061
  (`0061-provider-credentials-at-the-runner-boundary.md`), which established that a runner
  may hold a provider credential and the server never may. That boundary is unchanged. This
  ADR names the two modes it produces and stops the second endpoint from costing what the
  first did. ADR 0050 and 0058 are unchanged in substance: the *server* still never proxies
  a model provider.
- **Wire contract:** **decision 5 requires a change.** Today a `ModelCombination` carries a
  provider and a bare list of model ids, with nowhere to put a price, a context window or a
  modality. Per-model metadata therefore needs a reviewed field on the wire type and a
  fixture revision — it must not be smuggled through the `additional` map, which is
  explicitly forbidden as a contract change without a review. Decisions 1–4 and 6–8 need no
  contract change; harness identity travels as an opaque string and no fixture names
  opencode, so decision 8 changes no fixture byte.

## What was measured, and when

Everything below was measured on 2026-09-04 against the installed binaries and a live
gateway, not read from a vendor page. Where a vendor's documentation and the measurement
disagree, both are recorded.

### The three harnesses do not differ the way the vendor pages suggest

| Harness | How an endpoint is supplied | Measured |
|---|---|---|
| claude-code | Environment variables: `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN` | Works. The base URL takes no `/v1` suffix — adding one produces a real 404 that the CLI then reports misleadingly as "There's an issue with the selected model". |
| codex | Invocation flags: `-c model_provider=…`, `-c model_providers.<name>.base_url=…`, `.env_key=…`, `.wire_api=…` | Works, **per invocation**, with no matching section on disk. This is what makes decision 3 affordable: the runner never edits the person's own `~/.codex/config.toml`. |
| opencode | A written `opencode.json` plus an npm package (`@ai-sdk/gateway`) | Works, but only by writing a file into the workspace. It is the only one of the three that cannot be credentialed by per-spawn injection alone. |

### What a catalog actually publishes, and why decision 7 exists

One endpoint's catalog (`GET /v1/models`) returned 373 models. Per model it publishes an
id, and variously: `context_window`, `max_tokens`, `pricing`, `modalities`,
`supported_parameters`, `knowledge` cutoff, and data-retention flags.

Two measurements shape decisions 5 and 7:

- **Pricing is not a pair of numbers.** Across the catalog, `pricing` takes 24 distinct key
  shapes — `input` and `output`, but also `input_cache_read`, `input_cache_write`,
  `input_tiers`, `output_tiers`, `service_tiers`, `regional`, `peak_pricing`,
  `web_search`, per-second audio and video rates, and a literal `varies_by_provider`.
  Normalising that into a typed input/output struct would silently falsify a large minority
  of the catalog. It is stored as published, and read by whatever understands the shape it
  finds.
- **Coverage is partial.** 352 of 373 models publish a price and 272 publish a context
  window. The remaining ones publish neither zero nor a default — they publish nothing, and
  that is what Tack must store and render.

### Vercel documents neither working path

For codex, Vercel documents only editing `~/.codex/config.toml` or running its own setup
CLI. For opencode, only its setup CLI or an interactive `/connect`. The per-invocation
codex path and the project-local opencode path are both this project's findings. They are
not vendor guarantees and must not be relied on as though a vendor promised them.

### `ANTHROPIC_API_KEY=""` is not what makes claude-code reach a gateway

Vercel's page states that a non-empty `ANTHROPIC_API_KEY` wins over `ANTHROPIC_AUTH_TOKEN`.
Measured against CLI 2.1.260 through a request-capturing server, all three states — empty,
unset, and non-empty — produced byte-identical outgoing requests, with `ANTHROPIC_AUTH_TOKEN`
winning every time. The variable is set empty anyway because it costs nothing and matches
vendor guidance, but nothing may be built on the belief that it is load-bearing.

### A bad credential does not fail fast

A 401 from an endpoint is retried up to eleven times with exponential backoff and does not
finish inside 90 seconds; a 404 fails immediately. The per-attempt process timeout does
bound it, so nothing hangs forever — but a misconfigured credential silently consumes an
entire attempt budget rather than erroring. This is the argument for `doctor` reporting
endpoint health *before* anyone spends an attempt, not only after one fails.

### A subscription restricts the catalog, and that is why decision 1 has two modes

A codex running against a ChatGPT account refused a model with
`"The 'openai/gpt-5.6-sol' model is not supported when using Codex with a ChatGPT account."`
The same model id is served by an endpoint holding a key. One runner can therefore reach a
model two ways with different availability, different limits and different cost behaviour —
which is the case a single `model_provider` string cannot express.

### Why opencode cannot satisfy decisions 3, 5 and 6

Its `--format json` event stream carries no model field at any point, so the adapter cannot
honestly state which model served a request — it can only echo back what was asked for. A
real fix exists (`opencode export <sessionID>` returns `info.model.{providerID,id}`) but
requires a second subprocess with its own timeout and failure handling. Its own module
documentation additionally warns that an unrelated `opencode.json` elsewhere can change its
behaviour, which is the environment-sensitivity decision 8 refers to.

None of this makes opencode a bad program. It makes it a harness whose contract with Tack
cannot be stated as precisely as the other two, and the honest options were to build that
precision or to stop claiming it.

### What is still unmeasured

Whether an endpoint accepts bare (`anthropic/claude-sonnet-5`) or prefixed model ids for
routing was never confirmed against a valid credential — every probe with a deliberately
fake key is rejected before model resolution. Whether a *successful* response ever names a
model different from the requested one, which is what would justify promoting the
observation source above "requested, not confirmed", is likewise unmeasured. And no
endpoint other than one gateway has been probed, so the claim that decision 4's shape fits
LiteLLM and OpenRouter is reasoned, not measured.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*

**2026-09-04 — DECISION 8 AUTHORISED; decisions 1–7 remain proposed.** The user directed
the removal of opencode in chat ("borra opencode"), which is decision 8, and the work was
carried out the same day. Nothing in that instruction addressed the other seven decisions,
so this ADR's status stays `proposed`: decisions 1–4 and 6–7 describe what VI-B2 already
built and are therefore already true of the tree, but they have not been accepted as
binding, and **decision 5 — per-model prices and limits, which requires a reviewed wire
field — has neither been accepted nor implemented.** A later card must not treat this
amendment as acceptance of the table as a whole. If the user disagrees with this reading,
they strike this paragraph.

## Amendment — 2026-09-05: decision 4 was wrong and has been replaced

The original decision 4 read "**an endpoint is a configuration entry: name, base URL,
credential, wire** — adding LiteLLM or OpenRouter is a row, not a code change". That was the
ADR author's invention, not a directive, and it does not survive contact with the providers
it names. A config table can only describe providers that agree on a shape, and they do not:
the catalog lives at a different path per provider, returns a different JSON body, prices
in different units, and authenticates with a different header. Written that way, every
difference between providers ends up in the code that reads the table — which is the
coupling the decision claimed to remove, moved one level up.

Decision 4 now says a provider is a module behind one trait. The differences live inside
each module; the machinery that calls them stays vendor-free. Adding OpenRouter is one
module and one registry line — simple, which is the actual goal, rather than zero lines,
which was never achievable.

## What exists today, measured against these decisions

Read from the code on 2026-09-05, not from this document.

| Decision | State |
|---|---|
| 1 — two modes | **Built.** Subscription is the absence of a `[provider.<name>]` entry; `provider.rs` treats it as a typed `None`, never a second branch. |
| 2 — gateway and direct API are one mode | **Built.** Both are a base URL plus a bearer credential in the same `KnownEndpoint` shape. |
| 3 — a harness declares a wire | **Built.** `Wire::{AnthropicMessages, OpenAiResponses}`; claude-code takes an endpoint through environment variables, codex through invocation flags. |
| 4 — a provider is a module | **Not built.** There is no trait. `known_endpoint` and `catalog_url` are `match` arms over a provider name, and `attach_catalog` looks up `VERCEL_AI_GATEWAY_CONFIG_KEY` directly rather than iterating configured providers. |
| 5 — the catalog is the source | **Partly built.** The catalog is fetched, but `fetch_catalog_ids` keeps only `id` and drops every other field, and `ProviderConfig` carries only `enabled` and `secret`. |
| 6 — a quote is never a measurement | Nothing to break yet: no quoted price is stored. |
| 7 — unpublished is null | Nothing to break yet, same reason. |
| 8 — opencode removed | **Done**, in its own change; no fixture byte moved. |

Of the four endpoints these decisions are meant to serve, exactly one works:
`vercel-ai-gateway`, on both wires. Anthropic direct, OpenAI direct and OpenRouter do not
exist.

**The one contract change still required.** `ModelCombination` carries `model_provider`, a
`Vec<ModelId>`, a `discovery` string and a flattened `additional` map. There is nowhere on
the wire for a price, a context window or a modality, and `additional` must not be used for
this — a contract change smuggled through the escape hatch is a contract change nobody
reviewed. Decision 5 needs a named field on the wire type and a revision of the byte-pinned
fixtures in `docs/contracts/runner-v1/`. Decisions 1–4 and 6–8 need no contract change.

## Amendment — 2026-09-05: accepted

The user accepted this ADR on 2026-09-05, having first rejected the decision 4 it was
written with. Two things were made explicit in accepting it, and neither was in the
original text:

**The two modes were never in question.** Subscription login (a harness that logs in by
itself — Codex, Claude) and API key plus endpoint are different mechanisms, and the endpoint
in the second may be a vendor's own API (Anthropic, OpenAI) or a gateway (Vercel AI Gateway,
OpenRouter) without that being a third mode. Presenting this as an open choice was a
misreading; it was a directive.

**The abstraction is the deliverable.** Providers that use an endpoint and a key must be
modular and scalable, so that adding one later is simple. Each provider's module must
understand its own provider's endpoints and get what is needed from them: which models
exist, what they cost, what their limits are, and whatever else that provider publishes.
"Simple" is the requirement. "Without writing code" was not, and is not achievable, because
providers do not follow one exact standard.

Wave 15 gains VI-B4, which builds the trait and proves it with a second provider module.
