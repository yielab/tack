//! Amazon Alexa skill endpoint (`POST /api/alexa`).
//!
//! Maps voice intents onto the same repository and workflow logic the REST
//! handlers use. The endpoint is disabled (404) unless `alexa_skill_id` is
//! configured. Each request is authenticated by verifying the application ID
//! Alexa embeds in every request envelope (constant-time comparison) and by
//! rejecting timestamps outside Alexa's ±150 second tolerance window.
//!
//! Responses are localised from the request's `locale` field: any `es-*`
//! locale gets Spanish speech, everything else falls back to English.
//!
//! User-level problems (unknown project, missing slot, invalid transition)
//! are answered with HTTP 200 + spoken text, as the Alexa protocol expects.
//! Only verification failures produce HTTP errors.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::{info, instrument};

use tack_core::models::{CreateItem, Item, ItemFilter, Project, UpdateItem};

use crate::error::{ApiError, ApiResult};
use crate::handlers::items::propagate_parent_completion;
use crate::handlers::websocket::{self, BoardEvent};
use crate::middleware::constant_time_eq;
use crate::router::AppState;

/// Maximum allowed clock skew between Alexa and this server (Alexa's own
/// certification requirement is 150 seconds).
const TIMESTAMP_TOLERANCE_SECS: i64 = 150;

/// How many open item titles to read aloud before summarising.
const SPOKEN_ITEM_LIMIT: usize = 3;

// ─── Request envelope (subset of the Alexa skill request schema) ────────────

#[derive(Debug, Deserialize)]
pub struct AlexaRequest {
    session: Option<AlexaSession>,
    context: Option<AlexaContext>,
    request: AlexaRequestBody,
}

#[derive(Debug, Deserialize)]
struct AlexaSession {
    application: Option<AlexaApplication>,
}

#[derive(Debug, Deserialize)]
struct AlexaContext {
    #[serde(rename = "System")]
    system: Option<AlexaSystem>,
}

#[derive(Debug, Deserialize)]
struct AlexaSystem {
    application: Option<AlexaApplication>,
}

#[derive(Debug, Deserialize)]
struct AlexaApplication {
    #[serde(rename = "applicationId")]
    application_id: String,
}

#[derive(Debug, Deserialize)]
struct AlexaRequestBody {
    #[serde(rename = "type")]
    kind: String,
    timestamp: Option<DateTime<Utc>>,
    locale: Option<String>,
    intent: Option<AlexaIntent>,
}

#[derive(Debug, Deserialize)]
struct AlexaIntent {
    name: String,
    #[serde(default)]
    slots: HashMap<String, AlexaSlot>,
}

#[derive(Debug, Deserialize)]
struct AlexaSlot {
    value: Option<String>,
}

impl AlexaRequest {
    /// Skill application ID, from the session or (for out-of-session
    /// requests) the context block.
    fn application_id(&self) -> Option<&str> {
        self.session
            .as_ref()
            .and_then(|s| s.application.as_ref())
            .or_else(|| {
                self.context
                    .as_ref()
                    .and_then(|c| c.system.as_ref())
                    .and_then(|s| s.application.as_ref())
            })
            .map(|a| a.application_id.as_str())
    }

    /// Non-empty value of a named intent slot.
    fn slot(&self, name: &str) -> Option<&str> {
        self.request
            .intent
            .as_ref()
            .and_then(|i| i.slots.get(name))
            .and_then(|s| s.value.as_deref())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    }

    /// Response language, from the request locale.
    fn lang(&self) -> Lang {
        Lang::from_locale(self.request.locale.as_deref())
    }
}

// ─── Localisation ────────────────────────────────────────────────────────────

/// Response language. Spanish for any `es-*` locale, English otherwise.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Lang {
    En,
    Es,
}

impl Lang {
    fn from_locale(locale: Option<&str>) -> Self {
        match locale {
            Some(l) if l.starts_with("es") => Lang::Es,
            _ => Lang::En,
        }
    }
}

fn welcome(lang: Lang) -> Json<Value> {
    match lang {
        Lang::En => prompt(
            "Welcome to Flex P M. You can add a task, list open tasks, or complete a task. \
             What would you like to do?",
            "Try saying: add a task called water the plants.",
        ),
        Lang::Es => prompt(
            "Bienvenido a Flex P M. Puedes agregar una tarea, listar tus tareas pendientes \
             o completar una tarea. ¿Qué quieres hacer?",
            "Prueba decir: agrega una tarea llamada regar las plantas.",
        ),
    }
}

