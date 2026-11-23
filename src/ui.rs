use crate::view_model::{AccessMode, DisplayItem, ServerPrompt, SidebarView, UiAction, UiState};
use crate::{terminal::Terminal, workbench_ui::WorkbenchUi, workspace::WorkspaceReview};
use eframe::egui::{self, Color32, RichText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};

const CANVAS: Color32 = Color32::from_rgb(29, 31, 32);
const SIDEBAR: Color32 = Color32::from_rgb(23, 25, 26);
const SURFACE: Color32 = Color32::from_rgb(38, 41, 42);
const TEXT: Color32 = Color32::from_rgb(234, 237, 235);
const MUTED: Color32 = Color32::from_rgb(166, 176, 171);
const ACCENT: Color32 = Color32::from_rgb(144, 204, 171);
const BORDER: Color32 = Color32::from_rgb(62, 67, 65);
const ERROR: Color32 = Color32::from_rgb(245, 160, 150);

enum DialogResult {
    Folder(Option<String>),
    Files(Vec<String>),
    ProjectParent(Option<String>),
    AddProject(Option<String>),
}

pub struct Ui {
    logo: Option<egui::TextureHandle>,
    markdown_cache: CommonMarkCache,
    skill_search: String,
    help_open: bool,
    dialog_tx: Sender<DialogResult>,
    dialog_rx: Receiver<DialogResult>,
    folder_dialog: bool,
    file_dialog: bool,
    project_open: bool,
    project_name: String,
    project_parent: String,
    project_parent_dialog: bool,
    add_project_dialog: bool,
}

