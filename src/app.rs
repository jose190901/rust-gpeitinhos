use crate::{
    account::SubscriptionAccount,
    projects::{Project, ProjectOperation, ProjectWorker},
    runtime::{Runtime, RuntimeEvent},
    terminal::{Terminal, TerminalAction, sandbox_policy},
    ui::Ui,
    view_model::*,
    workbench_ui::WorkbenchUi,
    workspace::WorkspaceReview,
};
use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Outgoing {
    thread: Option<String>,
    cwd: String,
    model: String,
    effort: String,
    access: AccessMode,
    text: String,
    files: Vec<String>,
    skills: Vec<SkillInfo>,
    generation: u64,
    expected_turn: Option<String>,
}
#[derive(Clone)]
struct ActiveSettings {
    model: String,
    effort: String,
    access: AccessMode,
    cwd: String,
}
#[derive(Clone)]
struct LiveHistory {
    items: Vec<DisplayItem>,
    cursor: Option<String>,
}
enum Pending {
    Initialize,
    BootstrapAccount,
    Configuration,
    Models,
    Usage,
    Threads(bool),
    Skills(String),
    Verify(Outgoing),
    Start(Outgoing),
    Resume {
        id: String,
        send: Option<Outgoing>,
    },
    History {
        id: String,
        older: bool,
        generation: u64,
    },
    LegacyHistory {
        id: String,
        generation: u64,
        older: bool,
    },
    Turn {
        id: String,
        submission: Outgoing,
    },
    Steer {
        id: String,
        turn: String,
        submission: Outgoing,
    },
    TerminalExec(String),
    TerminalInput {
        process: String,
        text: Option<String>,
    },
    TerminalStop,
    ReviewCommand(mpsc::Sender<Result<Vec<u8>, String>>),
    Ignore,
}
impl Pending {
    fn generation(&self) -> Option<u64> {
        match self {
            Self::Verify(o)
            | Self::Start(o)
            | Self::Resume { send: Some(o), .. }
            | Self::Turn { submission: o, .. } => Some(o.generation),
            Self::Steer { submission: o, .. } => Some(o.generation),
            _ => None,
        }
    }
}
#[derive(Serialize, Deserialize, Default)]
struct Preferences {
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    threads: Vec<ThreadInfo>,
    #[serde(default)]
    projects: Vec<Project>,
}

pub struct RustGPeitinhosApp {
    state: UiState,
    ui: Ui,
    runtime: Option<Runtime>,
    starting: Receiver<Result<Runtime, String>>,
    pending: HashMap<u64, (Pending, Instant)>,
    active: HashMap<String, String>,
    active_settings: HashMap<String, ActiveSettings>,
    live_history: HashMap<String, LiveHistory>,
    completed_turns: HashSet<String>,
    prompts: VecDeque<ServerPrompt>,
    threads_cursor: Option<String>,
    history_cursor: Option<String>,
    history_generation: u64,
    data: PathBuf,
    account_identity: Option<String>,
    provider_valid: bool,
    root: PathBuf,
    binary: PathBuf,
    send_in_flight: bool,
    startup: Instant,
    smoke: bool,
    smoke_done: bool,
    first_frame: bool,
    screenshot_requested: bool,
    screenshot_done: bool,
    screenshot_receiver: Receiver<()>,
    screenshot_sender: mpsc::Sender<()>,
    smoke_chat: bool,
    smoke_terminal: bool,
    smoke_steer: bool,
    smoke_steered: bool,
    smoke_steer_accepted: bool,
    smoke_sent: bool,
    send_generation: u64,
    cancelled: HashSet<u64>,
    terminal: Terminal,
    review: WorkspaceReview,
    workbench: WorkbenchUi,
    project_worker: ProjectWorker,
    last_usage: Instant,
}