fn help(lang: Lang) -> Json<Value> {
    match lang {
        Lang::En => prompt(
            "You can say: add a task called buy cement. Or: what are my open tasks. \
             Or: complete the task buy cement. To target a project, add: in project, \
             followed by its name.",
            "What would you like to do?",
        ),
        Lang::Es => prompt(
            "Puedes decir: agrega una tarea llamada comprar cemento. O: cuáles son mis \
             tareas pendientes. O: completa la tarea comprar cemento. Para elegir un \
             proyecto, agrega: en el proyecto, seguido de su nombre.",
            "¿Qué quieres hacer?",
        ),
    }
}

fn goodbye(lang: Lang) -> Json<Value> {
    match lang {
        Lang::En => speech("Goodbye.", true),
        Lang::Es => speech("Hasta luego.", true),
    }
}

fn fallback(lang: Lang) -> Json<Value> {
    match lang {
        Lang::En => prompt(
            "Sorry, I didn't catch that. You can add, list, or complete tasks.",
            "Try saying: what are my open tasks.",
        ),
        Lang::Es => prompt(
            "Perdón, no entendí. Puedes agregar, listar o completar tareas.",
            "Prueba decir: cuáles son mis tareas pendientes.",
        ),
    }
}

fn unsupported(lang: Lang) -> Json<Value> {
    match lang {
        Lang::En => speech("Sorry, I can't handle that kind of request.", true),
        Lang::Es => speech("Lo siento, no puedo manejar ese tipo de solicitud.", true),
    }
}

fn ask_add_title(lang: Lang) -> Json<Value> {
    match lang {
        Lang::En => prompt(
            "What should the task be called?",
            "Say: add a task called, followed by its name.",
        ),
        Lang::Es => prompt(
            "¿Cómo se llama la tarea?",
            "Di: agrega una tarea llamada, seguido de su nombre.",
        ),
    }
}

fn ask_complete_title(lang: Lang) -> Json<Value> {
    match lang {
        Lang::En => prompt(
            "Which task should I complete?",
            "Say: complete the task, followed by its name.",
        ),
        Lang::Es => prompt(
            "¿Qué tarea quieres completar?",
            "Di: completa la tarea, seguido de su nombre.",
        ),
    }
}

fn msg_no_projects(lang: Lang) -> String {
    match lang {
        Lang::En => "You don't have any projects yet. Create one in Flex P M first.".into(),
        Lang::Es => "Aún no tienes proyectos. Crea uno en Flex P M primero.".into(),
    }
}

fn msg_project_not_found(lang: Lang, name: &str) -> String {
    match lang {
        Lang::En => format!("I couldn't find a project called {name}."),
        Lang::Es => format!("No encontré un proyecto llamado {name}."),
    }
}

fn msg_added(lang: Lang, term: &str, title: &str, project: &str) -> String {
    match lang {
        Lang::En => format!("Added {term} {title} to {project}."),
        Lang::Es => format!("Agregué {term} {title} a {project}."),
    }
}

fn msg_nothing_open(lang: Lang, project: &str) -> String {
    match lang {
        Lang::En => format!("There is nothing open in {project}. Nice work."),
        Lang::Es => format!("No hay nada pendiente en {project}. ¡Buen trabajo!"),
    }
}

fn msg_open_summary(
    lang: Lang,
    count: usize,
    term: &str,
    project: &str,
    titles: &[&str],
) -> String {
    let plural = if count == 1 { "" } else { "s" };
    match lang {
        Lang::En => {
            let listing = if count > titles.len() {
                format!("The first {} are: {}.", titles.len(), titles.join(", "))
            } else {
                format!("They are: {}.", titles.join(", "))
            };
            format!("You have {count} open {term}{plural} in {project}. {listing}")
        }
        Lang::Es => {
            let listing = if count > titles.len() {
                format!("Las primeras {} son: {}.", titles.len(), titles.join(", "))
            } else {
                format!("Son: {}.", titles.join(", "))
            };
            let pendiente = if count == 1 {
                "pendiente"
            } else {
                "pendientes"
            };
            format!("Tienes {count} {term}{plural} {pendiente} en {project}. {listing}")
        }
    }
}

fn msg_no_done_status(lang: Lang, project: &str) -> String {
    match lang {
        Lang::En => format!("The workflow of {project} has no done status."),
        Lang::Es => format!("El flujo de trabajo de {project} no tiene un estado de terminado."),
    }
}

fn msg_open_item_not_found(lang: Lang, title: &str, project: &str) -> String {
    match lang {
        Lang::En => format!("I couldn't find an open task called {title} in {project}."),
        Lang::Es => format!("No encontré nada pendiente llamado {title} en {project}."),
    }
}

fn msg_invalid_transition(lang: Lang, title: &str, from: &str, to: &str) -> String {
    match lang {
        Lang::En => {
            format!("{title} can't move from {from} straight to {to} in this workflow.")
        }
        Lang::Es => {
            format!("{title} no puede pasar de {from} directo a {to} en este flujo de trabajo.")
        }
    }
}