impl Ui {
    pub fn new(ctx: &egui::Context) -> Self {
        let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = CANVAS;
        style.visuals.window_fill = SURFACE;
        style.visuals.extreme_bg_color = SIDEBAR;
        style.visuals.faint_bg_color = SURFACE;
        style.visuals.override_text_color = Some(TEXT);
        style.visuals.hyperlink_color = ACCENT;
        style.visuals.selection.bg_fill = Color32::from_rgb(49, 81, 64);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, TEXT);
        style.visuals.widgets.inactive.bg_fill = SURFACE;
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(52, 59, 55);
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(52, 76, 62);
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT);
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 7.0);
        style.spacing.interact_size.y = 30.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(21.0));
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, egui::FontId::monospace(13.0));
        ctx.set_theme(egui::Theme::Dark);
        ctx.set_style_of(egui::Theme::Dark, style);
        let (dialog_tx, dialog_rx) = mpsc::channel();
        let logo = image::load_from_memory(include_bytes!("../assets/logo.png"))
            .ok()
            .map(|image| {
                let rgba = image.to_rgba8();
                let pixels = egui::ColorImage::from_rgba_unmultiplied(
                    [rgba.width() as usize, rgba.height() as usize],
                    rgba.as_raw(),
                );
                ctx.load_texture("rustgpeitinhos-logo", pixels, egui::TextureOptions::LINEAR)
            });
        Self {
            logo,
            markdown_cache: CommonMarkCache::default(),
            skill_search: String::new(),
            help_open: false,
            dialog_tx,
            dialog_rx,
            folder_dialog: false,
            file_dialog: false,
            project_open: false,
            project_name: String::new(),
            project_parent: String::new(),
            project_parent_dialog: false,
            add_project_dialog: false,
        }
    }

    /// eframe 0.37 fornece um Ui raiz; nenhum trabalho do motor ocorre aqui.
    pub fn render(
        &mut self,
        root: &mut egui::Ui,
        state: &mut UiState,
        workbench: &mut WorkbenchUi,
        terminal: &mut Terminal,
        review: &mut WorkspaceReview,
    ) -> Vec<UiAction> {
        let mut actions = Vec::new();
        if state.prompt.is_none()
            && root
                .ctx()
                .input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::J))
        {
            workbench.open = !workbench.open;
        }
        while let Ok(result) = self.dialog_rx.try_recv() {
            match result {
                DialogResult::Folder(path) => {
                    self.folder_dialog = false;
                    if let Some(path) = path {
                        actions.push(UiAction::Folder(path));
                    }
                }
                DialogResult::Files(paths) => {
                    self.file_dialog = false;
                    if !paths.is_empty() {
                        actions.push(UiAction::Attach(paths));
                    }
                }
                DialogResult::ProjectParent(path) => {
                    self.project_parent_dialog = false;
                    if let Some(path) = path {
                        self.project_parent = path;
                    }
                }
                DialogResult::AddProject(path) => {
                    self.add_project_dialog = false;
                    if let Some(path) = path {
                        actions.push(UiAction::AddProject(path));
                    }
                }
            }
        }
        egui::Panel::left("ponte-sidebar")
            .exact_size(240.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(14))
            .show(root, |ui| self.sidebar(ui, state, &mut actions));
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(CANVAS).inner_margin(0))
            .show(root, |ui| {
                egui::Panel::top("conversation-header")
                    .frame(
                        egui::Frame::new()
                            .fill(CANVAS)
                            .inner_margin(egui::Margin::symmetric(22, 14)),
                    )
                    .show(ui, |ui| self.header(ui, state, &mut actions));
                egui::Panel::bottom("conversation-composer")
                    .frame(
                        egui::Frame::new()
                            .fill(CANVAS)
                            .inner_margin(egui::Margin::symmetric(24, 16)),
                    )
                    .show(ui, |ui| self.composer(ui, state, &mut actions, workbench));
                workbench.render(
                    ui,
                    terminal,
                    review,
                    &state.cwd,
                    state.access,
                    state.connected,
                );
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(CANVAS)
                            .inner_margin(egui::Margin::symmetric(24, 12)),
                    )
                    .show(ui, |ui| self.history(ui, state, &mut actions));
            });
        let ctx = root.ctx();
        self.skills_window(ctx, state);
        self.help_window(ctx, &mut actions);
        self.project_window(ctx, state, &mut actions);
        self.server_prompt(ctx, state, &mut actions);
        actions
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, state: &mut UiState, actions: &mut Vec<UiAction>) {
        ui.horizontal(|ui| {
            if let Some(logo) = &self.logo {
                let size = logo.size();
                ui.image((
                    logo.id(),
                    egui::vec2(48.0, 48.0 * size[1] as f32 / size[0] as f32),
                ));
            }
            ui.label(RichText::new("RUST GPeitinhos").size(19.0).strong());
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let projects = matches!(state.sidebar_view, SidebarView::Projects);
            if ui.selectable_label(!projects, "Conversas").clicked() {
                state.sidebar_view = SidebarView::Conversations;
                state.search.clear();
            }
            if ui.selectable_label(projects, "Projetos").clicked() {
                state.sidebar_view = SidebarView::Projects;
                state.search.clear();
            }
        });
        let projects = matches!(state.sidebar_view, SidebarView::Projects);
        if projects {
            if ui
                .add_enabled(
                    !state.project_busy,
                    egui::Button::new("+ Novo projeto")
                        .min_size(egui::vec2(ui.available_width(), 32.0)),
                )
                .clicked()
            {
                if self.project_parent.is_empty() {
                    self.project_parent = state.cwd.clone();
                }
                self.project_open = true;
            }
            if ui
                .add_enabled(
                    !self.add_project_dialog && !state.project_busy,
                    egui::Button::new("Adicionar pasta existente")
                        .min_size(egui::vec2(ui.available_width(), 30.0)),
                )
                .clicked()
            {
                self.pick_project_folder(ui.ctx(), &state.cwd, true);
            }
        } else {
            if ui
                .add_enabled(
                    !state.preparing && !state.loading_history,
                    egui::Button::new("Nova conversa")
                        .min_size(egui::vec2(ui.available_width(), 34.0)),
                )
                .clicked()
            {
                actions.push(UiAction::NewChat);
            }
            if let Some(path) = &state.project_filter {
                ui.horizontal(|ui| {
                    let name = state
                        .projects
                        .iter()
                        .find(|project| &project.path == path)
                        .map(|project| project.name.as_str())
                        .unwrap_or_else(|| basename(path));
                    ui.add_sized(
                        [(ui.available_width() - 57.0).max(70.0), 25.0],
                        egui::Label::new(RichText::new(name).color(ACCENT).size(12.0)).truncate(),
                    )
                    .on_hover_text(path);
                    if ui.small_button("Todos").clicked() {
                        actions.push(UiAction::ClearProjectFilter);
                    }
                });
            }
        }
        ui.add(
            egui::TextEdit::singleline(&mut state.search)
                .hint_text(if projects {
                    "Buscar projetos"
                } else {
                    "Buscar conversas"
                })
                .desired_width(f32::INFINITY),
        );
        ui.add_space(6.0);

        egui::Panel::bottom("sidebar-account")
            .frame(
                egui::Frame::new()
                    .fill(SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(0, 8)),
            )
            .show(ui, |ui| {
                ui.separator();
                let plan = state
                    .plan
                    .as_deref()
                    .map(plan_label)
                    .unwrap_or("Conta ChatGPT");
                ui.label(RichText::new(plan).strong().size(13.0));
                let status_color = if state.connected { ACCENT } else { MUTED };
                ui.add(
                    egui::Label::new(RichText::new(&state.status).color(status_color).size(12.0))
                        .truncate(),
                )
                .on_hover_text(&state.status);
                if state.usage.render(ui, &state.model) {
                    actions.push(UiAction::RefreshUsage);
                }
                ui.horizontal(|ui| {
                    if ui.button("ChatGPT web").clicked() {
                        actions.push(UiAction::OpenWeb);
                    }
                    if ui.small_button("Ajuda").clicked() {
                        self.help_open = true;
                    }
                });
                if ui
                    .add_enabled(
                        !state.preparing,
                        egui::Button::new("Recarregar conexão").small(),
                    )
                    .clicked()
                {
                    actions.push(UiAction::Refresh);
                }
            });

        let query = state.search.trim().to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt(if projects {
                "project-list"
            } else {
                "thread-list"
            })
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if projects {
                    let mut projects: Vec<_> = state
                        .projects
                        .iter()
                        .filter(|project| {
                            query.is_empty()
                                || project.name.to_lowercase().contains(&query)
                                || project.path.to_lowercase().contains(&query)
                        })
                        .collect();
                    projects.sort_by_key(|project| project.name.to_lowercase());
                    if projects.is_empty() {
                        ui.label(
                            RichText::new(if state.projects.is_empty() {
                                "Crie um projeto ou adicione uma pasta existente."
                            } else {
                                "Nenhum projeto encontrado."
                            })
                            .color(MUTED)
                            .size(13.0),
                        );
                    }
                    for project in projects {
                        ui.push_id(&project.path, |ui| {
                            if ui
                                .add_enabled(
                                    !state.preparing
                                        && !state.loading_history
                                        && !state.project_busy,
                                    egui::Button::new(RichText::new(&project.name).strong())
                                        .selected(
                                            state.project_filter.as_deref()
                                                == Some(project.path.as_str()),
                                        )
                                        .min_size(egui::vec2(ui.available_width(), 31.0))
                                        .truncate(),
                                )
                                .on_hover_text(&project.path)
                                .clicked()
                            {
                                actions.push(UiAction::SelectProject(project.path.clone()));
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&project.path).size(11.0).color(MUTED),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&project.path);
                            ui.add_space(4.0);
                        });
                    }
                } else {
                    let mut ids = HashSet::new();
                    let mut threads: Vec<_> = state
                        .threads
                        .iter()
                        .filter(|thread| {
                            ids.insert(thread.id.as_str())
                                && state
                                    .project_filter
                                    .as_deref()
                                    .is_none_or(|project| project_contains(project, &thread.cwd))
                                && (query.is_empty()
                                    || thread.title().to_lowercase().contains(&query)
                                    || thread.cwd.to_lowercase().contains(&query))
                        })
                        .collect();
                    threads.sort_by_key(|thread| std::cmp::Reverse(thread.updated_at));
                    if threads.is_empty() {
                        ui.label(
                            RichText::new(if state.threads_loading {
                                "Carregando suas conversas…"
                            } else if state.project_filter.is_some() {
                                "Nenhuma conversa neste projeto."
                            } else if state.threads.is_empty() {
                                "Suas conversas aparecerão aqui."
                            } else {
                                "Nenhuma conversa encontrada."
                            })
                            .color(MUTED)
                            .size(13.0),
                        );
                    }
                    ui.spacing_mut().item_spacing.y = 4.0;
                    for thread in threads {
                        if ui
                            .add_enabled(
                                !state.preparing && !state.loading_history,
                                egui::Button::new(thread.title())
                                    .selected(
                                        state.current_thread.as_deref() == Some(thread.id.as_str()),
                                    )
                                    .truncate()
                                    .min_size(egui::vec2(ui.available_width(), 30.0)),
                            )
                            .on_hover_text(format!("{}\n{}", thread.title(), thread.cwd))
                            .clicked()
                        {
                            actions.push(UiAction::SelectThread(thread.id.clone()));
                        }
                    }
                    if state.more_threads
                        && ui
                            .add_enabled(
                                !state.threads_loading,
                                egui::Button::new(if state.threads_loading {
                                    "Carregando conversas…"
                                } else {
                                    "Carregar mais conversas"
                                }),
                            )
                            .clicked()
                    {
                        actions.push(UiAction::MoreThreads);
                    }
                }
            });
    }

    fn header(&mut self, ui: &mut egui::Ui, state: &mut UiState, actions: &mut Vec<UiAction>) {
        if let Some(path) = &state.project_filter {
            let name = state
                .projects
                .iter()
                .find(|project| &project.path == path)
                .map(|project| project.name.as_str())
                .unwrap_or_else(|| basename(path));
            ui.add(
                egui::Label::new(
                    RichText::new(format!("Projeto: {name}"))
                        .color(ACCENT)
                        .size(12.0),
                )
                .truncate(),
            )
            .on_hover_text(path);
        }
        ui.add_enabled_ui(
            !state.busy && !state.preparing && !state.loading_history,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let model_label = state
                        .models
                        .iter()
                        .find(|model| model.model == state.model)
                        .map(|model| model.display_name.as_str())
                        .unwrap_or(&state.model)
                        .to_owned();
                    egui::ComboBox::from_id_salt("model")
                        .selected_text(model_label)
                        .width(175.0)
                        .show_ui(ui, |ui| {
                            for model in &state.models {
                                if ui
                                    .selectable_value(
                                        &mut state.model,
                                        model.model.clone(),
                                        &model.display_name,
                                    )
                                    .changed()
                                {
                                    if !model
                                        .supported_reasoning_efforts
                                        .iter()
                                        .any(|effort| effort.reasoning_effort == state.effort)
                                    {
                                        if let Some(effort) = model
                                            .supported_reasoning_efforts
                                            .iter()
                                            .find(|effort| effort.reasoning_effort == "high")
                                            .or_else(|| model.supported_reasoning_efforts.first())
                                        {
                                            state.effort = effort.reasoning_effort.clone();
                                        }
                                    }
                                }
                            }
                            if state.models.is_empty() {
                                ui.label("Carregando modelos da conta…");
                            }
                        });
                    egui::ComboBox::from_id_salt("effort")
                        .selected_text(effort_label(&state.effort))
                        .width(95.0)
                        .show_ui(ui, |ui| {
                            if let Some(model) =
                                state.models.iter().find(|model| model.model == state.model)
                            {
                                for effort in &model.supported_reasoning_efforts {
                                    ui.selectable_value(
                                        &mut state.effort,
                                        effort.reasoning_effort.clone(),
                                        effort_label(&effort.reasoning_effort),
                                    );
                                }
                            }
                        })
                        .response
                        .on_hover_text(
                            "Raciocínio: mais alto pode levar mais tempo e consumir mais uso.",
                        );
                    egui::ComboBox::from_id_salt("access")
                        .selected_text(state.access.label())
                        .width(163.0)
                        .show_ui(ui, |ui| {
                            for access in
                                [AccessMode::ReadOnly, AccessMode::Folder, AccessMode::Full]
                            {
                                ui.selectable_value(&mut state.access, access, access.label());
                            }
                        })
                        .response
                        .on_hover_text("Permissões aplicadas no próximo envio.");
                });
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Pasta").size(12.0).color(MUTED));
                    let width = (ui.available_width() - 205.0).max(80.0);
                    let edit = ui.add_sized(
                        [width, 28.0],
                        egui::TextEdit::singleline(&mut state.folder_edit)
                            .hint_text("Caminho da pasta"),
                    );
                    let enter =
                        edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    let changed = state.folder_edit.trim() != state.cwd;
                    if (ui
                        .add_enabled(
                            changed && !state.folder_edit.trim().is_empty(),
                            egui::Button::new("Aplicar"),
                        )
                        .clicked()
                        || (enter && changed))
                        && !state.folder_edit.trim().is_empty()
                    {
                        actions.push(UiAction::Folder(state.folder_edit.trim().to_owned()));
                    }
                    if ui
                        .add_enabled(!self.folder_dialog, egui::Button::new("Escolher…"))
                        .clicked()
                    {
                        self.pick_folder(ui.ctx(), &state.cwd);
                    }
                });
            },
        );
    }

    fn composer(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut UiState,
        actions: &mut Vec<UiAction>,
        workbench: &mut WorkbenchUi,
    ) {
        if let Some(error) = &state.error {
            egui::Frame::new()
                .fill(Color32::from_rgb(58, 36, 34))
                .inner_margin(10)
                .corner_radius(6)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("connection-error")
                        .max_height(100.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(error).color(ERROR))
                                    .wrap()
                                    .selectable(true),
                            );
                        });
                    if ui
                        .add_enabled(
                            !state.preparing,
                            egui::Button::new("Tentar reconectar").small(),
                        )
                        .clicked()
                    {
                        actions.push(UiAction::Refresh);
                    }
                });
        }
        if !state.notices.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new(format!("Avisos ({})", state.notices.len()))
                    .size(12.0)
                    .color(MUTED),
            )
            .id_salt("notices")
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("notice-list")
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for notice in &state.notices {
                            ui.add(egui::Label::new(notice).wrap().selectable(true));
                        }
                    });
            });
        }
        egui::Frame::new()
            .fill(SURFACE)
            .inner_margin(12)
            .corner_radius(10)
            .show(ui, |ui| {
                let mut remove_attachment = None;
                let mut remove_skill = None;
                if !state.attachments.is_empty() || !state.selected_skills.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (index, path) in state.attachments.iter().enumerate() {
                            if ui
                                .add_enabled(!state.preparing, egui::Button::new(format!("{} · remover", basename(path))).small())
                                .on_hover_text(path)
                                .clicked()
                            {
                                remove_attachment = Some(index);
                            }
                        }
                        for (index, skill) in state.selected_skills.iter().enumerate() {
                            if ui
                                .add_enabled(!state.preparing, egui::Button::new(
                                    RichText::new(format!("${} · remover", skill.name)).color(ACCENT)).small())
                                .on_hover_text(&skill.description)
                                .clicked()
                            {
                                remove_skill = Some(index);
                            }
                        }
                    });
                }
                if let Some(index) = remove_attachment {
                    state.attachments.remove(index);
                }
                if let Some(index) = remove_skill {
                    state.selected_skills.remove(index);
                }
                let draft_focused = egui::ScrollArea::vertical()
                    .id_salt("draft-scroll")
                    .max_height(165.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut state.draft)
                                .id(egui::Id::unique("rustgpeitinhos-message-draft"))
                                .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT,egui::Key::Enter))
                                .hint_text("O que vamos fazer nesta pasta?")
                                .desired_rows(3)
                                .desired_width(f32::INFINITY)
                                .frame(egui::Frame::NONE),
                        )
                        .has_focus()
                    })
                    .inner;
                let can_send = can_send(state);
                let mut send = false;
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.file_dialog && !state.preparing,
                            egui::Button::new("Anexar").small(),
                        )
                        .clicked()
                    {
                        self.pick_files(ui.ctx(), &state.cwd);
                    }
                    if ui
                        .add_enabled(!state.preparing, egui::Button::new(format!(
                            "Skills ({})",
                            state.skills.iter().filter(|skill| skill.enabled).count()
                        )))
                        .clicked()
                    {
                        state.show_skills = !state.show_skills;
                    }
                    if ui
                        .selectable_label(workbench.open, "Terminal · Alterações")
                        .on_hover_text("Mostrar ou ocultar painel · Ctrl+J")
                        .clicked()
                    {
                        workbench.open = !workbench.open;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        send = ui.add_enabled(can_send, egui::Button::new(
                            RichText::new(if state.preparing { "Enviando…" } else if state.busy { "Enviar agora" } else { "Enviar" })
                                .color(Color32::from_rgb(20, 38, 28)))
                                .fill(ACCENT).min_size(egui::vec2(76.0, 32.0)))
                            .on_hover_text(if state.busy { "Complementar as instruções em execução. Se o trabalho terminar antes do envio, sua mensagem será preservada para enviar novamente." } else { "Enviar mensagem · Enter" })
                            .clicked();
                        if state.busy {
                            if ui.button("Interromper").clicked() { actions.push(UiAction::Interrupt); }
                            ui.spinner();
                        }
                    });
                });
                if draft_focused
                    && !self.help_open
                    && !self.project_open
                    && !state.show_skills
                    && state.prompt.is_none()
                    && can_send
                    && ui.ctx().input_mut(consume_send_enter)
                {
                    send = true;
                }
                if send {
                    actions.push(UiAction::Send);
                }
            });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Enter para enviar · Shift+Enter para pular linha")
                    .size(11.0)
                    .color(MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(if state.connected {
                        "Usando sua assinatura ChatGPT"
                    } else {
                        "Conexão pela assinatura ChatGPT"
                    })
                    .size(11.0)
                    .color(MUTED),
                );
            });
        });
    }

    fn history(&mut self, ui: &mut egui::Ui, state: &mut UiState, actions: &mut Vec<UiAction>) {
        egui::ScrollArea::vertical().id_salt(("history", &state.current_thread)).stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            if state.more_history && ui.add_enabled(!state.loading_history, egui::Button::new("Carregar mensagens anteriores")).clicked() { actions.push(UiAction::MoreHistory); }
            if state.loading_history {
                ui.horizontal(|ui| { ui.spinner(); ui.label(RichText::new("Carregando conversa…").color(MUTED)); });
            }
            let width = ui.available_width().min(940.0);
            let padding = ((ui.available_width() - width) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(padding);
                ui.vertical(|ui| {
                    ui.set_width(width);
                    if state.items.is_empty() && !state.loading_history {
                        ui.add_space(42.0);
                        ui.label(RichText::new("Uma conversa, sua pasta.").size(23.0).strong());
                        ui.add_space(4.0);
                        ui.label(RichText::new("Peça uma alteração, investigue um problema ou continue um projeto.").color(MUTED));
                        ui.add_space(18.0);
                        ui.label(RichText::new(basename(&state.cwd)).color(ACCENT));
                    }
                    for item in &state.items {
                        ui.push_id(&item.id, |ui| self.message(ui, item));
                        ui.add_space(18.0);
                    }
                    if state.busy { ui.horizontal(|ui| { ui.spinner(); ui.label(RichText::new("Codex trabalhando…").size(12.0).color(MUTED)); }); }
                });
            });
        });
    }

    fn message(&mut self, ui: &mut egui::Ui, item: &DisplayItem) {
        match item.kind.as_str() {
            "userMessage" | "user" => {
                egui::Frame::new()
                    .fill(SURFACE)
                    .inner_margin(14)
                    .corner_radius(8)
                    .show(ui, |ui| {
                        message_heading(ui, "Você", MUTED, &item.text);
                        CommonMarkViewer::new().show(ui, &mut self.markdown_cache, &item.text);
                    });
            }
            "agentMessage" | "assistant" | "agent" => {
                message_heading(ui, "Codex", ACCENT, &item.text);
                CommonMarkViewer::new().show(ui, &mut self.markdown_cache, &item.text);
            }
            _ => {
                let title = tool_label(&item.kind);
                let title = if item.running {
                    format!("{title} · em andamento")
                } else {
                    title.to_owned()
                };
                egui::CollapsingHeader::new(RichText::new(title).color(MUTED).size(13.0))
                    .id_salt("tool")
                    .show(ui, |ui| {
                        let text = if item.text.is_empty() {
                            serde_json::to_string_pretty(&item.raw).unwrap_or_default()
                        } else {
                            item.text.clone()
                        };
                        egui::ScrollArea::both()
                            .id_salt("tool-output")
                            .max_height(320.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(RichText::new(&text).monospace().size(12.0))
                                        .wrap_mode(egui::TextWrapMode::Extend)
                                        .selectable(true),
                                );
                            });
                        if ui.small_button("Copiar conteúdo").clicked() {
                            ui.ctx().copy_text(text);
                        }
                    });
            }
        }
    }

    fn project_window(
        &mut self,
        ctx: &egui::Context,
        state: &UiState,
        actions: &mut Vec<UiAction>,
    ) {
        if !self.project_open || state.prompt.is_some() {
            return;
        }
        let mut close = false;
        let response =
            egui::Modal::new(egui::Id::unique("rustgpeitinhos-new-project")).show(ctx, |ui| {
                ui.set_width(470.0_f32.min((ctx.content_rect().width() - 60.0).max(260.0)));
                ui.heading("Novo projeto");
                ui.label(
                    RichText::new("Crie uma pasta para organizar suas conversas e arquivos.")
                        .color(MUTED),
                );
                ui.add_space(8.0);
                ui.label("Nome do projeto");
                let name = ui.add_enabled(
                    !state.project_busy,
                    egui::TextEdit::singleline(&mut self.project_name)
                        .hint_text("Nome da nova pasta")
                        .desired_width(f32::INFINITY),
                );
                ui.label("Criar dentro de");
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        !state.project_busy,
                        egui::TextEdit::singleline(&mut self.project_parent)
                            .desired_width((ui.available_width() - 106.0).max(100.0)),
                    );
                    if ui
                        .add_enabled(
                            !self.project_parent_dialog && !state.project_busy,
                            egui::Button::new("Escolher…"),
                        )
                        .clicked()
                    {
                        let directory = self.project_parent.clone();
                        self.pick_project_folder(ctx, &directory, false);
                    }
                });
                let name_valid = crate::projects::validate_name(&self.project_name);
                if let Ok(name) = &name_valid {
                    let target = Path::new(self.project_parent.trim()).join(name);
                    ui.add(
                        egui::Label::new(
                            RichText::new(target.to_string_lossy())
                                .color(MUTED)
                                .size(12.0),
                        )
                        .truncate(),
                    )
                    .on_hover_text(target.to_string_lossy());
                } else if !self.project_name.is_empty() {
                    ui.label(
                        RichText::new(name_valid.as_ref().unwrap_err())
                            .color(ERROR)
                            .size(12.0),
                    );
                }
                ui.add_space(8.0);
                let ready = !state.project_busy
                    && !self.project_parent_dialog
                    && !self.project_parent.trim().is_empty()
                    && name_valid.is_ok();
                let enter =
                    name.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button("Cancelar").clicked() {
                        close = true;
                    }
                    if (ui
                        .add_enabled(ready, egui::Button::new("Criar projeto"))
                        .clicked()
                        || enter)
                        && ready
                    {
                        actions.push(UiAction::CreateProject {
                            parent: self.project_parent.trim().to_owned(),
                            name: name_valid.as_ref().unwrap().clone(),
                        });
                        close = true;
                    }
                    if state.project_busy {
                        ui.spinner();
                    }
                });
            });
        if close || response.should_close() {
            self.project_open = false;
        }
    }

    fn pick_project_folder(&mut self, ctx: &egui::Context, cwd: &str, add_existing: bool) {
        if add_existing {
            self.add_project_dialog = true;
        } else {
            self.project_parent_dialog = true;
        }
        let tx = self.dialog_tx.clone();
        let ctx = ctx.clone();
        let cwd = cwd.to_owned();
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title(if add_existing {
                    "Adicionar pasta aos projetos"
                } else {
                    "Escolher onde criar o projeto"
                })
                .set_directory(cwd)
                .pick_folder()
                .map(|path| path.to_string_lossy().into_owned());
            let _ = tx.send(if add_existing {
                DialogResult::AddProject(path)
            } else {
                DialogResult::ProjectParent(path)
            });
            ctx.request_repaint();
        });
    }

    fn pick_folder(&mut self, ctx: &egui::Context, cwd: &str) {
        self.folder_dialog = true;
        let tx = self.dialog_tx.clone();
        let ctx = ctx.clone();
        let cwd = cwd.to_owned();
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("Escolher pasta do projeto")
                .set_directory(cwd)
                .pick_folder()
                .map(|path| path.to_string_lossy().into_owned());
            let _ = tx.send(DialogResult::Folder(path));
            ctx.request_repaint();
        });
    }

    fn pick_files(&mut self, ctx: &egui::Context, cwd: &str) {
        self.file_dialog = true;
        let tx = self.dialog_tx.clone();
        let ctx = ctx.clone();
        let cwd = cwd.to_owned();
        std::thread::spawn(move || {
            let files = rfd::FileDialog::new()
                .set_title("Anexar arquivos")
                .set_directory(cwd)
                .pick_files()
                .unwrap_or_default()
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            let _ = tx.send(DialogResult::Files(files));
            ctx.request_repaint();
        });
    }

    fn skills_window(&mut self, ctx: &egui::Context, state: &mut UiState) {
        if !state.show_skills {
            return;
        }
        let mut open = true;
        egui::Window::new("Skills")
            .open(&mut open)
            .default_size([580.0, 450.0])
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(RichText::new("Escolha as instruções para o próximo envio.").color(MUTED));
                ui.add(
                    egui::TextEdit::singleline(&mut self.skill_search)
                        .hint_text("Buscar por nome ou descrição")
                        .desired_width(f32::INFINITY),
                );
                let query = self.skill_search.to_lowercase();
                let skills: Vec<_> = state
                    .skills
                    .iter()
                    .filter(|skill| {
                        skill.enabled
                            && (query.is_empty()
                                || skill.name.to_lowercase().contains(&query)
                                || skill.description.to_lowercase().contains(&query))
                    })
                    .collect();
                if skills.is_empty() {
                    ui.label("Nenhuma skill disponível para esta busca.");
                }
                egui::ScrollArea::vertical()
                    .id_salt("skills-list")
                    .max_height(450.0)
                    .show_rows(ui, 54.0, skills.len(), |ui, range| {
                        for index in range {
                            let skill = skills[index];
                            ui.push_id(&skill.path, |ui| {
                                let selected = state
                                    .selected_skills
                                    .iter()
                                    .any(|selected| selected.path == skill.path);
                                if ui
                                    .add_enabled(
                                        !state.preparing,
                                        egui::Button::new(RichText::new(&skill.name).strong())
                                            .selected(selected)
                                            .min_size(egui::vec2(ui.available_width(), 27.0))
                                            .truncate(),
                                    )
                                    .clicked()
                                {
                                    if selected {
                                        state
                                            .selected_skills
                                            .retain(|selected| selected.path != skill.path);
                                    } else {
                                        state.selected_skills.push(skill.clone());
                                    }
                                }
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&skill.description).size(12.0).color(MUTED),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&skill.description);
                            });
                        }
                    });
            });
        state.show_skills = open;
    }

    fn help_window(&mut self, ctx: &egui::Context, actions: &mut Vec<UiAction>) {
        egui::Window::new("Sobre o RUST GPeitinhos").open(&mut self.help_open).default_width(450.0).collapsible(false).show(ctx, |ui| {
            ui.label("Cliente independente em Rust para o Codex. Sem afiliação ou endosso da OpenAI.");
            ui.label("Os modelos usam sua assinatura ChatGPT e os limites da sua conta. Raciocínio alto pode consumir mais uso.");
            ui.add_space(8.0);
            ui.label("As conversas ficam organizadas pela pasta de trabalho. As permissões escolhidas se aplicam ao próximo envio.");
            ui.label("Configure suas integrações no ChatGPT web e recarregue a conexão. A disponibilidade local depende do suporte do motor e das permissões da conta.");
            ui.add_space(8.0);
            ui.label(RichText::new("O Browser embutido, voz e prévias interativas do aplicativo desktop não estão implementados neste cliente.").size(12.0).color(MUTED));
            if ui.button("Abrir ChatGPT web").clicked() { actions.push(UiAction::OpenWeb); }
        });
    }

    fn server_prompt(
        &mut self,
        ctx: &egui::Context,
        state: &mut UiState,
        actions: &mut Vec<UiAction>,
    ) {
        let Some(mut prompt) = state.prompt.take() else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::unique(("server-prompt", prompt.id.to_string())))
            .show(ctx, |ui| {
                ui.set_width(560.0_f32.min((ctx.content_rect().width() - 48.0).max(250.0)));
                egui::ScrollArea::vertical().id_salt("prompt-scroll").max_height((ctx.content_rect().height() - 100.0).max(150.0)).show(ui, |ui| {
                    if prompt.method == "item/commandExecution/requestApproval" || prompt.method == "item/fileChange/requestApproval" {
                        approval_prompt(ui, &mut prompt, &mut answer);
                    } else if prompt.method == "item/permissions/requestApproval" {
                        permissions_prompt(ui, &mut prompt, &mut answer);
                    } else if prompt.method == "item/tool/requestUserInput" || prompt.method == "tool/requestUserInput" {
                        questions_prompt(ui, &mut prompt, &mut answer);
                    } else if prompt.method == "mcpServer/elicitation/request" {
                        elicitation_prompt(ui, &mut prompt, &mut answer, actions);
                    } else {
                        ui.heading("Pedido do motor");
                        ui.label("Este tipo de interação ainda não possui um formulário nesta interface.");
                        ui.label(RichText::new(&prompt.method).monospace());
                        details(ui, &prompt.params);
                        if ui.button("Recusar pedido").clicked() { answer = Some(UiAction::Reject { id: prompt.id.clone(), message: "Interação não suportada pela interface; recusada pelo usuário.".into() }); }
                    }
                });
            });
        if let Some(answer) = answer {
            actions.push(answer);
        }
        // O controlador remove o pedido somente quando a resposta entra na fila do motor.
        // Em caso de desconexão ou fila cheia, a resposta pode ser tentada novamente.
        state.prompt = Some(prompt);
    }
}

