use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub model: String,
    pub display_name: String,
    #[serde(default)]
    pub supported_reasoning_efforts: Vec<EffortInfo>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffortInfo {
    pub reasoning_effort: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadInfo {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub model_provider: String,
    #[serde(default)]
    pub history_mode: Option<String>,
}
impl ThreadInfo {
    pub fn title(&self) -> &str {
        self.name
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if self.preview.is_empty() {
                    "Nova conversa"
                } else {
                    &self.preview
                }
            })
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub path: String,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AccessMode {
    ReadOnly,
    #[default]
    Folder,
    Full,
}
impl AccessMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "Somente leitura",
            Self::Folder => "Autonomia na pasta",
            Self::Full => "Acesso completo",
        }
    }
    pub fn sandbox(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::Folder => "workspace-write",
            Self::Full => "danger-full-access",
        }
    }
}
#[derive(Debug, Clone)]
pub struct DisplayItem {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub running: bool,
    pub raw: Value,
}
#[derive(Debug, Clone)]
pub struct ServerPrompt {
    pub id: Value,
    pub method: String,
    pub params: Value,
    pub fields: std::collections::BTreeMap<String, String>,
}

pub struct UiState {
    pub status: String,
    pub connected: bool,
    pub plan: Option<String>,
    pub models: Vec<ModelInfo>,
    pub threads: Vec<ThreadInfo>,
    pub skills: Vec<SkillInfo>,
    pub current_thread: Option<String>,
    pub cwd: String,
    pub model: String,
    pub effort: String,
    pub access: AccessMode,
    pub draft: String,
    pub attachments: Vec<String>,
    pub selected_skills: Vec<SkillInfo>,
    pub items: Vec<DisplayItem>,
    pub busy: bool,
    pub preparing: bool,
    pub usage: crate::usage::UsageState,
    pub projects: Vec<crate::projects::Project>,
    pub project_filter: Option<String>,
    pub project_busy: bool,
    pub sidebar_view: SidebarView,
    pub loading_history: bool,
    pub error: Option<String>,
    pub notices: Vec<String>,
    pub prompt: Option<ServerPrompt>,
    pub more_threads: bool,
    pub threads_loading: bool,
    pub more_history: bool,
    pub show_skills: bool,
    pub search: String,
    pub folder_edit: String,
}
impl UiState {
    pub fn new(cwd: String) -> Self {
        Self {
            status: "Conectando ao Codex…".into(),
            connected: false,
            plan: None,
            models: vec![],
            threads: vec![],
            skills: vec![],
            current_thread: None,
            cwd: cwd.clone(),
            model: "gpt-6.2-sol".into(),
            effort: "high".into(),
            access: AccessMode::Folder,
            draft: String::new(),
            attachments: vec![],
            selected_skills: vec![],
            items: vec![],
            busy: false,
            preparing: false,
            usage: Default::default(),
            projects: vec![],
            project_filter: None,
            project_busy: false,
            sidebar_view: SidebarView::Conversations,
            loading_history: false,
            error: None,
            notices: vec![],
            prompt: None,
            more_threads: false,
            threads_loading: true,
            more_history: false,
            show_skills: false,
            search: String::new(),
            folder_edit: cwd,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarView {
    Conversations,
    Projects,
}

#[derive(Debug)]
pub enum UiAction {
    RefreshUsage,
    SelectProject(String),
    ClearProjectFilter,
    CreateProject { parent: String, name: String },
    AddProject(String),
    NewChat,
    SelectThread(String),
    Send,
    Interrupt,
    Refresh,
    MoreThreads,
    MoreHistory,
    Folder(String),
    Attach(Vec<String>),
    OpenWeb,
    OpenPath(String),
    OpenOriginal,
    Respond { id: Value, result: Value },
    Reject { id: Value, message: String },
}