impl RustGPeitinhosApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        root: PathBuf,
        binary: PathBuf,
        smoke: bool,
        smoke_chat: bool,
        launched: Instant,
        data: PathBuf,
    ) -> Self {
        let preferences: Preferences = std::fs::read(data.join("preferences.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let cwd = if Path::new(&preferences.cwd).is_dir() {
            preferences.cwd
        } else {
            root.to_string_lossy().into_owned()
        };
        let mut state = UiState::new(cwd);
        state.projects = preferences.projects;
        if smoke_chat {
            if let Some(folder) = std::env::var_os("PONTE_AGENT_TEST_DIR") {
                let folder = PathBuf::from(folder);
                if folder.is_absolute() && folder.is_dir() {
                    state.cwd = folder.to_string_lossy().into_owned();
                    state.folder_edit = state.cwd.clone();
                }
            }
        }
        let (tx, starting) = mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        let worker_root = root.clone();
        let worker_binary = binary.clone();
        std::thread::spawn(move || {
            let result = Runtime::spawn(&worker_binary, &worker_root);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        let (screenshot_sender, screenshot_receiver) = mpsc::channel();
        Self {
            state,
            ui: Ui::new(&cc.egui_ctx),
            runtime: None,
            starting,
            pending: HashMap::new(),
            active: HashMap::new(),
            active_settings: HashMap::new(),
            live_history: HashMap::new(),
            completed_turns: HashSet::new(),
            prompts: VecDeque::new(),
            threads_cursor: None,
            history_cursor: None,
            history_generation: 0,
            data,
            account_identity: None,
            provider_valid: false,
            root,
            binary,
            send_in_flight: false,
            startup: launched,
            smoke,
            smoke_done: false,
            first_frame: false,
            screenshot_requested: false,
            screenshot_done: false,
            screenshot_receiver,
            screenshot_sender,
            smoke_chat,
            smoke_terminal: std::env::args().any(|s| s == "--smoke-terminal"),
            smoke_steer: std::env::args().any(|s| s == "--smoke-steer"),
            smoke_steered: false,
            smoke_steer_accepted: false,
            smoke_sent: false,
            send_generation: 0,
            cancelled: HashSet::new(),
            terminal: Terminal::default(),
            review: WorkspaceReview::new(),
            workbench: WorkbenchUi::new(),
            project_worker: ProjectWorker::new(),
            last_usage: Instant::now(),
        }
    }

    fn rpc(&mut self, method: &str, params: Value, pending: Pending) {
        if self.smoke {
            eprintln!(
                "rustgpeitinhos +{}ms request {method}",
                self.startup.elapsed().as_millis()
            );
        }
        let generation = pending.generation();
        let result = self
            .runtime
            .as_ref()
            .ok_or_else(|| "O Codex ainda está conectando.".to_owned())
            .and_then(|r| r.request(method, params));
        match result {
            Ok(id) => {
                self.pending.insert(id, (pending, Instant::now()));
            }
            Err(e) => {
                if matches!(&pending, Pending::Usage) {
                    self.state.usage.set_error(e);
                    return;
                }
                if let Pending::ReviewCommand(reply) = &pending {
                    let _ = reply.send(Err(e.clone()));
                    return;
                }
                if let Pending::TerminalExec(process) = &pending {
                    self.terminal.finish(process, Err(&e));
                }
                if let Pending::History { id, generation, .. }
                | Pending::LegacyHistory { id, generation, .. } = &pending
                {
                    if self.current_history(id, *generation) {
                        self.state.loading_history = false;
                    }
                }
                if generation == Some(self.send_generation) {
                    self.send_in_flight = false;
                }
                self.fail(e);
            }
        }
    }

    fn fail(&mut self, error: String) {
        self.state.error = Some(error);
    }
    fn valid_submission(&self, outgoing: &Outgoing) -> bool {
        self.state.connected
            && self.provider_valid
            && self.send_in_flight
            && outgoing.generation == self.send_generation
            && !self.cancelled.contains(&outgoing.generation)
    }
    fn fail_submission(&mut self, outgoing: &Outgoing, error: String) {
        if outgoing.generation == self.send_generation {
            self.send_in_flight = false;
        }
        self.fail(error);
    }
    fn save_preferences(&mut self) {
        if self.smoke {
            return;
        }
        let result: Result<(), String> = (|| {
            let bytes = serde_json::to_vec(&Preferences {
                cwd: self.state.cwd.clone(),
                threads: vec![],
                projects: self.state.projects.clone(),
            })
            .map_err(|e| e.to_string())?;
            std::fs::create_dir_all(&self.data).map_err(|e| e.to_string())?;
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let temporary = self
                .data
                .join(format!("preferences-{}-{nonce}.tmp", std::process::id()));
            let mut created = false;
            let write = (|| -> Result<(), String> {
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&temporary).map_err(|e| e.to_string())?;
                created = true;
                std::io::Write::write_all(&mut file, &bytes).map_err(|e| e.to_string())?;
                std::fs::rename(&temporary, self.data.join("preferences.json"))
                    .map_err(|e| e.to_string())?;
                Ok(())
            })();
            if created && write.is_err() {
                let _ = std::fs::remove_file(&temporary);
            }
            write
        })();
        if let Err(error) = result {
            self.fail(format!("Não foi possível salvar seus projetos e preferências: {error}. As pastas e seus arquivos permanecem no disco."));
        }
    }

    fn fetch_usage(&mut self) {
        if self
            .pending
            .values()
            .any(|(p, _)| matches!(p, Pending::Usage))
        {
            return;
        }
        self.last_usage = Instant::now();
        self.rpc("account/rateLimits/read", json!({}), Pending::Usage);
    }

    fn fetch_threads(&mut self, append: bool) {
        self.state.threads_loading = true;
        let mut params = json!({"limit":100,"currentAccountOnly":true,"sortKey":"updated_at","useStateDbOnly":true});
        if append {
            params["cursor"] = json!(self.threads_cursor);
        }
        self.rpc("thread/list", params, Pending::Threads(append));
    }
    fn fetch_skills(&mut self) {
        self.rpc(
            "skills/list",
            json!({"cwds":[self.state.cwd],"forceReload":false}),
            Pending::Skills(self.state.cwd.clone()),
        );
    }
    fn load_history(&mut self, id: String, older: bool) {
        self.history_generation = self.history_generation.saturating_add(1);
        let generation = self.history_generation;
        if !older {
            if let Some(history) = self.live_history.get(&id) {
                self.state.items = history.items.clone();
                self.history_cursor = history.cursor.clone();
                self.state.more_history = self.history_cursor.is_some();
                if self.active.contains_key(&id) {
                    // A snapshot and live deltas have no shared sequence number.
                    // Keep the complete local stream instead of guessing which
                    // deltas a concurrent snapshot already contains.
                    self.state.loading_history = false;
                    return;
                }
            }
        }
        self.state.loading_history = true;
        if self
            .state
            .threads
            .iter()
            .any(|t| t.id == id && t.history_mode.as_deref() == Some("legacy"))
        {
            self.rpc(
                "thread/read",
                json!({"threadId":id,"includeTurns":true}),
                Pending::LegacyHistory {
                    id,
                    generation,
                    older,
                },
            );
            return;
        }
        let mut params = json!({"threadId":id,"limit":100,"sortDirection":"desc"});
        if older {
            params["cursor"] = json!(self.history_cursor);
        }
        self.rpc(
            "thread/items/list",
            params,
            Pending::History {
                id,
                older,
                generation,
            },
        );
    }

    fn current_history(&self, id: &str, generation: u64) -> bool {
        self.state.current_thread.as_deref() == Some(id) && generation == self.history_generation
    }

    fn check_account(&mut self, response: &Value) -> Result<SubscriptionAccount, String> {
        let account = SubscriptionAccount::verify(response)?;
        let identity = response["account"]["id"]
            .as_str()
            .or_else(|| response["account"]["email"].as_str())
            .ok_or("O Codex não confirmou a identidade da conta.")?
            .to_owned();
        if let Some(expected) = &self.account_identity {
            if expected != &identity {
                return Err(
                    "A conta ChatGPT mudou. Reconecte o aplicativo antes de continuar.".into(),
                );
            }
        }
        self.account_identity = Some(identity);
        Ok(account)
    }

    fn response(&mut self, id: u64, message: Value) {
        if self.smoke {
            eprintln!(
                "rustgpeitinhos +{}ms response {id} error={}",
                self.startup.elapsed().as_millis(),
                message.get("error").is_some()
            );
        }
        let Some((pending, _)) = self.pending.remove(&id) else {
            return;
        };
        // A -> B -> A may have two reads for the same thread in flight.
        // Thread identity alone cannot identify the current view's response.
        if let Pending::History { id, generation, .. }
        | Pending::LegacyHistory { id, generation, .. } = &pending
        {
            if !self.current_history(id, *generation) {
                return;
            }
        }
        if let Some(error) = message.get("error") {
            if matches!(&pending, Pending::Usage) {
                self.state.usage.set_error(
                    error["message"]
                        .as_str()
                        .unwrap_or("Limites indisponíveis.")
                        .into(),
                );
                return;
            }
            if let Pending::ReviewCommand(reply) = pending {
                let _ = reply.send(Err(error["message"]
                    .as_str()
                    .unwrap_or("Falha na revisão Git.")
                    .into()));
                return;
            }
            if let Pending::TerminalExec(process) = &pending {
                self.terminal.finish(
                    process,
                    Err(error["message"].as_str().unwrap_or("Falha no terminal.")),
                );
                return;
            }
            if matches!(
                &pending,
                Pending::TerminalInput { .. } | Pending::TerminalStop
            ) {
                self.terminal.error = Some(
                    error["message"]
                        .as_str()
                        .unwrap_or("Falha no controle do terminal.")
                        .into(),
                );
                return;
            }
            if let Pending::History {
                id,
                generation,
                older,
            } = &pending
            {
                self.rpc(
                    "thread/read",
                    json!({"threadId":id,"includeTurns":true}),
                    Pending::LegacyHistory {
                        id: id.clone(),
                        generation: *generation,
                        older: *older,
                    },
                );
                return;
            }
            if matches!(&pending, Pending::LegacyHistory { .. }) {
                self.state.loading_history = false;
            }
            if matches!(&pending, Pending::Threads(_)) {
                self.state.threads_loading = false;
            }
            if pending.generation() == Some(self.send_generation) {
                self.send_in_flight = false;
            }
            let text = error["message"]
                .as_str()
                .unwrap_or("Erro retornado pelo Codex")
                .to_owned();
            if matches!(
                pending,
                Pending::BootstrapAccount | Pending::Configuration | Pending::Verify(_)
            ) {
                self.state.connected = false;
            }
            self.fail(text);
            return;
        }
        let result = &message["result"];
        match pending {
            Pending::Initialize => {
                if let Some(r) = &self.runtime {
                    let _ = r.notify("initialized", json!({}));
                }
                self.rpc(
                    "account/read",
                    json!({"refreshToken":false}),
                    Pending::BootstrapAccount,
                );
            }
            Pending::BootstrapAccount => match self.check_account(result) {
                Ok(account) => {
                    self.state.plan = Some(account.plan);
                    self.state.status = "Carregando seus modelos e conversas…".into();
                    self.rpc(
                        "config/read",
                        json!({"cwd":self.state.cwd,"includeLayers":false}),
                        Pending::Configuration,
                    );
                    self.rpc(
                        "model/list",
                        json!({"limit":100,"includeHidden":false}),
                        Pending::Models,
                    );
                    self.fetch_threads(false);
                    self.fetch_skills();
                    self.fetch_usage();
                }
                Err(error) => {
                    self.state.connected = false;
                    self.fail(error);
                }
            },
            Pending::Configuration => {
                self.provider_valid = result["config"]["model_provider"]
                    .as_str()
                    .unwrap_or("openai")
                    == "openai";
                self.state.connected = self.provider_valid && self.state.plan.is_some();
                self.state.status = if self.state.connected {
                    "Assinatura ChatGPT conectada"
                } else {
                    "Provedor não autorizado"
                }
                .into();
                if !self.provider_valid {
                    self.fail(
                        "A configuração usa outro provedor. O envio por assinatura foi bloqueado."
                            .into(),
                    );
                }
            }
            Pending::Models => {
                self.state.models =
                    serde_json::from_value(result["data"].clone()).unwrap_or_default();
                if !self
                    .state
                    .models
                    .iter()
                    .any(|m| m.model == self.state.model)
                {
                    self.state.error=Some("GPT‑6.2 Sol não foi retornado pelo catálogo. Escolha um modelo disponível.".into());
                }
            }
            Pending::Usage => self.state.usage.apply_snapshot(result),
            Pending::Threads(append) => {
                self.state.threads_loading = false;
                let rows: Vec<ThreadInfo> = result["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|row| match serde_json::from_value(row.clone()) {
                        Ok(thread) => Some(thread),
                        Err(error) => {
                            self.state
                                .notices
                                .push(format!("Uma conversa não pôde ser lida: {error}"));
                            None
                        }
                    })
                    .collect();
                self.threads_cursor = result["nextCursor"].as_str().map(str::to_owned);
                self.state.more_threads = self.threads_cursor.is_some();
                if !append {
                    self.state.threads.clear();
                }
                for row in rows {
                    if !self.state.threads.iter().any(|t| t.id == row.id) {
                        self.state.threads.push(row);
                    }
                }
                self.save_preferences();
            }
            Pending::Skills(cwd) => {
                if cwd != self.state.cwd {
                    return;
                }
                self.state.skills = result["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|entry| entry["skills"].as_array().into_iter().flatten())
                    .filter_map(|s| serde_json::from_value(s.clone()).ok())
                    .collect();
            }
            Pending::Verify(outgoing) => {
                if !self.valid_submission(&outgoing) {
                    return;
                }
                if let Err(error) = self.check_account(result) {
                    self.state.connected = false;
                    self.live_history.clear();
                    self.history_generation = self.history_generation.saturating_add(1);
                    self.state.loading_history = false;
                    self.fail_submission(&outgoing, error);
                    return;
                }
                if let Some(turn) = &outgoing.expected_turn {
                    let Some(thread) = &outgoing.thread else {
                        return;
                    };
                    if self.active.get(thread) != Some(turn) || self.completed_turns.contains(turn)
                    {
                        self.fail_submission(&outgoing,"O trabalho terminou antes deste envio. Sua mensagem foi mantida; envie para continuar a conversa.".into());
                        return;
                    }
                    self.rpc("turn/steer",json!({"threadId":thread,"expectedTurnId":turn,"input":Self::user_input(&outgoing)}),Pending::Steer{id:thread.clone(),turn:turn.clone(),submission:outgoing});
                    return;
                }
                if let Some(id) = &outgoing.thread {
                    self.rpc("thread/resume",json!({"threadId":id,"excludeTurns":true,"modelProvider":"openai","model":outgoing.model,"cwd":outgoing.cwd,"sandbox":outgoing.access.sandbox(),"runtimeWorkspaceRoots":[outgoing.cwd],"approvalPolicy":"on-request","approvalsReviewer":"user","developerInstructions":"Não use skills Superpowers automaticamente; apenas se o usuário solicitar explicitamente. Não substitua GPT-6.2 Astra por outra versão em delegações solicitadas ao modelo exato."}),Pending::Resume{id:id.clone(),send:Some(outgoing)});
                } else {
                    self.rpc("thread/start",json!({"cwd":outgoing.cwd,"model":outgoing.model,"modelProvider":"openai","ephemeral":self.smoke_chat,"allowProviderModelFallback":false,"runtimeWorkspaceRoots":[outgoing.cwd],"sandbox":outgoing.access.sandbox(),"approvalPolicy":"on-request","approvalsReviewer":"user","serviceName":"rustgpeitinhos","developerInstructions":"Responda em português quando apropriado. Não use skills Superpowers automaticamente; apenas se o usuário solicitar explicitamente. Não substitua GPT-6.2 Astra por outra versão em delegações solicitadas ao modelo exato."}),Pending::Start(outgoing));
                }
            }
            Pending::Start(mut outgoing) => {
                if !self.valid_submission(&outgoing) {
                    return;
                }
                if result["modelProvider"].as_str() != Some("openai") {
                    self.fail_submission(
                        &outgoing,
                        "O Codex retornou outro provedor. Envio bloqueado.".into(),
                    );
                    return;
                }
                if result["model"].as_str() != Some(outgoing.model.as_str()) {
                    self.fail_submission(
                        &outgoing,
                        "O runtime mudou o modelo solicitado. O envio foi bloqueado.".into(),
                    );
                    return;
                }
                let Some(id) = result["thread"]["id"].as_str().map(str::to_owned) else {
                    self.fail_submission(
                        &outgoing,
                        "O Codex não retornou o ID da conversa.".into(),
                    );
                    return;
                };
                outgoing.thread = Some(id.clone());
                self.state.current_thread = Some(id.clone());
                self.state.items.clear();
                if let Ok(info) = serde_json::from_value(result["thread"].clone()) {
                    self.state.threads.insert(0, info);
                }
                self.save_preferences();
                self.send_turn(outgoing);
            }
            Pending::Resume { id, send } => {
                if let Some(outgoing) = send {
                    if !self.valid_submission(&outgoing) {
                        return;
                    }
                    if result["modelProvider"].as_str() != Some("openai") {
                        self.fail_submission(
                            &outgoing,
                            "Esta conversa pertence a outro provedor. Envio bloqueado.".into(),
                        );
                        return;
                    }
                    if result["model"].as_str() != Some(outgoing.model.as_str()) {
                        self.fail_submission(
                            &outgoing,
                            "O runtime mudou o modelo solicitado. O envio foi bloqueado.".into(),
                        );
                        return;
                    }
                    self.send_turn(outgoing);
                } else if self.state.current_thread.as_deref() == Some(&id) {
                    self.load_history(id, false);
                }
            }
            Pending::History { id, older, .. } => {
                let mut items: Vec<DisplayItem> = result["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|entry| display_item(&entry["item"], false))
                    .collect();
                items.reverse();
                self.history_cursor = result["nextCursor"].as_str().map(str::to_owned);
                self.state.more_history = self.history_cursor.is_some();
                if older {
                    items = prepend_history(items, &mut self.state.items);
                }
                self.state.items = items;
                self.state.loading_history = false;
                self.finish_history_load(&id);
            }
            Pending::LegacyHistory { id, older, .. } => {
                let mut items = result["thread"]["turns"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|turn| turn["items"].as_array().into_iter().flatten())
                    .filter_map(|item| display_item(item, false))
                    .collect();
                if older || self.active.contains_key(&id) {
                    items = prepend_history(items, &mut self.state.items);
                }
                self.state.items = items;
                self.history_cursor = None;
                self.state.more_history = false;
                self.state.loading_history = false;
                self.finish_history_load(&id);
            }
            Pending::Turn { id, submission } => {
                if submission.generation == self.send_generation {
                    self.send_in_flight = false;
                }
                if let Some(turn) = result["turn"]["id"].as_str() {
                    if !self.completed_turns.contains(turn) {
                        self.active.insert(id.clone(), turn.to_owned());
                    }
                }
                if self.cancelled.contains(&submission.generation) || !self.state.connected {
                    if let Some(turn) = result["turn"]["id"].as_str() {
                        if !self.completed_turns.contains(turn) {
                            self.rpc(
                                "turn/interrupt",
                                json!({"threadId":id,"turnId":turn}),
                                Pending::Ignore,
                            );
                        }
                    }
                }
                if self.state.current_thread.as_deref() == Some(&id) {
                    self.clear_sent_draft(&id, &submission);
                    self.state.busy = self.active.contains_key(&id);
                }
            }
            Pending::Steer {
                id,
                turn,
                submission,
            } => {
                if submission.generation == self.send_generation {
                    self.send_in_flight = false;
                }
                if result["turnId"].as_str() != Some(turn.as_str()) {
                    self.fail("O Codex não confirmou o trabalho esperado. Sua mensagem foi mantida; confira a conversa antes de reenviar.".into());
                    return;
                }
                if self.cancelled.contains(&submission.generation) || !self.state.connected {
                    self.rpc(
                        "turn/interrupt",
                        json!({"threadId":id,"turnId":turn}),
                        Pending::Ignore,
                    );
                }
                self.clear_sent_draft(&id, &submission);
                self.smoke_steer_accepted = self.smoke_steer;
            }
            Pending::TerminalExec(process) => self.terminal.finish(&process, Ok(result)),
            Pending::TerminalInput { process, text } => {
                if self.terminal.process_id() == Some(&process)
                    && text.as_deref() == Some(self.terminal.input.as_str())
                {
                    self.terminal.input.clear();
                }
            }
            Pending::ReviewCommand(reply) => {
                let output = result["stdout"].as_str().unwrap_or("");
                let response = if result["exitCode"].as_i64() != Some(0) {
                    Err(format!(
                        "Git: {}",
                        result["stderr"].as_str().unwrap_or("O comando falhou.")
                    ))
                } else if output.len() >= 1024 * 1024 {
                    Err(
                        "O resultado Git excedeu o limite de 1 MiB. Selecione uma pasta menor."
                            .into(),
                    )
                } else {
                    Ok(output.as_bytes().to_vec())
                };
                let _ = reply.send(response);
            }
            Pending::TerminalStop | Pending::Ignore => {}
        }
    }

    fn send_turn(&mut self, outgoing: Outgoing) {
        if !self.valid_submission(&outgoing) {
            return;
        }
        let Some(thread) = outgoing.thread.clone() else {
            self.fail("Conversa não inicializada.".into());
            return;
        };
        let input = Self::user_input(&outgoing);
        // Capture before turn/start: item notifications may precede its reply.
        self.live_history.insert(
            thread.clone(),
            LiveHistory {
                items: self.state.items.clone(),
                cursor: self.history_cursor.clone(),
            },
        );
        self.active_settings.insert(
            thread.clone(),
            ActiveSettings {
                model: outgoing.model.clone(),
                effort: outgoing.effort.clone(),
                access: outgoing.access,
                cwd: outgoing.cwd.clone(),
            },
        );
        let policy = sandbox_policy(outgoing.access, &outgoing.cwd);
        self.rpc("turn/start",json!({"threadId":thread,"input":input,"model":outgoing.model,"effort":outgoing.effort,"cwd":outgoing.cwd,"sandboxPolicy":policy,"approvalPolicy":"on-request","approvalsReviewer":"user"}),Pending::Turn{id:thread,submission:outgoing});
    }

    fn finish_history_load(&mut self, thread: &str) {
        if self.active.contains_key(thread) {
            self.live_history.insert(
                thread.into(),
                LiveHistory {
                    items: self.state.items.clone(),
                    cursor: self.history_cursor.clone(),
                },
            );
        } else {
            // Retain the completed stream until a fresh history read succeeds.
            self.live_history.remove(thread);
        }
    }

    fn clear_sent_draft(&mut self, thread: &str, submission: &Outgoing) {
        if self.state.current_thread.as_deref() != Some(thread) {
            return;
        }
        let same_skills = self
            .state
            .selected_skills
            .iter()
            .map(|s| &s.path)
            .eq(submission.skills.iter().map(|s| &s.path));
        if self.state.draft == submission.text
            && self.state.attachments == submission.files
            && same_skills
        {
            self.state.draft.clear();
            self.state.attachments.clear();
            self.state.selected_skills.clear();
        }
    }

    fn user_input(outgoing: &Outgoing) -> Vec<Value> {
        let mut text = outgoing.text.clone();
        let mut input = vec![];
        let mut files = vec![];
        for file in &outgoing.files {
            let image = Path::new(file)
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|e| {
                    matches!(
                        e.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp" | "gif"
                    )
                });
            if image {
                input.push(json!({"type":"localImage","path":file}));
            } else {
                files.push(file.clone());
            }
        }
        if !files.is_empty() {
            text.push_str("\n\nArquivos selecionados pelo usuário para esta solicitação (leia os caminhos quando necessário):\n");
            text.push_str(&files.join("\n"));
        }
        input.insert(0, json!({"type":"text","text":text}));
        for skill in &outgoing.skills {
            input.push(json!({"type":"skill","name":skill.name,"path":skill.path}));
        }
        input
    }

    fn notification(&mut self, message: Value) {
        let method = message["method"].as_str().unwrap_or("");
        if self.smoke {
            eprintln!(
                "rustgpeitinhos +{}ms event {method}",
                self.startup.elapsed().as_millis()
            );
        }
        let p = &message["params"];
        if method == "account/rateLimits/updated" {
            if self.state.usage.apply_update(p) && self.state.connected {
                self.fetch_usage();
            }
            return;
        }
        if method == "command/exec/outputDelta" {
            self.terminal.notification(p);
            return;
        }
        if message.get("id").is_some() {
            let prompt = ServerPrompt {
                id: message["id"].clone(),
                method: method.into(),
                params: p.clone(),
                fields: BTreeMap::new(),
            };
            if self.state.prompt.is_some() {
                self.prompts.push_back(prompt);
            } else {
                self.state.prompt = Some(prompt);
            }
            return;
        }
        if method == "serverRequest/resolved" {
            let id = &p["requestId"];
            self.prompts.retain(|prompt| &prompt.id != id);
            if self
                .state
                .prompt
                .as_ref()
                .is_some_and(|prompt| &prompt.id == id)
            {
                self.state.prompt = self.prompts.pop_front();
            }
            return;
        }
        if method == "account/updated" {
            if self.account_identity.is_some() || p["authMode"].as_str() != Some("chatgpt") {
                self.cancelled.insert(self.send_generation);
                self.send_in_flight = false;
                self.state.connected = false;
                self.state.usage.reset();
                self.live_history.clear();
                self.history_generation = self.history_generation.saturating_add(1);
                self.state.loading_history = false;
                self.provider_valid = false;
                self.fail(
                    "A sessão ChatGPT foi alterada ou encerrada. Reconecte antes de continuar."
                        .into(),
                );
                let active: Vec<_> = self
                    .active
                    .iter()
                    .map(|(t, id)| (t.clone(), id.clone()))
                    .collect();
                for (thread, turn) in active {
                    self.rpc(
                        "turn/interrupt",
                        json!({"threadId":thread,"turnId":turn}),
                        Pending::Ignore,
                    );
                }
            }
            return;
        }
        if method == "skills/changed" {
            self.fetch_skills();
            return;
        }
        if method == "model/rerouted" {
            let from = p["fromModel"].as_str().unwrap_or("modelo solicitado");
            let to = p["toModel"].as_str().unwrap_or("outro modelo");
            self.fail(format!("O Codex redirecionou {from} para {to}. A interrupção foi solicitada para preservar sua escolha de modelo."));
            if let (Some(thread), Some(turn)) = (p["threadId"].as_str(), p["turnId"].as_str()) {
                self.rpc(
                    "turn/interrupt",
                    json!({"threadId":thread,"turnId":turn}),
                    Pending::Ignore,
                );
            }
            return;
        }
        if method == "thread/started" {
            if let Ok(info) = serde_json::from_value::<ThreadInfo>(p["thread"].clone()) {
                if !self.state.threads.iter().any(|t| t.id == info.id) {
                    self.state.threads.insert(0, info);
                }
            }
            return;
        }
        let Some(thread) = p["threadId"].as_str() else {
            return;
        };
        if method == "turn/started" {
            if let Some(turn) = p["turn"]["id"].as_str() {
                self.active.insert(thread.into(), turn.into());
            }
        }
        if method == "turn/completed" {
            self.active.remove(thread);
            self.active_settings.remove(thread);
            if let Some(history) = self.live_history.get_mut(thread) {
                for item in &mut history.items {
                    item.running = false;
                }
            }
            if let Some(id) = p["turn"]["id"].as_str() {
                self.completed_turns.insert(id.into());
            }
            if self.state.current_thread.as_deref() == Some(thread) {
                self.state.busy = false;
                if p["turn"]["status"].as_str() == Some("failed") {
                    self.state.error = Some(
                        p["turn"]["error"]["message"]
                            .as_str()
                            .unwrap_or("O turno falhou.")
                            .into(),
                    );
                }
                for item in &mut self.state.items {
                    item.running = false;
                }
            }
            self.fetch_threads(false);
            return;
        }
        if let Some(history) = self.live_history.get_mut(thread) {
            update_display_items(&mut history.items, method, p);
        }
        if self.state.current_thread.as_deref() != Some(thread) {
            return;
        }
        update_display_items(&mut self.state.items, method, p);
        if method == "error" {
            self.fail(
                p["error"]["message"]
                    .as_str()
                    .unwrap_or("Erro no turno.")
                    .into(),
            );
        }
    }

    fn action(&mut self, action: UiAction) {
        if self.send_in_flight
            && matches!(
                &action,
                UiAction::NewChat
                    | UiAction::SelectThread(_)
                    | UiAction::Folder(_)
                    | UiAction::SelectProject(_)
            )
        {
            self.fail("Aguarde a preparação do envio ou clique em Interromper antes de trocar de conversa.".into());
            return;
        }
        match action {
            UiAction::RefreshUsage => {
                if self.state.connected {
                    self.fetch_usage();
                }
            }
            UiAction::ClearProjectFilter => self.state.project_filter = None,
            UiAction::SelectProject(path) => {
                if !self.state.projects.iter().any(|p| p.path == path) {
                    return;
                }
                if !Path::new(&path).is_dir() {
                    self.fail("A pasta do projeto não está disponível.".into());
                    return;
                }
                self.action(UiAction::Folder(path.clone()));
                self.state.project_filter = Some(path);
                self.state.sidebar_view = SidebarView::Conversations;
                self.state.search.clear();
            }
            UiAction::CreateProject { parent, name } => {
                if let Err(e) = self
                    .project_worker
                    .submit(ProjectOperation::Create { parent, name })
                {
                    self.fail(e);
                }
                self.state.project_busy = self.project_worker.is_busy();
            }
            UiAction::AddProject(path) => {
                if let Err(e) = self
                    .project_worker
                    .submit(ProjectOperation::AddExisting { path })
                {
                    self.fail(e);
                }
                self.state.project_busy = self.project_worker.is_busy();
            }
            UiAction::NewChat => {
                self.history_generation = self.history_generation.saturating_add(1);
                self.state.current_thread = None;
                self.state.items.clear();
                self.state.busy = false;
                self.state.loading_history = false;
                self.state.error = None;
                self.history_cursor = None;
                self.state.more_history = false;
            }
            UiAction::SelectThread(id) => {
                let Some(thread) = self.state.threads.iter().find(|t| t.id == id).cloned() else {
                    return;
                };
                self.state.current_thread = Some(id.clone());
                self.state.cwd = thread.cwd;
                self.state.folder_edit = self.state.cwd.clone();
                self.state.items.clear();
                self.state.error = None;
                self.state.busy = self.active.contains_key(&id);
                if self.state.busy {
                    if let Some(settings) = self.active_settings.get(&id) {
                        self.state.model = settings.model.clone();
                        self.state.effort = settings.effort.clone();
                        self.state.access = settings.access;
                        self.state.cwd = settings.cwd.clone();
                        self.state.folder_edit = self.state.cwd.clone();
                    }
                }
                self.history_cursor = None;
                self.state.more_history = false;
                self.load_history(id, false);
                self.fetch_skills();
            }
            UiAction::Send => {
                if !self.state.connected
                    || self.send_in_flight
                    || self.state.loading_history
                    || self.state.prompt.is_some()
                {
                    return;
                }
                if self.state.draft.trim().is_empty() && self.state.attachments.is_empty() {
                    return;
                }
                let Some(model) = self
                    .state
                    .models
                    .iter()
                    .find(|m| m.model == self.state.model)
                else {
                    self.fail("Escolha um modelo retornado pelo Codex.".into());
                    return;
                };
                if !model
                    .supported_reasoning_efforts
                    .iter()
                    .any(|e| e.reasoning_effort == self.state.effort)
                {
                    self.fail("Este modelo não oferece o nível de raciocínio escolhido.".into());
                    return;
                }
                let cwd = match std::fs::canonicalize(&self.state.cwd) {
                    Ok(p) if p.is_dir() => p.to_string_lossy().into_owned(),
                    _ => {
                        self.fail("Escolha uma pasta existente para trabalhar.".into());
                        return;
                    }
                };
                self.send_in_flight = true;
                self.state.error = None;
                self.send_generation = self.send_generation.saturating_add(1);
                self.rpc(
                    "account/read",
                    json!({"refreshToken":false}),
                    Pending::Verify(Outgoing {
                        thread: self.state.current_thread.clone(),
                        cwd,
                        model: self.state.model.clone(),
                        effort: self.state.effort.clone(),
                        access: self.state.access,
                        text: self.state.draft.clone(),
                        files: self.state.attachments.clone(),
                        skills: self.state.selected_skills.clone(),
                        generation: self.send_generation,
                        expected_turn: self
                            .state
                            .current_thread
                            .as_ref()
                            .and_then(|id| self.active.get(id))
                            .cloned(),
                    }),
                );
            }
            UiAction::Interrupt => {
                self.cancelled.insert(self.send_generation);
                let awaiting_turn=self.pending.values().any(|(p,_)|matches!(p,Pending::Turn{submission,..}|Pending::Steer{submission,..}if submission.generation==self.send_generation));
                if !awaiting_turn {
                    self.send_in_flight = false;
                }
                if let Some(id) = self.state.current_thread.clone() {
                    if let Some(turn) = self.active.get(&id) {
                        self.rpc(
                            "turn/interrupt",
                            json!({"threadId":id,"turnId":turn}),
                            Pending::Ignore,
                        );
                    }
                }
            }
            UiAction::Refresh => {
                if !self.state.connected {
                    self.runtime.take();
                    self.terminal.disconnected();
                    self.pending.clear();
                    self.active.clear();
                    self.active_settings.clear();
                    self.live_history.clear();
                    self.history_generation = self.history_generation.saturating_add(1);
                    self.prompts.clear();
                    self.state.prompt = None;
                    self.account_identity = None;
                    self.provider_valid = false;
                    self.state.plan = None;
                    self.state.usage.reset();
                    self.state.error = None;
                    self.send_in_flight = false;
                    self.state.busy = false;
                    self.state.loading_history = false;
                    self.state.current_thread = None;
                    self.state.threads.clear();
                    self.state.items.clear();
                    self.state.status = "Reconectando ao Codex…".into();
                    let (tx, rx) = mpsc::channel();
                    self.starting = rx;
                    let root = self.root.clone();
                    let binary = self.binary.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(Runtime::spawn(&binary, &root));
                    });
                } else {
                    self.fetch_threads(false);
                    self.fetch_skills();
                    self.fetch_usage();
                    self.rpc(
                        "model/list",
                        json!({"limit":100,"includeHidden":false}),
                        Pending::Models,
                    );
                }
            }
            UiAction::MoreThreads => self.fetch_threads(true),
            UiAction::MoreHistory => {
                if let Some(id) = self.state.current_thread.clone() {
                    self.load_history(id, true);
                }
            }
            UiAction::Folder(folder) => {
                if !Path::new(&folder).is_dir() {
                    self.fail("A pasta selecionada não existe.".into());
                    return;
                }
                self.state.cwd = folder.clone();
                self.history_generation = self.history_generation.saturating_add(1);
                self.state.project_filter = None;
                self.state.folder_edit = folder;
                self.state.current_thread = None;
                self.state.items.clear();
                self.state.busy = false;
                self.state.loading_history = false;
                self.history_cursor = None;
                self.state.more_history = false;
                self.fetch_skills();
                self.save_preferences();
            }
            UiAction::Attach(files) => {
                for file in files {
                    if Path::new(&file).is_file() && !self.state.attachments.contains(&file) {
                        self.state.attachments.push(file);
                    }
                }
            }
            UiAction::OpenWeb => open_path("https://chatgpt.com/"),
            UiAction::OpenPath(path) => open_path(&path),
            UiAction::OpenOriginal => open_path("codex://"),
            UiAction::Respond { id, result } => {
                if self.state.prompt.as_ref().is_none_or(|p| p.id != id) {
                    return;
                }
                match self
                    .runtime
                    .as_ref()
                    .ok_or_else(|| "O Codex está desconectado.".to_owned())
                    .and_then(|r| r.respond(id, result))
                {
                    Ok(()) => self.state.prompt = self.prompts.pop_front(),
                    Err(error) => self.fail(error),
                }
            }
            UiAction::Reject { id, message } => {
                if self.state.prompt.as_ref().is_none_or(|p| p.id != id) {
                    return;
                }
                match self
                    .runtime
                    .as_ref()
                    .ok_or_else(|| "O Codex está desconectado.".to_owned())
                    .and_then(|r| r.reject(id, &message))
                {
                    Ok(()) => self.state.prompt = self.prompts.pop_front(),
                    Err(error) => self.fail(error),
                }
            }
        }
    }

    fn pump(&mut self) {
        if let Some(result) = self.project_worker.poll() {
            match result {
                Ok(project) => {
                    if !self.state.projects.iter().any(|p| p.path == project.path) {
                        self.state.projects.push(project);
                        self.state.projects.sort_by_key(|p| p.name.to_lowercase());
                    }
                    self.state.sidebar_view = SidebarView::Projects;
                    self.save_preferences();
                }
                Err(error) => self.fail(error),
            }
        }
        self.state.project_busy = self.project_worker.is_busy();
        self.review.pump();
        for command in self.review.take_commands() {
            if self
                .pending
                .values()
                .filter(|(pending, _)| matches!(pending, Pending::ReviewCommand(_)))
                .count()
                >= 2
            {
                let _ = command.reply.send(Err(
                    "A revisão anterior ainda está encerrando. Atualize em alguns segundos.".into(),
                ));
                continue;
            }
            if !self.state.connected {
                let _ = command
                    .reply
                    .send(Err("Conecte o Codex e atualize a revisão.".into()));
                continue;
            }
            self.rpc(
                "command/exec",
                json!({
                    "command":command.command,"cwd":command.cwd,"env":command.env,
                    "sandboxPolicy":sandbox_policy(AccessMode::ReadOnly,&command.cwd),
                    "timeoutMs":8000,"outputBytesCap":1024*1024,
                }),
                Pending::ReviewCommand(command.reply),
            );
        }
        if self.runtime.is_none() {
            if let Ok(result) = self.starting.try_recv() {
                match result {
                    Ok(runtime) => {
                        self.runtime = Some(runtime);
                        self.rpc("initialize",json!({"clientInfo":{"name":"rustgpeitinhos","title":"RUST GPeitinhos","version":"0.1.0"},"capabilities":{"experimentalApi":true,"mcpServerOpenaiFormElicitation":true}}),Pending::Initialize);
                    }
                    Err(error) => {
                        self.state.status = "Falha ao iniciar o Codex".into();
                        self.fail(error);
                    }
                }
            }
        }
        for _ in 0..256 {
            let event = self.runtime.as_ref().and_then(|r| r.events.try_recv().ok());
            match event {
                Some(RuntimeEvent::Message(message)) => {
                    if message.get("method").is_some() {
                        self.notification(message);
                    } else if let Some(id) = message["id"].as_u64() {
                        self.response(id, message);
                    }
                }
                Some(RuntimeEvent::Disconnected(error)) => {
                    self.terminal.disconnected();
                    self.state.connected = false;
                    self.state.status = "Desconectado".into();
                    self.fail(error);
                    break;
                }
                None => break,
            }
        }
        let expired: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, (pending, at))| {
                !matches!(pending, Pending::TerminalExec(_))
                    && at.elapsed() > Duration::from_secs(60)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some((p, _)) = self.pending.remove(&id) {
                if matches!(&p, Pending::Turn { .. } | Pending::Steer { .. }) {
                    // A aceitação pode ter ocorrido. Encerrar nossa conexão
                    // impede que um turno incerto continue sem controle.
                    self.runtime.take();
                    self.terminal.disconnected();
                    self.state.connected = false;
                    self.provider_valid = false;
                    self.active.clear();
                    self.state.status = "Conexão encerrada após envio sem confirmação".into();
                }
                if p.generation() == Some(self.send_generation) {
                    self.cancelled.insert(self.send_generation);
                    self.send_in_flight = false;
                }
                if let Pending::History { id, generation, .. }
                | Pending::LegacyHistory { id, generation, .. } = &p
                {
                    if self.current_history(id, *generation) {
                        self.state.loading_history = false;
                    } else {
                        continue;
                    }
                }
                if matches!(p, Pending::Threads(_)) {
                    self.state.threads_loading = false;
                }
            }
            self.fail("O Codex não respondeu dentro do prazo. O pedido não será reenviado automaticamente.".into());
        }
        self.state.busy = self.send_in_flight
            || self
                .state
                .current_thread
                .as_ref()
                .is_some_and(|id| self.active.contains_key(id));
        self.state.preparing = self.send_in_flight;
        if self.state.connected && self.last_usage.elapsed() >= Duration::from_secs(60) {
            self.fetch_usage();
        }
    }

    fn terminal_action(&mut self, action: TerminalAction) {
        match action {
            TerminalAction::Run => {
                if !self.state.connected || !self.provider_valid || self.state.prompt.is_some() {
                    self.terminal.error = Some("Conecte a assinatura ChatGPT e responda aos pedidos pendentes antes de executar.".into());
                    return;
                }
                let cwd = match std::fs::canonicalize(&self.state.cwd) {
                    Ok(path) if path.is_dir() => path.to_string_lossy().into_owned(),
                    _ => {
                        self.terminal.error = Some("Escolha uma pasta existente.".into());
                        return;
                    }
                };
                if let Some((process, params)) = self.terminal.start(&cwd, self.state.access) {
                    self.rpc("command/exec", params, Pending::TerminalExec(process));
                }
            }
            TerminalAction::Stop => {
                if let Some(process) = self.terminal.process_id().map(str::to_owned) {
                    self.rpc(
                        "command/exec/terminate",
                        json!({"processId":process}),
                        Pending::TerminalStop,
                    );
                }
            }
            TerminalAction::Input | TerminalAction::CloseInput => {
                let close = matches!(action, TerminalAction::CloseInput);
                if let (Some(process), Some(params)) = (
                    self.terminal.process_id().map(str::to_owned),
                    self.terminal.write_params(close),
                ) {
                    self.rpc(
                        "command/exec/write",
                        params,
                        Pending::TerminalInput {
                            process,
                            text: if close {
                                None
                            } else {
                                Some(self.terminal.input.clone())
                            },
                        },
                    );
                }
            }
            TerminalAction::Clear => self.terminal.clear(),
        }
    }
}