fn message_heading(ui: &mut egui::Ui, label: &str, color: Color32, text: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(color).size(12.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .small_button("Copiar")
                .on_hover_text("Copiar mensagem")
                .clicked()
            {
                ui.ctx().copy_text(text.to_owned());
            }
        });
    });
}

fn consume_send_enter(input: &mut egui::InputState) -> bool {
    // consume_key aceita Shift extra; Shift+Enter deve apenas pular linha.
    let mut send = false;
    input.events.retain(|event| {
        let matches=matches!(event,egui::Event::Key{key:egui::Key::Enter,pressed:true,repeat:false,modifiers,..}
            if !modifiers.shift && !modifiers.alt && !modifiers.mac_cmd);
        send |= matches;
        !matches
    });
    send
}

fn can_send(state: &UiState) -> bool {
    state.connected
        && !state.preparing
        && !state.loading_history
        && state.prompt.is_none()
        && (!state.draft.trim().is_empty() || !state.attachments.is_empty())
        && state.attachments.iter().all(|path| !path.trim().is_empty())
}

fn project_contains(project: &str, cwd: &str) -> bool {
    fn normalize(path: &str) -> Option<std::path::PathBuf> {
        let path = Path::new(path);
        if !path.is_absolute() {
            return None;
        }
        let mut normalized = std::path::PathBuf::new();
        for component in path.components() {
            if component == std::path::Component::ParentDir {
                normalized.pop();
            } else {
                normalized.push(component.as_os_str());
            }
        }
        Some(normalized)
    }
    match (normalize(project), normalize(cwd)) {
        (Some(project), Some(cwd)) => cwd.starts_with(project),
        _ => false,
    }
}