fn msg_wip_limit(lang: Lang, status: &str) -> String {
    match lang {
        Lang::En => format!("The {status} column is at its work-in-progress limit."),
        Lang::Es => format!("La columna {status} alcanzó su límite de trabajo en curso."),
    }
}

fn msg_completed(lang: Lang, title: &str, status: &str, project: &str) -> String {
    match lang {
        Lang::En => format!("Marked {title} as {status} in {project}."),
        Lang::Es => format!("Marqué {title} como {status} en {project}."),
    }
}

// ─── Response builders ───────────────────────────────────────────────────────

/// Plain-text speech response, optionally ending the session.
fn speech(text: &str, end_session: bool) -> Json<Value> {
    Json(json!({
        "version": "1.0",
        "response": {
            "outputSpeech": { "type": "PlainText", "text": text },
            "shouldEndSession": end_session,
        }
    }))
}

/// Question that keeps the session open and re-prompts if the user is silent.
fn prompt(text: &str, reprompt: &str) -> Json<Value> {
    Json(json!({
        "version": "1.0",
        "response": {
            "outputSpeech": { "type": "PlainText", "text": text },
            "reprompt": {
                "outputSpeech": { "type": "PlainText", "text": reprompt }
            },
            "shouldEndSession": false,
        }
    }))
}

// ─── Handler ─────────────────────────────────────────────────────────────────

#[instrument(skip(state, payload))]
pub async fn handle_request(
    State(state): State<AppState>,
    Json(payload): Json<AlexaRequest>,
) -> ApiResult<Json<Value>> {
    let Some(ref expected_skill_id) = state.config.alexa_skill_id else {
        return Err(ApiError::NotFound(
            "Alexa integration is not enabled".into(),
        ));
    };

    let app_id = payload
        .application_id()
        .ok_or_else(|| ApiError::Forbidden("Missing Alexa application ID".into()))?;
    if !constant_time_eq(app_id.as_bytes(), expected_skill_id.as_bytes()) {
        return Err(ApiError::Forbidden("Unknown Alexa skill".into()));
    }

    // Replay protection: reject stale or future-dated requests.
    let timestamp = payload
        .request
        .timestamp
        .ok_or_else(|| ApiError::BadRequest("Missing request timestamp".into()))?;
    if (Utc::now() - timestamp).num_seconds().abs() > TIMESTAMP_TOLERANCE_SECS {
        return Err(ApiError::BadRequest(
            "Request timestamp out of tolerance".into(),
        ));
    }

    let lang = payload.lang();

    match payload.request.kind.as_str() {
        "LaunchRequest" => Ok(welcome(lang)),
        "SessionEndedRequest" => Ok(Json(json!({ "version": "1.0", "response": {} }))),
        "IntentRequest" => {
            let intent = payload
                .request
                .intent
                .as_ref()
                .map(|i| i.name.as_str())
                .unwrap_or_default();
            info!(intent, locale = ?payload.request.locale, "Alexa intent received");
            match intent {
                "AddTaskIntent" => add_task(&state, &payload, lang).await,
                "ListTasksIntent" => list_tasks(&state, &payload, lang).await,
                "CompleteTaskIntent" => complete_task(&state, &payload, lang).await,
                "AMAZON.HelpIntent" => Ok(help(lang)),
                "AMAZON.StopIntent" | "AMAZON.CancelIntent" => Ok(goodbye(lang)),
                _ => Ok(fallback(lang)),
            }
        }
        _ => Ok(unsupported(lang)),
    }
}

// ─── Intents ─────────────────────────────────────────────────────────────────

async fn add_task(state: &AppState, req: &AlexaRequest, lang: Lang) -> ApiResult<Json<Value>> {
    let Some(title) = req.slot("title") else {
        return Ok(ask_add_title(lang));
    };

    let project = match resolve_project(state, req.slot("project"), lang).await? {
        Ok(p) => p,
        Err(spoken) => return Ok(speech(&spoken, true)),
    };

    let initial_status = project.workflow.initial_status().map_err(ApiError::Core)?;
    let item = state
        .repo
        .create_item(
            project.id,
            &initial_status,
            CreateItem {
                title: title.to_string(),
                ..Default::default()
            },
        )
        .await?;

    websocket::broadcast_event(
        state,
        BoardEvent::ItemCreated {
            project_id: project.id,
            item_id: item.id,
            status: item.status.clone(),
        },
    );

    Ok(speech(
        &msg_added(
            lang,
            &vocab_term(&project, "task"),
            &item.title,
            &project.name,
        ),
        true,
    ))
}