impl eframe::App for RustGPeitinhosApp {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if !self.first_frame {
            self.first_frame = true;
            if self.smoke {
                eprintln!(
                    "rustgpeitinhos: first UI frame after app creation: {} ms",
                    self.startup.elapsed().as_millis()
                );
            }
        }
        self.pump();
        if self.smoke_terminal && !self.smoke_sent && self.state.connected {
            self.smoke_sent = true;
            self.workbench.open = true;
            self.state.access = AccessMode::ReadOnly;
            self.terminal.command =
                r#"printf '%s\n' 'RustGPeitinhos — terminal integrado' 'UTF-8: ação, revisão, ç'"#
                    .into();
            self.terminal_action(TerminalAction::Run);
        }
        if self.smoke_chat
            && !self.smoke_sent
            && self.state.connected
            && !self.state.models.is_empty()
            && !self.state.skills.is_empty()
        {
            self.smoke_sent = true;
            if std::env::var_os("PONTE_AGENT_TEST_DIR").is_some() {
                self.state.access = AccessMode::Folder;
                self.state.draft="Teste de integração autorizado somente dentro da pasta atual. Sem usar skills e sem acessar a internet: crie prova.txt contendo exatamente PONTE_AGENT_OK, execute cat prova.txt para conferir e responda somente PONTE_AGENT_OK. Não altere nenhum outro arquivo.".into();
            } else if self.smoke_steer {
                self.state.access = AccessMode::ReadOnly;
                self.state.draft="Teste de integração autorizado. Sem acessar arquivos, internet ou skills: execute apenas sleep 4 no terminal antes de responder. Aguarde instruções complementares; a resposta final será definida por elas.".into();
            } else {
                self.state.access = AccessMode::ReadOnly;
                self.state.draft="Responda somente PONTE_OK. Este é um teste de integração. Não use ferramentas, não execute comandos e não use skills.".into();
            }
            self.action(UiAction::Send);
        }
        if self.smoke_steer
            && self.smoke_sent
            && !self.smoke_steered
            && !self.send_in_flight
            && self
                .state
                .current_thread
                .as_ref()
                .is_some_and(|id| self.active.contains_key(id))
        {
            self.smoke_steered = true;
            self.state.draft="Instrução complementar deste teste: responda somente PONTE_OK. Não faça nenhuma outra ação.".into();
            self.action(UiAction::Send);
        }
        let actions = self.ui.render(
            root,
            &mut self.state,
            &mut self.workbench,
            &mut self.terminal,
            &mut self.review,
        );
        for action in actions {
            self.action(action);
        }
        let terminal_actions = std::mem::take(&mut self.workbench.actions);
        for action in terminal_actions {
            self.terminal_action(action);
        }
        let ready = self.state.connected
            && !self.state.models.is_empty()
            && !self.state.skills.is_empty()
            && !self.state.threads_loading;
        let agent_test = std::env::var_os("PONTE_AGENT_TEST_DIR").is_some();
        let terminal_ok = self.smoke_sent
            && !self.terminal.running
            && self.terminal.exit_code == Some(0)
            && self.terminal.output.contains("UTF-8: ação, revisão, ç\n");
        let marker = if agent_test {
            "PONTE_AGENT_OK"
        } else {
            "PONTE_OK"
        };
        let chat_ok = (!self.smoke_steer || self.smoke_steer_accepted)
            && self
                .state
                .items
                .iter()
                .any(|i| i.kind == "agentMessage" && i.text.contains(marker))
            && !self.state.busy
            && (!agent_test
                || std::fs::read_to_string(Path::new(&self.state.cwd).join("prova.txt"))
                    .is_ok_and(|s| s.trim() == marker));
        if self.smoke
            && !self.smoke_done
            && ((ready
                && (!self.smoke_chat || chat_ok || self.state.error.is_some())
                && (!self.smoke_terminal || terminal_ok || self.terminal.error.is_some()))
                || self.startup.elapsed()
                    > Duration::from_secs(if self.smoke_chat { 150 } else { 50 }))
        {
            self.smoke_done = true;
            println!(
                "{}",
                json!({"smoke":ready&&(!self.smoke_chat||chat_ok)&&(!self.smoke_terminal||terminal_ok),"chatTest":if self.smoke_chat{json!(chat_ok)}else{Value::Null},"terminalTest":if self.smoke_terminal{json!(terminal_ok)}else{Value::Null},"models":self.state.models.len(),"threads":self.state.threads.len(),"skills":self.state.skills.len(),"elapsedMs":self.startup.elapsed().as_millis(),"error":self.state.error,"terminalError":self.terminal.error})
            );
            if let Some(path) = std::env::var_os("PONTE_SCREENSHOT") {
                let tx = self.screenshot_sender.clone();
                let ctx = root.ctx().clone();
                root.ctx().request_screenshot(move |pixels| {
                    let rgba: Vec<u8> = pixels.pixels.iter().flat_map(|p| p.to_array()).collect();
                    let _ = image::save_buffer(
                        Path::new(&path),
                        &rgba,
                        pixels.size[0] as u32,
                        pixels.size[1] as u32,
                        image::ColorType::Rgba8,
                    );
                    let _ = tx.send(());
                    ctx.request_repaint();
                });
                self.screenshot_requested = true;
            } else {
                root.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.screenshot_requested
            && !self.screenshot_done
            && self.screenshot_receiver.try_recv().is_ok()
        {
            self.screenshot_done = true;
            root.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        root.ctx().request_repaint_after(
            if self.state.busy
                || self.terminal.running
                || self.review.loading
                || self.review.diff_loading
            {
                Duration::from_millis(35)
            } else {
                Duration::from_millis(180)
            },
        );
    }
}

fn open_path(path: &str) {
    // Nunca interpretar a URL ou o caminho como código de shell.
    if path.starts_with('-') || path.contains('\0') {
        return;
    }
    let _ = std::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn prepend_history(
    mut history: Vec<DisplayItem>,
    current: &mut Vec<DisplayItem>,
) -> Vec<DisplayItem> {
    // Cursors can be inclusive. Existing items own the latest streamed text,
    // so neither a boundary duplicate nor a stale history row replaces them.
    let mut seen: HashSet<String> = current.iter().map(|item| item.id.clone()).collect();
    history.retain(|item| seen.insert(item.id.clone()));
    history.append(current);
    history
}

fn update_display_items(items: &mut Vec<DisplayItem>, method: &str, params: &Value) {
    if matches!(method, "item/started" | "item/completed") {
        if let Some(item) = display_item(&params["item"], method == "item/started") {
            if let Some(existing) = items.iter_mut().find(|i| i.id == item.id) {
                *existing = item;
            } else {
                items.push(item);
            }
        }
    } else if matches!(
        method,
        "item/agentMessage/delta"
            | "item/commandExecution/outputDelta"
            | "item/reasoning/summaryTextDelta"
            | "item/reasoning/textDelta"
    ) {
        let id = params["itemId"].as_str().unwrap_or("unknown");
        let delta = params["delta"].as_str().unwrap_or("");
        if let Some(item) = items.iter_mut().find(|i| i.id == id) {
            item.text.push_str(delta);
        } else {
            items.push(DisplayItem {
                id: id.into(),
                kind: if method.contains("agentMessage") {
                    "agentMessage"
                } else if method.contains("reasoning") {
                    "reasoning"
                } else {
                    "commandExecution"
                }
                .into(),
                text: delta.into(),
                running: true,
                raw: json!({}),
            });
        }
    }
}

pub fn display_item(item: &Value, running: bool) -> Option<DisplayItem> {
    let kind = item["type"].as_str()?.to_owned();
    let id = item["id"].as_str()?.to_owned();
    let text = match kind.as_str() {
        "agentMessage" => item["text"].as_str().unwrap_or("").into(),
        "userMessage" => item["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["text"].as_str().or_else(|| c["path"].as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        "commandExecution" => format!(
            "{}\n{}",
            item["command"].as_str().unwrap_or("Comando"),
            item["aggregatedOutput"].as_str().unwrap_or("")
        ),
        "reasoning" => item["summary"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => serde_json::to_string_pretty(item).unwrap_or_default(),
    };
    Some(DisplayItem {
        id,
        kind,
        text,
        running,
        raw: item.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (RustGPeitinhosApp, Receiver<Vec<u8>>, Outgoing) {
        let (runtime, wire) = Runtime::test_transport();
        let (_start, starting) = mpsc::channel();
        let (screenshot_sender, screenshot_receiver) = mpsc::channel();
        let context = egui::Context::default();
        let mut state = UiState::new("/project".into());
        state.connected = true;
        state.plan = Some("plus".into());
        state.draft = "Enviado".into();
        let app = RustGPeitinhosApp {
            state,
            ui: Ui::new(&context),
            runtime: Some(runtime),
            starting,
            pending: HashMap::new(),
            active: HashMap::new(),
            active_settings: HashMap::new(),
            live_history: HashMap::new(),
            completed_turns: HashSet::new(),
            prompts: VecDeque::new(),
            threads_cursor: None,
            history_cursor: None,
            history_generation: 0,
            data: PathBuf::from("/unused-test-data"),
            account_identity: Some("test-account".into()),
            provider_valid: true,
            root: PathBuf::from("/project"),
            binary: PathBuf::from("/codex"),
            send_in_flight: true,
            startup: Instant::now(),
            smoke: false,
            smoke_done: false,
            first_frame: false,
            screenshot_requested: false,
            screenshot_done: false,
            screenshot_receiver,
            screenshot_sender,
            smoke_chat: false,
            smoke_terminal: false,
            smoke_steer: false,
            smoke_steered: false,
            smoke_steer_accepted: false,
            smoke_sent: false,
            send_generation: 1,
            cancelled: HashSet::new(),
            terminal: Terminal::default(),
            review: WorkspaceReview::new(),
            workbench: WorkbenchUi::new(),
            project_worker: ProjectWorker::new(),
            last_usage: Instant::now(),
        };
        let submission = Outgoing {
            thread: Some("thread".into()),
            cwd: "/project".into(),
            model: "gpt-6.2-sol".into(),
            effort: "high".into(),
            access: AccessMode::Folder,
            text: "Enviado".into(),
            files: vec![],
            skills: vec![],
            generation: 1,
            expected_turn: None,
        };
        (app, wire, submission)
    }
    fn start_response(id: u64) -> Value {
        json!({"id":id,"result":{"modelProvider":"openai","model":"gpt-6.2-sol","thread":{"id":"thread","cwd":"/project"}}})
    }
    fn turn_response(id: u64) -> Value {
        json!({"id":id,"result":{"turn":{"id":"turn","status":"inProgress"}}})
    }

    #[test]
    fn account_invalidation_blocks_an_already_prepared_send() {
        let (mut app, wire, submission) = fixture();
        app.pending
            .insert(90, (Pending::Start(submission), Instant::now()));
        app.notification(json!({"method":"account/updated","params":{"authMode":"apikey"}}));
        app.response(90, start_response(90));
        assert!(!app.state.connected);
        assert!(wire.try_recv().is_err());
    }
    #[test]
    fn interruption_during_preparation_prevents_turn_start() {
        let (mut app, wire, submission) = fixture();
        app.pending
            .insert(90, (Pending::Start(submission), Instant::now()));
        app.action(UiAction::Interrupt);
        app.response(90, start_response(90));
        assert!(!app.send_in_flight);
        assert!(wire.try_recv().is_err());
        assert_eq!(app.state.draft, "Enviado");
    }
    #[test]
    fn an_unrelated_error_does_not_release_the_send_transaction() {
        let (mut app, _wire, _submission) = fixture();
        app.pending
            .insert(90, (Pending::Skills("/project".into()), Instant::now()));
        app.response(
            90,
            json!({"id":90,"error":{"message":"Catálogo indisponível"}}),
        );
        assert!(app.send_in_flight);
    }
    #[test]
    fn late_turn_acceptance_keeps_new_draft_edits() {
        let (mut app, _wire, submission) = fixture();
        app.state.current_thread = Some("thread".into());
        app.state.draft = "Novo rascunho".into();
        app.pending.insert(
            90,
            (
                Pending::Turn {
                    id: "thread".into(),
                    submission,
                },
                Instant::now(),
            ),
        );
        app.response(90, turn_response(90));
        assert_eq!(app.state.draft, "Novo rascunho");
    }
    #[test]
    fn cancellation_waits_for_the_turn_id_and_then_interrupts_it() {
        let (mut app, wire, submission) = fixture();
        app.pending.insert(
            90,
            (
                Pending::Turn {
                    id: "thread".into(),
                    submission,
                },
                Instant::now(),
            ),
        );
        app.action(UiAction::Interrupt);
        assert!(app.send_in_flight);
        assert!(wire.try_recv().is_err());
        app.response(90, turn_response(90));
        let request: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "turn/interrupt");
        assert_eq!(request["params"]["turnId"], "turn");
    }
    #[test]
    fn completed_turn_is_not_reactivated_by_a_late_start_response() {
        let (mut app, _wire, submission) = fixture();
        app.completed_turns.insert("turn".into());
        app.pending.insert(
            90,
            (
                Pending::Turn {
                    id: "thread".into(),
                    submission,
                },
                Instant::now(),
            ),
        );
        app.response(90, turn_response(90));
        assert!(app.active.is_empty());
        assert!(!app.send_in_flight);
    }
    #[test]
    fn legacy_history_uses_the_compatible_read_contract() {
        let (mut app, wire, _submission) = fixture();
        app.state.current_thread = Some("thread".into());
        app.pending.insert(
            90,
            (
                Pending::History {
                    id: "thread".into(),
                    older: false,
                    generation: 0,
                },
                Instant::now(),
            ),
        );
        app.response(90, json!({"id":90,"error":{"message":"Not paginated"}}));
        let request: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "thread/read");
        assert_eq!(request["params"]["includeTurns"], true);
        let id = request["id"].as_u64().unwrap();
        app.response(id,json!({"id":id,"result":{"thread":{"turns":[{"items":[{"id":"m","type":"agentMessage","text":"Histórico antigo"}]}]}}}));
        assert_eq!(app.state.items[0].text, "Histórico antigo");
    }
    #[test]
    fn ambiguous_usage_updates_request_one_snapshot_without_blocking_chat() {
        let (mut app, wire, _) = fixture();
        app.state.usage.apply_snapshot(&json!({
            "rateLimits":{"limitId":null,"primary":{"usedPercent":10}},
            "rateLimitsByLimitId":{"codex":{"limitId":null,"primary":{"usedPercent":10}}}
        }));
        for _ in 0..2 {
            app.notification(json!({"method":"account/rateLimits/updated","params":{
                "rateLimits":{"limitId":null,"primary":{"usedPercent":20}}
            }}));
        }
        let requests: Vec<Value> = wire
            .try_iter()
            .map(|bytes| serde_json::from_slice(&bytes).unwrap())
            .collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["method"], "account/rateLimits/read");
        assert!(app.state.connected);
        assert!(app.state.error.is_none());
    }

    #[test]
    fn returning_to_active_work_keeps_the_full_stream_without_a_concurrent_snapshot() {
        let (mut app, wire, submission) = fixture();
        app.state.current_thread = Some("thread".into());
        app.state
            .threads
            .push(serde_json::from_value(json!({"id":"thread","cwd":"/project"})).unwrap());
        app.history_cursor = Some("earlier-items".into());
        app.state.more_history = true;
        app.state.items.push(
            display_item(
                &json!({"type":"agentMessage","id":"previous","text":"Prefixo antigo"}),
                false,
            )
            .unwrap(),
        );
        app.send_turn(submission);
        let request: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "turn/start");
        // Item events may precede the turn/start acknowledgement.
        app.notification(json!({"method":"item/started","params":{"threadId":"thread","item":{"id":"message","type":"agentMessage","text":""}}}));
        app.notification(json!({"method":"item/agentMessage/delta","params":{"threadId":"thread","itemId":"message","delta":"antes "}}));
        let request_id = request["id"].as_u64().unwrap();
        app.response(request_id, turn_response(request_id));
        app.action(UiAction::NewChat);
        app.notification(json!({"method":"item/agentMessage/delta","params":{"threadId":"thread","itemId":"message","delta":"fora da conversa"}}));
        app.action(UiAction::SelectThread("thread".into()));
        let requests: Vec<Value> = wire
            .try_iter()
            .map(|bytes| serde_json::from_slice(&bytes).unwrap())
            .collect();
        assert!(requests.iter().all(|request| !matches!(
            request["method"].as_str(),
            Some("thread/items/list" | "thread/read")
        )));
        assert_eq!(app.state.items.len(), 2);
        assert_eq!(app.state.items[0].text, "Prefixo antigo");
        assert_eq!(app.state.items[1].text, "antes fora da conversa");
        assert!(app.state.items[1].running);
        assert_eq!(app.history_cursor.as_deref(), Some("earlier-items"));
        assert!(app.state.more_history);
        assert!(!app.state.loading_history);

        app.notification(json!({"method":"item/completed","params":{"threadId":"thread","item":{"id":"message","type":"agentMessage","text":"antes fora da conversa final"}}}));
        app.notification(json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":"completed"}}}));
        assert!(app.live_history.contains_key("thread"));
        app.action(UiAction::NewChat);
        app.action(UiAction::SelectThread("thread".into()));
        assert_eq!(app.state.items[1].text, "antes fora da conversa final");
        assert!(app.state.loading_history);
        let history: Value = wire
            .try_iter()
            .map(|bytes| serde_json::from_slice::<Value>(&bytes).unwrap())
            .find(|request| request["method"] == "thread/items/list")
            .unwrap();
        let id = history["id"].as_u64().unwrap();
        app.response(id,json!({"id":id,"result":{"data":[{"item":{"id":"message","type":"agentMessage","text":"antes fora da conversa final"}},{"item":{"id":"previous","type":"agentMessage","text":"Prefixo antigo"}}],"nextCursor":null}}));
        assert_eq!(app.state.items.len(), 2);
        assert_eq!(app.state.items[1].text, "antes fora da conversa final");
        assert!(!app.live_history.contains_key("thread"));
    }
    #[test]
    fn stale_same_thread_reads_and_legacy_fallback_cannot_replace_a_newer_view() {
        let (mut app, wire, _) = fixture();
        app.send_in_flight = false;
        app.state.threads = ["a", "b"]
            .into_iter()
            .map(|id| serde_json::from_value(json!({"id":id,"cwd":"/project"})).unwrap())
            .collect();
        let mut reads = vec![];
        for thread in ["a", "b", "a"] {
            app.action(UiAction::SelectThread(thread.into()));
            reads.push(
                wire.try_iter()
                    .map(|bytes| serde_json::from_slice::<Value>(&bytes).unwrap())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .find(|request| request["method"] == "thread/items/list")
                    .unwrap()["id"]
                    .as_u64()
                    .unwrap(),
            );
        }
        app.response(reads[0],json!({"id":reads[0],"result":{"data":[{"item":{"id":"stale","type":"agentMessage","text":"Antigo A"}}]}}));
        assert!(app.state.items.is_empty());
        assert!(app.state.loading_history);
        app.response(
            reads[1],
            json!({"id":reads[1],"error":{"message":"Not paginated"}}),
        );
        assert!(
            wire.try_recv().is_err(),
            "Stale errors must not trigger a fallback"
        );
        app.response(
            reads[2],
            json!({"id":reads[2],"error":{"message":"Not paginated"}}),
        );
        let legacy: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        let old_legacy_id = legacy["id"].as_u64().unwrap();
        assert_eq!(legacy["method"], "thread/read");
        app.action(UiAction::SelectThread("b".into()));
        app.action(UiAction::SelectThread("a".into()));
        let latest: Value = wire
            .try_iter()
            .map(|bytes| serde_json::from_slice::<Value>(&bytes).unwrap())
            .filter(|request| request["method"] == "thread/items/list")
            .last()
            .unwrap();
        app.response(old_legacy_id,json!({"id":old_legacy_id,"result":{"thread":{"turns":[{"items":[{"id":"stale","type":"agentMessage","text":"Antigo legacy A"}]}]}}}));
        assert!(app.state.items.is_empty());
        assert!(app.state.loading_history);
        let latest_id = latest["id"].as_u64().unwrap();
        app.response(latest_id,json!({"id":latest_id,"result":{"data":[{"item":{"id":"new","type":"agentMessage","text":"Atual A"}}]}}));
        assert_eq!(app.state.items[0].text, "Atual A");
        assert!(!app.state.loading_history);
    }
    #[test]
    fn older_history_deduplicates_the_boundary_and_keeps_live_text() {
        let (mut app, wire, _) = fixture();
        app.send_in_flight = false;
        app.state.current_thread = Some("thread".into());
        app.active.insert("thread".into(), "turn".into());
        app.state.items = vec![
            display_item(
                &json!({"id":"message","type":"agentMessage","text":"Prefixo "}),
                true,
            )
            .unwrap(),
        ];
        app.history_cursor = Some("earlier".into());
        app.live_history.insert(
            "thread".into(),
            LiveHistory {
                items: app.state.items.clone(),
                cursor: app.history_cursor.clone(),
            },
        );
        app.action(UiAction::MoreHistory);
        let request: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        let id = request["id"].as_u64().unwrap();
        app.notification(json!({"method":"item/agentMessage/delta","params":{"threadId":"thread","itemId":"message","delta":"novo"}}));
        // An inclusive boundary or fallback can include a stale copy of an
        // item that has already received deltas. Never replay those deltas.
        app.response(id,json!({"id":id,"result":{"data":[{"item":{"id":"message","type":"agentMessage","text":"Prefixo novo"}},{"item":{"id":"older","type":"userMessage","content":[{"text":"Anterior"}]}}],"nextCursor":"oldest"}}));
        assert_eq!(app.state.items.len(), 2);
        assert_eq!(app.state.items[0].id, "older");
        assert_eq!(app.state.items[1].text, "Prefixo novo");
        assert!(app.state.items[1].running);
        assert_eq!(app.live_history["thread"].items[1].text, "Prefixo novo");
        assert_eq!(app.live_history["thread"].cursor.as_deref(), Some("oldest"));

        // A fallback read started during a turn can finish after that turn.
        // It still hydrates older rows and must preserve the final live item.
        app.action(UiAction::MoreHistory);
        let request: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        let id = request["id"].as_u64().unwrap();
        app.response(id, json!({"id":id,"error":{"message":"Not paginated"}}));
        let fallback: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        let fallback_id = fallback["id"].as_u64().unwrap();
        assert_eq!(fallback["method"], "thread/read");
        app.notification(json!({"method":"item/completed","params":{"threadId":"thread","item":{"id":"message","type":"agentMessage","text":"Prefixo novo final"}}}));
        app.notification(json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":"completed"}}}));
        app.response(fallback_id,json!({"id":fallback_id,"result":{"thread":{"turns":[{"items":[{"id":"oldest","type":"agentMessage","text":"Mais antigo"},{"id":"older","type":"agentMessage","text":"Anterior"},{"id":"message","type":"agentMessage","text":"Prefixo"}]}]}}}));
        assert_eq!(app.state.items.len(), 3);
        assert_eq!(app.state.items[0].id, "oldest");
        assert_eq!(app.state.items[2].text, "Prefixo novo final");
        assert!(!app.state.items[2].running);
    }
    #[test]
    fn changing_folder_during_history_load_clears_the_previous_context() {
        let (mut app, _wire, _submission) = fixture();
        app.send_in_flight = false;
        app.smoke = true;
        app.state.current_thread = Some("old-thread".into());
        app.state.loading_history = true;
        app.history_cursor = Some("old-cursor".into());
        app.state.more_history = true;
        app.pending.insert(
            90,
            (
                Pending::LegacyHistory {
                    id: "old-thread".into(),
                    generation: 0,
                    older: false,
                },
                Instant::now(),
            ),
        );
        app.action(UiAction::Folder(env!("CARGO_MANIFEST_DIR").into()));
        app.response(90, json!({"id":90,"result":{"thread":{"turns":[]}}}));
        assert!(!app.state.loading_history);
        assert!(!app.state.more_history);
        assert!(app.history_cursor.is_none());
        assert!(app.state.current_thread.is_none());
    }
    #[test]
    fn new_chat_abandons_history_loading_without_blocking_sending() {
        let (mut app, _wire, _) = fixture();
        app.send_in_flight = false;
        app.state.current_thread = Some("old".into());
        app.state.loading_history = true;
        app.action(UiAction::NewChat);
        assert!(!app.state.loading_history);
        assert!(app.state.current_thread.is_none());
    }
    #[test]
    fn a_preparing_send_cannot_be_redirected_to_another_context() {
        let (mut app, _wire, _) = fixture();
        app.state.current_thread = Some("original".into());
        app.action(UiAction::NewChat);
        assert_eq!(app.state.current_thread.as_deref(), Some("original"));
        app.action(UiAction::Interrupt);
        app.action(UiAction::NewChat);
        assert!(app.state.current_thread.is_none());
    }
    #[test]
    fn failed_prompt_response_preserves_the_request_and_fields() {
        let (mut app, _wire, _) = fixture();
        app.runtime.take();
        app.state.prompt = Some(ServerPrompt {
            id: json!("approval"),
            method: "test".into(),
            params: json!({}),
            fields: BTreeMap::from([("answer".into(), "keep".into())]),
        });
        app.action(UiAction::Respond {
            id: json!("approval"),
            result: json!({"decision":"accept"}),
        });
        assert_eq!(app.state.prompt.as_ref().unwrap().fields["answer"], "keep");
        app.action(UiAction::Reject {
            id: json!("approval"),
            message: "reject".into(),
        });
        assert!(app.state.prompt.is_some());
        assert!(app.state.error.is_some());
    }
    #[test]
    fn uncertain_turn_timeout_closes_our_connection_without_resending() {
        let (mut app, wire, submission) = fixture();
        app.pending.insert(
            90,
            (
                Pending::Turn {
                    id: "thread".into(),
                    submission,
                },
                Instant::now() - Duration::from_secs(61),
            ),
        );
        app.pump();
        assert!(app.runtime.is_none());
        assert!(!app.state.connected);
        assert!(!app.send_in_flight);
        assert!(wire.try_recv().is_err());
    }
    #[test]
    fn a_long_terminal_command_is_not_expired_as_a_normal_rpc() {
        let (mut app, _wire, _) = fixture();
        app.send_in_flight = false;
        app.pending.insert(
            90,
            (
                Pending::TerminalExec("terminal".into()),
                Instant::now() - Duration::from_secs(61),
            ),
        );
        app.pump();
        assert!(app.pending.contains_key(&90));
        assert!(app.state.connected);
    }
    #[test]
    fn runtime_model_redirection_is_visible_and_interrupted() {
        let (mut app, wire, _) = fixture();
        app.notification(json!({"method":"model/rerouted","params":{"threadId":"thread","turnId":"turn","fromModel":"gpt-6.2-sol","toModel":"different","reason":"highRiskCyberActivity"}}));
        let request: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "turn/interrupt");
        assert!(app.state.error.as_ref().unwrap().contains("different"));
    }
    #[test]
    fn terminal_input_acknowledgement_preserves_the_next_typed_line() {
        let (mut app, _wire, _) = fixture();
        app.terminal.command = "cat".into();
        let (process, _) = app
            .terminal
            .start("/project", AccessMode::ReadOnly)
            .unwrap();
        app.terminal.input = "próxima linha".into();
        app.pending.insert(
            90,
            (
                Pending::TerminalInput {
                    process,
                    text: Some("linha enviada".into()),
                },
                Instant::now(),
            ),
        );
        app.response(90, json!({"id":90,"result":{}}));
        assert_eq!(app.terminal.input, "próxima linha");
    }
    #[test]
    fn an_older_folder_skill_list_cannot_replace_the_current_one() {
        let (mut app, _wire, _submission) = fixture();
        app.state.skills = vec![SkillInfo {
            name: "current".into(),
            description: "Atual".into(),
            path: "/project/SKILL.md".into(),
            enabled: true,
        }];
        app.pending.insert(
            90,
            (Pending::Skills("/older-folder".into()), Instant::now()),
        );
        app.response(90, json!({"id":90,"result":{"data":[{"skills":[]}]}}));
        assert_eq!(app.state.skills[0].name, "current");
    }
    #[test]
    fn history_preserves_assistant_and_command_outputs() {
        assert_eq!(
            display_item(
                &json!({"id":"a","type":"agentMessage","text":"Resposta"}),
                false
            )
            .unwrap()
            .text,
            "Resposta"
        );
        let command=display_item(&json!({"id":"c","type":"commandExecution","command":"cargo test","aggregatedOutput":"8 passed"}),false).unwrap();
        assert!(command.text.contains("8 passed"));
    }
    #[test]
    fn active_work_is_steered_after_account_check_without_starting_another_turn() {
        let (mut app, wire, mut submission) = fixture();
        app.state.current_thread = Some("thread".into());
        app.active.insert("thread".into(), "active-turn".into());
        submission.expected_turn = Some("active-turn".into());
        app.pending
            .insert(90, (Pending::Verify(submission), Instant::now()));
        app.response(90,json!({"id":90,"result":{"account":{"type":"chatgpt","planType":"plus","id":"test-account"}}}));
        let request: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "turn/steer");
        assert_eq!(request["params"]["expectedTurnId"], "active-turn");
        assert_eq!(request["params"]["input"][0]["text"], "Enviado");
        assert!(request["params"].get("model").is_none());
        let id = request["id"].as_u64().unwrap();
        app.response(id, json!({"id":id,"result":{"turnId":"active-turn"}}));
        assert!(app.state.draft.is_empty());
        assert_eq!(app.active["thread"], "active-turn");
        assert!(!app.send_in_flight);
        assert!(wire.try_recv().is_err());
    }
    #[test]
    fn steering_preserves_new_edits_and_never_retries_a_finished_turn() {
        let (mut app, wire, mut submission) = fixture();
        app.state.current_thread = Some("thread".into());
        submission.expected_turn = Some("finished".into());
        app.pending
            .insert(90, (Pending::Verify(submission.clone()), Instant::now()));
        app.response(90,json!({"id":90,"result":{"account":{"type":"chatgpt","planType":"plus","id":"test-account"}}}));
        assert_eq!(app.state.draft, "Enviado");
        assert!(!app.send_in_flight);
        assert!(wire.try_recv().is_err());
        app.state.draft = "Edição nova".into();
        app.pending.insert(
            91,
            (
                Pending::Steer {
                    id: "thread".into(),
                    turn: "expected".into(),
                    submission,
                },
                Instant::now(),
            ),
        );
        app.response(91, json!({"id":91,"result":{"turnId":"expected"}}));
        assert_eq!(app.state.draft, "Edição nova");
        assert!(wire.try_recv().is_err());
    }
    #[test]
    fn cancelled_steering_acknowledgement_interrupts_the_expected_work() {
        let (mut app, wire, submission) = fixture();
        app.pending.insert(
            90,
            (
                Pending::Steer {
                    id: "thread".into(),
                    turn: "turn".into(),
                    submission,
                },
                Instant::now(),
            ),
        );
        app.cancelled.insert(app.send_generation);
        app.response(90, json!({"id":90,"result":{"turnId":"turn"}}));
        let request: Value = serde_json::from_slice(&wire.try_recv().unwrap()).unwrap();
        assert_eq!(request["method"], "turn/interrupt");
        assert_eq!(request["params"]["turnId"], "turn");
    }
    #[test]
    fn usage_failure_does_not_disable_chat_and_projects_survive_legacy_preferences() {
        let (mut app, _wire, _) = fixture();
        app.pending.insert(90, (Pending::Usage, Instant::now()));
        app.response(
            90,
            json!({"id":90,"error":{"message":"Limites temporariamente indisponíveis"}}),
        );
        assert!(app.state.connected);
        assert!(app.state.error.is_none());
        let old: Preferences =
            serde_json::from_value(json!({"cwd":"/project","threads":[]})).unwrap();
        assert!(old.projects.is_empty());
        let p = Preferences {
            cwd: "/project".into(),
            threads: vec![],
            projects: vec![Project {
                name: "Projeto".into(),
                path: "/project".into(),
            }],
        };
        let saved: Preferences = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
        assert_eq!(saved.projects[0].name, "Projeto");
    }
    #[test]
    fn unsupported_item_keeps_full_structured_result() {
        let item=display_item(&json!({"id":"m","type":"mcpToolCall","result":{"content":[{"type":"text","text":"Ferramenta"}]}}),false).unwrap();
        assert!(item.text.contains("Ferramenta"));
    }
    #[test]
    fn selecting_a_working_conversation_restores_its_real_model_and_permissions() {
        let (mut app, _wire, _) = fixture();
        app.send_in_flight = false;
        app.state.model = "gpt-6.2-astra".into();
        app.state
            .threads
            .push(serde_json::from_value(json!({"id":"thread","cwd":"/project"})).unwrap());
        app.active.insert("thread".into(), "turn".into());
        app.active_settings.insert(
            "thread".into(),
            ActiveSettings {
                model: "gpt-6.2-sol".into(),
                effort: "high".into(),
                access: AccessMode::Folder,
                cwd: "/project".into(),
            },
        );
        app.action(UiAction::SelectThread("thread".into()));
        assert_eq!(app.state.model, "gpt-6.2-sol");
        assert_eq!(app.state.access, AccessMode::Folder);
    }
    #[test]
    fn saving_projects_reports_a_non_writable_data_directory() {
        let (mut app, _wire, _) = fixture();
        app.data = PathBuf::from("/dev/null/not-a-directory");
        app.save_preferences();
        assert!(
            app.state
                .error
                .as_ref()
                .unwrap()
                .contains("Não foi possível salvar")
        );
    }
}