fn basename(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

fn plan_label(plan: &str) -> &str {
    match plan {
        "plus" => "ChatGPT Plus",
        "pro" => "ChatGPT Pro",
        "business" => "ChatGPT Business",
        "enterprise" => "ChatGPT Enterprise",
        "edu" => "ChatGPT Edu",
        "free" => "ChatGPT Free",
        _ => plan,
    }
}

fn effort_label(effort: &str) -> &str {
    match effort {
        "none" => "Nenhum",
        "minimal" => "Mínimo",
        "low" => "Baixo",
        "medium" => "Médio",
        "high" => "Alto",
        "xhigh" => "Muito alto",
        "max" => "Máximo",
        _ => effort,
    }
}

fn tool_label(kind: &str) -> &str {
    match kind {
        "commandExecution" => "Comando",
        "fileChange" => "Alteração de arquivos",
        "mcpToolCall" => "Ferramenta MCP",
        "dynamicToolCall" => "Ferramenta",
        "webSearch" => "Busca na web",
        "reasoning" => "Raciocínio",
        "plan" => "Plano",
        "imageView" => "Imagem",
        "collabAgentToolCall" => "Subagente",
        _ => kind,
    }
}

fn respond(prompt: &ServerPrompt, result: Value) -> UiAction {
    UiAction::Respond {
        id: prompt.id.clone(),
        result,
    }
}

fn details(ui: &mut egui::Ui, value: &Value) {
    egui::CollapsingHeader::new("Detalhes do pedido")
        .id_salt("request-details")
        .show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(serde_json::to_string_pretty(value).unwrap_or_default())
                            .monospace()
                            .size(12.0),
                    )
                    .selectable(true),
                );
            });
        });
}