async fn list_tasks(state: &AppState, req: &AlexaRequest, lang: Lang) -> ApiResult<Json<Value>> {
    let project = match resolve_project(state, req.slot("project"), lang).await? {
        Ok(p) => p,
        Err(spoken) => return Ok(speech(&spoken, true)),
    };

    let items = state
        .repo
        .list_items(project.id, &ItemFilter::default())
        .await?;
    let open: Vec<&Item> = items
        .iter()
        .filter(|i| !project.workflow.is_done_status(&i.status))
        .collect();

    if open.is_empty() {
        return Ok(speech(&msg_nothing_open(lang, &project.name), true));
    }

    let titles: Vec<&str> = open
        .iter()
        .take(SPOKEN_ITEM_LIMIT)
        .map(|i| i.title.as_str())
        .collect();

    Ok(speech(
        &msg_open_summary(
            lang,
            open.len(),
            &vocab_term(&project, "task"),
            &project.name,
            &titles,
        ),
        true,
    ))
}

async fn complete_task(state: &AppState, req: &AlexaRequest, lang: Lang) -> ApiResult<Json<Value>> {
    let Some(title) = req.slot("title") else {
        return Ok(ask_complete_title(lang));
    };

    let project = match resolve_project(state, req.slot("project"), lang).await? {
        Ok(p) => p,
        Err(spoken) => return Ok(speech(&spoken, true)),
    };

    let Some(done_status) = project.workflow.find_first_done_status() else {
        return Ok(speech(&msg_no_done_status(lang, &project.name), true));
    };

    let items = state
        .repo
        .list_items(project.id, &ItemFilter::default())
        .await?;
    let wanted = title.to_lowercase();
    let open = || {
        items
            .iter()
            .filter(|i| !project.workflow.is_done_status(&i.status))
    };
    let target = open()
        .find(|i| i.title.to_lowercase() == wanted)
        .or_else(|| open().find(|i| i.title.to_lowercase().contains(&wanted)));
    let Some(target) = target else {
        return Ok(speech(
            &msg_open_item_not_found(lang, title, &project.name),
            true,
        ));
    };

    // Same guards as the REST update handler: transition validity + WIP limit.
    if project
        .workflow
        .validate_transition(&target.status, done_status)
        .is_err()
    {
        return Ok(speech(
            &msg_invalid_transition(lang, &target.title, &target.status, done_status),
            true,
        ));
    }
    let count = state
        .repo
        .count_items_by_status(project.id, done_status)
        .await? as usize;
    if project
        .workflow
        .check_wip_limit(done_status, count)
        .is_err()
    {
        return Ok(speech(&msg_wip_limit(lang, done_status), true));
    }

    let old_status = target.status.clone();
    let item = state
        .repo
        .update_item(
            target.id,
            UpdateItem {
                status: Some(done_status.to_string()),
                ..Default::default()
            },
        )
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {} not found", target.id)))?;

    websocket::broadcast_event(
        state,
        BoardEvent::ItemUpdated {
            project_id: item.project_id,
            item_id: item.id,
            old_status: Some(old_status.clone()),
            new_status: item.status.clone(),
        },
    );
    propagate_parent_completion(state, &item, &old_status).await;

    Ok(speech(
        &msg_completed(lang, &item.title, &item.status, &project.name),
        true,
    ))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Resolve the target project from the optional `project` slot.
///
/// Returns `Ok(Err(spoken))` when resolution fails in a way the user must fix
/// by voice — Alexa expects 200 + speech for that, not an HTTP error. Without
/// a slot the most recently updated project is used, and every intent speaks
/// the chosen project's name so the user always hears where the action landed.
async fn resolve_project(
    state: &AppState,
    requested: Option<&str>,
    lang: Lang,
) -> ApiResult<Result<Project, String>> {
    let mut projects: Vec<Project> = state
        .repo
        .list_projects(state.workspace_id)
        .await?
        .into_iter()
        .filter(|p| !p.archived)
        .collect();

    if projects.is_empty() {
        return Ok(Err(msg_no_projects(lang)));
    }

    if let Some(wanted) = requested {
        let lower = wanted.to_lowercase();
        let found = projects
            .iter()
            .find(|p| p.name.to_lowercase() == lower)
            .or_else(|| {
                projects
                    .iter()
                    .find(|p| p.name.to_lowercase().contains(&lower))
            });
        return Ok(match found {
            Some(p) => Ok(p.clone()),
            None => Err(msg_project_not_found(lang, wanted)),
        });
    }

    projects.sort_by_key(|p| std::cmp::Reverse(p.updated_at));
    Ok(Ok(projects.remove(0)))
}

/// Project-specific vocabulary term (e.g. "Work Order" for construction).
fn vocab_term(project: &Project, key: &str) -> String {
    project
        .vocabulary
        .get(key)
        .cloned()
        .unwrap_or_else(|| key.to_string())
}