fn approval_prompt(ui: &mut egui::Ui, prompt: &mut ServerPrompt, answer: &mut Option<UiAction>) {
    let command = prompt.method.contains("commandExecution");
    ui.heading(if command {
        "Autorizar comando?"
    } else {
        "Autorizar alteração de arquivos?"
    });
    if let Some(reason) = prompt.params["reason"].as_str() {
        ui.label(reason);
    }
    if let Some(command) = prompt.params["command"].as_str() {
        egui::Frame::new()
            .fill(SIDEBAR)
            .inner_margin(10)
            .corner_radius(5)
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(RichText::new(command).monospace())
                        .wrap()
                        .selectable(true),
                );
            });
    }
    if let Some(cwd) = prompt.params["cwd"].as_str() {
        ui.label(RichText::new(cwd).size(12.0).color(MUTED));
    }
    if let Some(network) = prompt.params.get("networkApprovalContext") {
        ui.label("O pedido inclui acesso de rede:");
        details(ui, network);
    }
    details(ui, &prompt.params);
    let accept_allowed = prompt.params["availableDecisions"]
        .as_array()
        .is_none_or(|decisions| decisions.iter().any(|decision| decision == "accept"));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Recusar").clicked() {
            *answer = Some(respond(prompt, json!({"decision":"decline"})));
        }
        if ui
            .add_enabled(accept_allowed, egui::Button::new("Autorizar uma vez"))
            .clicked()
        {
            *answer = Some(respond(prompt, json!({"decision":"accept"})));
        }
    });
}

fn permissions_prompt(ui: &mut egui::Ui, prompt: &mut ServerPrompt, answer: &mut Option<UiAction>) {
    ui.heading("Autorizar permissões adicionais?");
    if let Some(reason) = prompt.params["reason"].as_str() {
        ui.label(reason);
    }
    ui.label("A autorização vale somente para este turno.");
    details(ui, &prompt.params["permissions"]);
    ui.horizontal(|ui| {
        if ui.button("Recusar").clicked() {
            *answer = Some(respond(prompt, json!({"permissions":{},"scope":"turn"})));
        }
        if ui.button("Autorizar neste turno").clicked() {
            *answer = Some(respond(
                prompt,
                json!({"permissions":prompt.params["permissions"],"scope":"turn"}),
            ));
        }
    });
}

fn questions_prompt(ui: &mut egui::Ui, prompt: &mut ServerPrompt, answer: &mut Option<UiAction>) {
    ui.heading("Codex precisa da sua resposta");
    let questions = prompt.params["questions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for question in &questions {
        let id = question["id"].as_str().unwrap_or_default();
        ui.push_id(id, |ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(question["question"].as_str().unwrap_or("Pergunta")).strong());
            let field = prompt.fields.entry(id.to_owned()).or_default();
            if let Some(options) = question["options"].as_array() {
                for option in options {
                    let label = option["label"].as_str().unwrap_or_default();
                    if ui
                        .selectable_label(field == label, label)
                        .on_hover_text(option["description"].as_str().unwrap_or_default())
                        .clicked()
                    {
                        *field = label.to_owned();
                    }
                }
            }
            let free_text = question["options"]
                .as_array()
                .is_none_or(|options| options.is_empty())
                || question["isOther"].as_bool().unwrap_or(false);
            if free_text {
                ui.add(
                    egui::TextEdit::singleline(field)
                        .hint_text("Sua resposta")
                        .password(question["isSecret"].as_bool().unwrap_or(false))
                        .desired_width(f32::INFINITY),
                );
            }
        });
    }
    let valid = !questions.is_empty()
        && questions.iter().all(|question| {
            question["id"]
                .as_str()
                .and_then(|id| prompt.fields.get(id))
                .is_some_and(|answer| !answer.trim().is_empty())
        });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if ui.button("Recusar pedido").clicked() {
            *answer = Some(UiAction::Reject {
                id: prompt.id.clone(),
                message: "Pedido de resposta recusado pelo usuário.".into(),
            });
        }
        if ui
            .add_enabled(valid, egui::Button::new("Enviar respostas"))
            .clicked()
        {
            let answers: Map<String, Value> = questions
                .iter()
                .filter_map(|question| {
                    let id = question["id"].as_str()?;
                    Some((
                        id.to_owned(),
                        json!({"answers":[prompt.fields.get(id).cloned().unwrap_or_default()]}),
                    ))
                })
                .collect();
            *answer = Some(respond(prompt, json!({"answers":answers})));
        }
    });
}

fn elicitation_prompt(
    ui: &mut egui::Ui,
    prompt: &mut ServerPrompt,
    answer: &mut Option<UiAction>,
    actions: &mut Vec<UiAction>,
) {
    ui.heading("Pedido de uma integração");
    if let Some(server) = prompt.params["serverName"].as_str() {
        ui.label(RichText::new(server).color(ACCENT));
    }
    ui.label(
        prompt.params["message"]
            .as_str()
            .unwrap_or("A integração precisa de uma confirmação."),
    );
    let mode = prompt.params["mode"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    if mode == "url" {
        let url = prompt.params["url"].as_str().unwrap_or_default().to_owned();
        ui.add(egui::Label::new(&url).wrap().selectable(true));
        let valid_url = url.starts_with("https://") || url.starts_with("http://");
        if ui
            .add_enabled(valid_url, egui::Button::new("Abrir página de confirmação"))
            .clicked()
        {
            actions.push(UiAction::OpenPath(url));
            prompt.fields.insert("_opened".into(), "true".into());
        }
        ui.horizontal(|ui| {
            if ui.button("Recusar").clicked() {
                *answer = Some(respond(prompt, json!({"action":"decline","content":null})));
            }
            if ui
                .add_enabled(
                    prompt
                        .fields
                        .get("_opened")
                        .is_some_and(|opened| opened == "true"),
                    egui::Button::new("Concluí na página"),
                )
                .clicked()
            {
                *answer = Some(respond(prompt, json!({"action":"accept","content":null})));
            }
        });
    } else if ["form", "openai/form", "openaiForm"].contains(&mode.as_str()) {
        let schema = prompt.params["requestedSchema"].clone();
        let properties = schema["properties"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        for (name, property) in &properties {
            ui.push_id(name, |ui| {
                form_field(ui, name, property, &schema, &mut prompt.fields)
            });
        }
        let content = form_content(&schema, &prompt.fields);
        if let Err(error) = &content {
            ui.label(RichText::new(error).color(MUTED).size(12.0));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Recusar").clicked() {
                *answer = Some(respond(prompt, json!({"action":"decline","content":null})));
            }
            if ui
                .add_enabled(content.is_ok(), egui::Button::new("Confirmar dados"))
                .clicked()
            {
                if let Ok(content) = content {
                    *answer = Some(respond(
                        prompt,
                        json!({"action":"accept","content":content}),
                    ));
                }
            }
        });
    } else {
        ui.label("Esta verificação precisa de um recurso ainda não disponível aqui.");
        details(ui, &prompt.params);
        if ui.button("Recusar verificação").clicked() {
            *answer = Some(respond(prompt, json!({"action":"decline","content":null})));
        }
    }
}

fn form_field(
    ui: &mut egui::Ui,
    name: &str,
    property: &Value,
    schema: &Value,
    fields: &mut BTreeMap<String, String>,
) {
    let label = property["title"].as_str().unwrap_or(name);
    let required = schema["required"]
        .as_array()
        .is_some_and(|required| required.iter().any(|value| value == name));
    ui.add_space(6.0);
    ui.label(
        RichText::new(if required {
            format!("{label} (obrigatório)")
        } else {
            label.to_owned()
        })
        .strong(),
    );
    if let Some(description) = property["description"].as_str() {
        ui.label(RichText::new(description).color(MUTED).size(12.0));
    }
    let default = match &property["default"] {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        value => value.to_string(),
    };
    let field = fields.entry(name.to_owned()).or_insert(default);
    let options = schema_options(property);
    match property["type"].as_str().unwrap_or("string") {
        "boolean" => {
            let mut checked = field == "true";
            ui.checkbox(&mut checked, "Sim");
            *field = checked.to_string();
        }
        "array" if !options.is_empty() => {
            let mut selected: Vec<String> = serde_json::from_str(field).unwrap_or_default();
            for (value, label) in options {
                let mut checked = selected.contains(&value);
                if ui.checkbox(&mut checked, label).changed() {
                    if checked {
                        selected.push(value);
                    } else {
                        selected.retain(|item| item != &value);
                    }
                }
            }
            *field = serde_json::to_string(&selected).unwrap_or_else(|_| "[]".into());
        }
        "string" if !options.is_empty() => {
            egui::ComboBox::from_id_salt("form-option")
                .selected_text(
                    options
                        .iter()
                        .find(|(value, _)| value == field)
                        .map(|(_, label)| label.as_str())
                        .unwrap_or("Escolher…"),
                )
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (value, label) in options {
                        ui.selectable_value(field, value, label);
                    }
                });
        }
        "string" | "number" | "integer" => {
            ui.add(egui::TextEdit::singleline(field).desired_width(f32::INFINITY));
        }
        _ => {
            ui.label(RichText::new("Campo não suportado nesta interface.").color(ERROR));
        }
    }
}

fn schema_options(property: &Value) -> Vec<(String, String)> {
    let source = if property["type"] == "array" {
        &property["items"]
    } else {
        property
    };
    if let Some(values) = source["enum"].as_array() {
        return values
            .iter()
            .enumerate()
            .filter_map(|(index, value)| {
                let value = value.as_str()?.to_owned();
                let label = source["enumNames"]
                    .get(index)
                    .and_then(Value::as_str)
                    .unwrap_or(&value)
                    .to_owned();
                Some((value, label))
            })
            .collect();
    }
    source["oneOf"]
        .as_array()
        .or_else(|| source["anyOf"].as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|option| {
                    let value = option["const"].as_str()?.to_owned();
                    let label = option["title"].as_str().unwrap_or(&value).to_owned();
                    Some((value, label))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn form_content(schema: &Value, fields: &BTreeMap<String, String>) -> Result<Value, String> {
    if schema["type"] != "object" {
        return Err("O formulário usa um esquema não suportado.".into());
    }
    let properties = schema["properties"]
        .as_object()
        .ok_or_else(|| "O formulário usa um esquema não suportado.".to_owned())?;
    let mut content = Map::new();
    for (name, property) in properties {
        let text = fields.get(name).map(String::as_str).unwrap_or_default();
        let label = property["title"].as_str().unwrap_or(name);
        let required = schema["required"]
            .as_array()
            .is_some_and(|required| required.iter().any(|value| value == name));
        if text.is_empty() {
            if required {
                return Err(format!("Preencha {label}."));
            }
            continue;
        }
        let invalid = || format!("Verifique o valor de {label}.");
        let options = schema_options(property);
        let value = match property["type"].as_str().unwrap_or("string") {
            "boolean" => Value::Bool(text.parse::<bool>().map_err(|_| invalid())?),
            "integer" => Value::from(text.parse::<i64>().map_err(|_| invalid())?),
            "number" => {
                let number = text.parse::<f64>().map_err(|_| invalid())?;
                Value::Number(serde_json::Number::from_f64(number).ok_or_else(invalid)?)
            }
            "string" => {
                let count = text.chars().count() as u64;
                if property["minLength"]
                    .as_u64()
                    .is_some_and(|minimum| count < minimum)
                    || property["maxLength"]
                        .as_u64()
                        .is_some_and(|maximum| count > maximum)
                {
                    return Err(invalid());
                }
                if !options.is_empty() && !options.iter().any(|(value, _)| value == text) {
                    return Err(invalid());
                }
                if let Some(format) = property["format"].as_str() {
                    if !valid_format(text, format) {
                        return Err(invalid());
                    }
                }
                Value::String(text.to_owned())
            }
            "array" if !options.is_empty() => {
                let values: Vec<String> = serde_json::from_str(text).map_err(|_| invalid())?;
                if values
                    .iter()
                    .any(|value| !options.iter().any(|(option, _)| option == value))
                    || property["minItems"]
                        .as_u64()
                        .is_some_and(|minimum| (values.len() as u64) < minimum)
                    || property["maxItems"]
                        .as_u64()
                        .is_some_and(|maximum| values.len() as u64 > maximum)
                {
                    return Err(invalid());
                }
                json!(values)
            }
            _ => return Err(format!("O campo {label} usa um tipo ainda não suportado.")),
        };
        if let Some(number) = value.as_f64() {
            if property["minimum"]
                .as_f64()
                .is_some_and(|minimum| number < minimum)
                || property["maximum"]
                    .as_f64()
                    .is_some_and(|maximum| number > maximum)
            {
                return Err(invalid());
            }
        }
        content.insert(name.clone(), value);
    }
    Ok(Value::Object(content))
}

fn valid_format(value: &str, format: &str) -> bool {
    match format {
        "email" => {
            value.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty() && !domain.is_empty() && !domain.contains('@')
            }) && !value.chars().any(char::is_whitespace)
        }
        "uri" => {
            value.split_once(':').is_some_and(|(scheme, rest)| {
                !scheme.is_empty()
                    && !rest.is_empty()
                    && scheme
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic())
                    && scheme
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || ['+', '-', '.'].contains(&c))
            }) && !value.chars().any(char::is_whitespace)
        }
        "date" => valid_date(value),
        "date-time" => valid_datetime(value),
        _ => false,
    }
}

fn valid_datetime(value: &str) -> bool {
    let Some((date, time)) = value.split_once(['T', 't']) else {
        return false;
    };
    if !valid_date(date)
        || !time.is_ascii()
        || time.len() < 9
        || time.as_bytes()[2] != b':'
        || time.as_bytes()[5] != b':'
    {
        return false;
    }
    let (Ok(hour), Ok(minute), Ok(second)) = (
        time[..2].parse::<u32>(),
        time[3..5].parse::<u32>(),
        time[6..8].parse::<u32>(),
    ) else {
        return false;
    };
    if hour > 23 || minute > 59 || second > 60 {
        return false;
    }
    let remainder = &time[8..];
    let zone = if let Some(fraction) = remainder.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return false;
        }
        &fraction[digits..]
    } else {
        remainder
    };
    if zone == "Z" || zone == "z" {
        return true;
    }
    if zone.len() != 6 || !matches!(zone.as_bytes()[0], b'+' | b'-') || zone.as_bytes()[3] != b':' {
        return false;
    }
    let (Ok(hour), Ok(minute)) = (zone[1..3].parse::<u32>(), zone[4..6].parse::<u32>()) else {
        return false;
    };
    hour <= 23 && minute <= 59
}

fn valid_date(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    if parts.len() != 3
        || parts[0].len() != 4
        || parts[1].len() != 2
        || parts[2].len() != 2
        || parts
            .iter()
            .any(|part| !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        parts[0].parse::<u32>(),
        parts[1].parse::<usize>(),
        parts[2].parse::<u32>(),
    ) else {
        return false;
    };
    if month == 0 || month > 12 {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    day > 0 && day <= days[month - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_sends_while_shift_enter_only_adds_a_line_during_active_work() {
        let ctx = egui::Context::default();
        let mut interface = Ui::new(&ctx);
        let mut state = UiState::new("/project".into());
        state.connected = true;
        state.busy = true;
        state.draft = "mensagem".into();
        let mut workbench = WorkbenchUi::new();
        let mut terminal = Terminal::default();
        let mut review = WorkspaceReview::new();
        let id = egui::Id::unique("rustgpeitinhos-message-draft");
        let screen = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1280.0, 820.0),
        ));
        ctx.run_ui(
            egui::RawInput {
                screen_rect: screen,
                ..Default::default()
            },
            |root| {
                interface.render(root, &mut state, &mut workbench, &mut terminal, &mut review);
            },
        )
        .drop_without_applying_deltas();
        ctx.memory_mut(|m| m.request_focus(id));
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
            let mut actions = vec![];
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: screen,
                    events: vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: Some(egui::Key::Enter),
                        pressed: true,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |root| {
                    actions = interface.render(
                        root,
                        &mut state,
                        &mut workbench,
                        &mut terminal,
                        &mut review,
                    );
                },
            )
            .drop_without_applying_deltas();
            if modifiers.shift {
                assert!(!actions.iter().any(|a| matches!(a, UiAction::Send)));
                assert_eq!(state.draft, "mensagem\n");
            } else {
                assert_eq!(
                    actions
                        .iter()
                        .filter(|a| matches!(a, UiAction::Send))
                        .count(),
                    1
                );
                assert_eq!(state.draft, "mensagem");
            }
            // O egui recalcula `repeat` usando keys_down. Soltar a tecla
            // garante que o próximo caso exercite Shift, não a proteção de
            // autorepetição de consume_send_enter.
            let draft_before_release = state.draft.clone();
            let mut release_actions = vec![];
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: screen,
                    events: vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: Some(egui::Key::Enter),
                        pressed: false,
                        repeat: false,
                        modifiers,
                    }],
                    ..Default::default()
                },
                |root| {
                    release_actions = interface.render(
                        root,
                        &mut state,
                        &mut workbench,
                        &mut terminal,
                        &mut review,
                    );
                },
            )
            .drop_without_applying_deltas();
            assert!(!release_actions.iter().any(|a| matches!(a, UiAction::Send)));
            assert_eq!(state.draft, draft_before_release);
        }
    }

    #[test]
    fn mcp_form_preserves_types_and_enforces_required_constraints() {
        let schema = json!({"type":"object","required":["count","color"],"properties":{"count":{"type":"integer","minimum":1,"maximum":5},"color":{"type":"string","enum":["red","blue"]},"enabled":{"type":"boolean"}}});
        let mut fields = BTreeMap::from([
            ("count".into(), "2".into()),
            ("color".into(), "blue".into()),
            ("enabled".into(), "false".into()),
        ]);
        assert_eq!(
            form_content(&schema, &fields).unwrap(),
            json!({"count":2,"color":"blue","enabled":false})
        );
        fields.insert("count".into(), "7".into());
        assert!(form_content(&schema, &fields).is_err());
        fields.insert("count".into(), String::new());
        assert!(form_content(&schema, &fields).is_err());
    }

    #[test]
    fn mcp_multiselect_uses_option_values_and_rejects_unknown_values() {
        let property = json!({"type":"array","minItems":1,"items":{"anyOf":[{"const":"a","title":"Primeiro"},{"const":"b","title":"Segundo"}]}});
        assert_eq!(
            schema_options(&property),
            vec![
                ("a".into(), "Primeiro".into()),
                ("b".into(), "Segundo".into())
            ]
        );
        let schema = json!({"type":"object","properties":{"choice":property}});
        let fields = BTreeMap::from([("choice".into(), "[\"missing\"]".into())]);
        assert!(form_content(&schema, &fields).is_err());
    }

    #[test]
    fn date_fields_reject_invalid_calendar_and_clock_values() {
        assert!(valid_format("2024-02-29", "date"));
        assert!(!valid_format("2026-02-29", "date"));
        assert!(!valid_format("2026-+1-01", "date"));
        assert!(valid_format("2026-11-22T13:40:30.123-03:00", "date-time"));
        assert!(!valid_format("2026-11-22T99:99:99Z", "date-time"));
        assert!(!valid_format("2026-11-22T13:40:30+99:00", "date-time"));
    }

    #[test]
    fn official_user_question_request_renders_and_preserves_its_answer_fields() {
        for method in ["item/tool/requestUserInput", "tool/requestUserInput"] {
            let ctx = egui::Context::default();
            let mut ui = Ui::new(&ctx);
            let mut state = UiState::new("/tmp".into());
            state.prompt = Some(ServerPrompt {
                id: json!("question-1"),
                method: method.into(),
                params: json!({"questions":[{
                    "id":"priority", "question":"Qual tarefa primeiro?", "isOther":true,
                    "options":[{"label":"Arquivos", "description":"Revisar mudanças"}]
                }]}),
                fields: BTreeMap::new(),
            });
            let mut actions = Vec::new();
            ctx.run_ui(egui::RawInput::default(), |root| {
                ui.server_prompt(root.ctx(), &mut state, &mut actions);
            })
            .drop_without_applying_deltas();
            let prompt = state
                .prompt
                .as_mut()
                .expect("A pergunta permanece pendente");
            assert!(
                prompt.fields.contains_key("priority"),
                "O pedido deve abrir o formulário de perguntas"
            );
            prompt.fields.insert("priority".into(), "Arquivos".into());
            ctx.run_ui(egui::RawInput::default(), |root| {
                ui.server_prompt(root.ctx(), &mut state, &mut actions);
            })
            .drop_without_applying_deltas();
            assert_eq!(
                state.prompt.as_ref().unwrap().fields["priority"],
                "Arquivos"
            );
            assert!(
                actions.is_empty(),
                "Renderizar não envia respostas automaticamente"
            );
        }
    }

    #[test]
    fn project_filter_respects_directory_boundaries_and_parent_components() {
        assert!(project_contains("/tmp/projeto", "/tmp/projeto"));
        assert!(project_contains("/tmp/projeto/", "/tmp/projeto/src"));
        assert!(!project_contains("/tmp/projeto", "/tmp/projeto-vizinho"));
        assert!(!project_contains("/tmp/projeto", "/tmp/projeto/../outro"));
        assert!(!project_contains("", "/tmp/projeto"));
    }

    #[test]
    fn send_is_disabled_until_ready_and_while_an_authorization_is_pending() {
        let mut state = UiState::new("/tmp".into());
        state.draft = "Olá".into();
        assert!(!can_send(&state));
        state.connected = true;
        assert!(can_send(&state));
        state.busy = true;
        assert!(
            can_send(&state),
            "Pode complementar um trabalho em execução"
        );
        state.preparing = true;
        assert!(!can_send(&state), "Aguarda a aceitação do envio anterior");
        state.preparing = false;
        state.busy = false;
        state.loading_history = true;
        assert!(!can_send(&state));
        state.loading_history = false;
        state.prompt = Some(ServerPrompt {
            id: json!(1),
            method: "test".into(),
            params: json!({}),
            fields: BTreeMap::new(),
        });
        assert!(!can_send(&state));
    }
}
