//! Painel local de comandos e revisão de alterações, sem operações Git destrutivas.
use crate::{
    terminal::{Terminal, TerminalAction},
    view_model::AccessMode,
    workspace::WorkspaceReview,
};
use eframe::egui::{self, Color32, RichText};

const SURFACE: Color32 = Color32::from_rgb(23, 25, 26);
const MUTED: Color32 = Color32::from_rgb(166, 176, 171);
const ACCENT: Color32 = Color32::from_rgb(144, 204, 171);
const ERROR: Color32 = Color32::from_rgb(245, 160, 150);

#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum Tab {
    #[default]
    Terminal,
    Git,
}

pub struct WorkbenchUi {
    pub open: bool,
    pub actions: Vec<TerminalAction>,
    tab: Tab,
    was_open: bool,
    review_cwd: Option<String>,
}

impl Default for WorkbenchUi {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkbenchUi {
    pub fn new() -> Self {
        Self {
            open: false,
            actions: Vec::new(),
            tab: Tab::Terminal,
            was_open: false,
            review_cwd: None,
        }
    }

    pub fn render(
        &mut self,
        root: &mut egui::Ui,
        terminal: &mut Terminal,
        review: &mut WorkspaceReview,
        cwd: &str,
        access: AccessMode,
        connected: bool,
    ) {
        let just_opened = self.open && !self.was_open;
        self.was_open = self.open;
        if !self.open {
            return;
        }
        let max_height = (root.available_height() - 100.0).max(130.0);
        egui::Panel::bottom("rustgpeitinhos-workbench")
            .default_size(260.0)
            .min_size(130.0)
            .max_size(max_height)
            .resizable(true)
            .frame(egui::Frame::new().fill(SURFACE).inner_margin(12))
            .show(root, |ui| {
                let previous_tab = self.tab;
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Terminal, "Terminal");
                    ui.selectable_value(&mut self.tab, Tab::Git, "Alterações");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Fechar").on_hover_text("Ctrl+J").clicked() {
                            self.open = false;
                        }
                    });
                });
                ui.separator();
                if self.tab == Tab::Git
                    && (just_opened
                        || previous_tab != self.tab
                        || self.review_cwd.as_deref() != Some(cwd))
                {
                    self.review_cwd = Some(cwd.to_owned());
                    review.refresh(cwd);
                }
                match self.tab {
                    Tab::Terminal => self.terminal(ui, terminal, cwd, access, connected),
                    Tab::Git => self.review(ui, review, cwd),
                }
            });
        self.was_open = self.open;
    }

    fn terminal(
        &mut self,
        ui: &mut egui::Ui,
        terminal: &mut Terminal,
        cwd: &str,
        access: AccessMode,
        connected: bool,
    ) {
        ui.horizontal(|ui| {
            let command_cwd = if terminal.running { &terminal.cwd } else { cwd };
            let command_access = if terminal.running {
                terminal.access
            } else {
                access
            };
            let directory_label = format!(
                "{} {command_cwd}",
                if terminal.running {
                    "Executando:"
                } else {
                    "Próximo comando:"
                }
            );
            ui.add_sized(
                [
                    (ui.available_width() - 330.0).max(80.0),
                    ui.spacing().interact_size.y,
                ],
                egui::Label::new(RichText::new(directory_label).size(12.0).color(MUTED)).truncate(),
            )
            .on_hover_text(command_cwd);
            ui.label(
                RichText::new(command_access.label())
                    .size(11.0)
                    .color(ACCENT),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Limpar").clicked() {
                    self.actions.push(TerminalAction::Clear);
                }
                if ui.small_button("Copiar saída").clicked() {
                    ui.ctx().copy_text(terminal.output.clone());
                }
                if terminal.running {
                    ui.spinner();
                } else if let Some(code) = terminal.exit_code {
                    ui.label(
                        RichText::new(format!("Saída {code}"))
                            .size(11.0)
                            .color(if code == 0 { MUTED } else { ERROR }),
                    );
                }
            });
        });
        if !terminal.running
            && !terminal.cwd.is_empty()
            && (terminal.cwd != cwd || terminal.access != access)
        {
            let previous = format!(
                "Saída anterior: {} · {}",
                terminal.cwd,
                terminal.access.label()
            );
            ui.add(egui::Label::new(RichText::new(&previous).size(11.0).color(MUTED)).truncate())
                .on_hover_text(previous);
        }
        if let Some(error) = &terminal.error {
            ui.add(egui::Label::new(RichText::new(error).size(12.0).color(ERROR)).truncate())
                .on_hover_text(error);
        }
        egui::Panel::bottom("rustgpeitinhos-command-input")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if terminal.running {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 205.0).max(100.0);
                        let response = ui.add_enabled(
                            connected,
                            egui::TextEdit::singleline(&mut terminal.input)
                                .desired_width(width)
                                .hint_text("Entrada para o comando em execução"),
                        );
                        let enter = response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        if (ui.add_enabled(connected, egui::Button::new("Enviar")).clicked() || enter)
                            && connected
                        {
                            self.actions.push(TerminalAction::Input);
                            response.request_focus();
                        }
                        if ui.add_enabled(connected, egui::Button::new("EOF")).on_hover_text("Encerrar entrada do comando").clicked() {
                            self.actions.push(TerminalAction::CloseInput);
                        }
                        if ui.button("Parar").clicked() {
                            self.actions.push(TerminalAction::Stop);
                        }
                    });
                } else {
                    ui.horizontal(|ui| {
                        let response = ui.add_enabled(
                            connected,
                            egui::TextEdit::singleline(&mut terminal.command)
                                .desired_width((ui.available_width() - 96.0).max(100.0))
                                .hint_text("Comando na pasta selecionada, por exemplo: cargo test")
                                .font(egui::TextStyle::Monospace),
                        ).on_hover_text(format!("Próximo comando em {cwd} · {}", access.label()));
                        let enter = response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter));
                        let ready = connected && !terminal.command.trim().is_empty();
                        if (ui.add_enabled(ready, egui::Button::new("Executar")).clicked() || enter)
                            && ready
                        {
                            self.actions.push(TerminalAction::Run);
                        }
                    });
                }
                ui.label(RichText::new("Comandos com entrada por linhas; programas de tela cheia não são suportados.")
                    .size(11.0).color(MUTED));
            });
        egui::ScrollArea::both()
            .id_salt("rustgpeitinhos-terminal-output")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if terminal.output.is_empty() {
                    ui.label(
                        RichText::new(if terminal.running {
                            "Aguardando saída…"
                        } else {
                            "A saída dos seus comandos aparecerá aqui."
                        })
                        .color(MUTED)
                        .size(12.0),
                    );
                } else {
                    ui.add(
                        egui::Label::new(RichText::new(&terminal.output).monospace().size(12.0))
                            .wrap_mode(egui::TextWrapMode::Extend)
                            .selectable(true),
                    );
                }
            });
    }

    fn review(&mut self, ui: &mut egui::Ui, review: &mut WorkspaceReview, cwd: &str) {
        ui.horizontal(|ui| {
            let directory = review.repo_root.as_deref().unwrap_or(cwd);
            ui.add_sized(
                [
                    (ui.available_width() - 230.0).max(90.0),
                    ui.spacing().interact_size.y,
                ],
                egui::Label::new(RichText::new(directory).color(MUTED).size(12.0)).truncate(),
            )
            .on_hover_text(directory);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!review.loading, egui::Button::new("Atualizar").small())
                    .clicked()
                {
                    self.review_cwd = Some(cwd.to_owned());
                    review.refresh(cwd);
                }
                if review.loading || review.diff_loading {
                    ui.spinner();
                }
                if ui
                    .add_enabled(
                        !review.diff.is_empty(),
                        egui::Button::new("Copiar diff").small(),
                    )
                    .clicked()
                {
                    ui.ctx().copy_text(review.diff.clone());
                }
            });
        });
        if let Some(error) = &review.error {
            ui.add(egui::Label::new(RichText::new(error).size(12.0).color(ERROR)).truncate())
                .on_hover_text(error);
        }
        if review.truncated {
            ui.label(
                RichText::new("O diff exibido foi limitado para manter a interface rápida.")
                    .color(MUTED)
                    .size(11.0),
            );
        }
        let mut selected = None;
        egui::Panel::left("rustgpeitinhos-changed-files")
            .default_size(200.0)
            .min_size(130.0)
            .max_size(340.0)
            .resizable(true)
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                egui::ScrollArea::both()
                    .id_salt("rustgpeitinhos-file-list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if review.files.is_empty() {
                            ui.label(
                                RichText::new(if review.loading {
                                    "Lendo alterações…"
                                } else {
                                    "Nenhum arquivo alterado."
                                })
                                .color(MUTED)
                                .size(12.0),
                            );
                        }
                        for file in &review.files {
                            if ui
                                .add(
                                    egui::Button::new(format!("{}  {}", file.status, file.path))
                                        .selected(
                                            review.selected.as_deref() == Some(file.path.as_str()),
                                        )
                                        .truncate()
                                        .min_size(egui::vec2(ui.available_width(), 27.0)),
                                )
                                .on_hover_text(&file.path)
                                .clicked()
                            {
                                selected = Some(file.path.clone());
                            }
                        }
                    });
            });
        if let Some(path) = selected {
            review.select(&path);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if review.diff.is_empty() {
                    ui.label(
                        RichText::new(if review.diff_loading {
                            "Carregando diff…"
                        } else if review.selected.is_some() {
                            "Sem diff de texto para este arquivo."
                        } else {
                            "Selecione um arquivo para conferir suas alterações."
                        })
                        .color(MUTED)
                        .size(12.0),
                    );
                } else {
                    let lines: Vec<_> = review.diff.lines().collect();
                    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                    egui::ScrollArea::both()
                        .id_salt(("rustgpeitinhos-file-diff", &review.selected))
                        .auto_shrink([false, false])
                        .show_rows(ui, row_height, lines.len(), |ui, range| {
                            for index in range {
                                let line = lines[index];
                                let color = if line.starts_with('+') && !line.starts_with("+++") {
                                    ACCENT
                                } else if line.starts_with('-') && !line.starts_with("---") {
                                    ERROR
                                } else if line.starts_with("@@") || line.starts_with("diff ") {
                                    Color32::from_rgb(149, 188, 211)
                                } else {
                                    MUTED
                                };
                                ui.add(
                                    egui::Label::new(RichText::new(line).monospace().color(color))
                                        .wrap_mode(egui::TextWrapMode::Extend)
                                        .selectable(true),
                                );
                            }
                        });
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ui::Ui, view_model::UiState, workspace::ChangedFile};

    struct PaintedText {
        text: String,
        rect: egui::Rect,
        clip: egui::Rect,
    }

    fn text_shapes(shape: &egui::Shape, clip: egui::Rect, output: &mut Vec<PaintedText>) {
        match shape {
            egui::Shape::Text(text) => output.push(PaintedText {
                text: text.galley.text().to_owned(),
                rect: text.visual_bounding_rect(),
                clip,
            }),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    text_shapes(shape, clip, output);
                }
            }
            _ => {}
        }
    }

    /// Exercises the full layout: sidebar, header, composer and workbench together.
    fn render_minimum_window(
        workbench: &mut WorkbenchUi,
        terminal: &mut Terminal,
        review: &mut WorkspaceReview,
        cwd: &str,
    ) -> Vec<PaintedText> {
        let mut state = UiState::new(cwd.into());
        state.connected = true;
        state.status = "Conectado".into();
        state.plan = Some("plus".into());
        render_state_minimum_window(workbench, terminal, review, &mut state)
    }

    fn render_state_minimum_window(
        workbench: &mut WorkbenchUi,
        terminal: &mut Terminal,
        review: &mut WorkspaceReview,
        state: &mut UiState,
    ) -> Vec<PaintedText> {
        let ctx = egui::Context::default();
        let mut ui = Ui::new(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 640.0));
        let mut painted = Vec::new();
        for _ in 0..3 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |root| {
                    let actions = ui.render(root, state, workbench, terminal, review);
                    assert!(
                        actions.is_empty(),
                        "Desenhar não deve iniciar ações do chat"
                    );
                },
            );
            painted.clear();
            for clipped in &output.shapes {
                text_shapes(&clipped.shape, clipped.clip_rect, &mut painted);
            }
            output.drop_without_applying_deltas();
            assert!(
                workbench.actions.is_empty(),
                "Desenhar não deve executar comandos ou enviar entrada"
            );
        }
        painted
    }

    fn assert_visible(texts: &[PaintedText], expected: &str) {
        let text = texts
            .iter()
            .find(|text| text.text == expected)
            .unwrap_or_else(|| panic!("Controle não desenhado: {expected}"));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 640.0));
        assert!(
            screen.expand(1.0).contains_rect(text.rect),
            "{expected} saiu da janela: {:?}",
            text.rect
        );
        assert!(
            text.clip.expand(1.0).contains_rect(text.rect),
            "{expected} foi cortado: {:?} / {:?}",
            text.rect,
            text.clip
        );
    }

    #[test]
    fn stopped_terminal_keeps_run_control_visible_at_minimum_window_size() {
        let mut workbench = WorkbenchUi::new();
        workbench.open = true;
        let mut terminal = Terminal::default();
        terminal.command = "printf exemplo".into();
        terminal.output = "resultado anterior\n".into();
        let mut review = WorkspaceReview::new();
        let cwd = format!("/tmp/{}", "pasta-com-nome-longo/".repeat(10));
        let texts = render_minimum_window(&mut workbench, &mut terminal, &mut review, &cwd);
        assert_visible(&texts, "Executar");
        assert_visible(&texts, "Copiar saída");
        assert_visible(&texts, "Enviar");
        assert_eq!(terminal.command, "printf exemplo");
        assert!(!terminal.running);
    }

    #[test]
    fn running_terminal_keeps_stdin_and_stop_visible_without_sending_input() {
        let mut workbench = WorkbenchUi::new();
        workbench.open = true;
        let mut terminal = Terminal::default();
        terminal.running = true;
        terminal.cwd = "/tmp/contexto-da-execucao".into();
        terminal.access = AccessMode::ReadOnly;
        terminal.input = "entrada de teste".into();
        terminal.output = "processo aguardando entrada\n".into();
        let mut review = WorkspaceReview::new();
        let texts = render_minimum_window(
            &mut workbench,
            &mut terminal,
            &mut review,
            "/tmp/nova-pasta",
        );
        assert_visible(&texts, "entrada de teste");
        assert_visible(&texts, "EOF");
        assert_visible(&texts, "Parar");
        assert_visible(&texts, "Somente leitura");
        assert_eq!(terminal.input, "entrada de teste");
        assert!(terminal.running);
    }

    #[test]
    fn git_review_displays_selected_file_and_diff_without_starting_actions() {
        let cwd = "/tmp/revisao-ui";
        let mut workbench = WorkbenchUi::new();
        workbench.open = true;
        workbench.was_open = true;
        workbench.tab = Tab::Git;
        workbench.review_cwd = Some(cwd.into());
        let mut terminal = Terminal::default();
        let mut review = WorkspaceReview::new();
        review.cwd = cwd.into();
        review.repo_root = Some(cwd.into());
        review.files = vec![ChangedFile {
            path: "src/main.rs".into(),
            status: " M".into(),
        }];
        review.selected = Some("src/main.rs".into());
        review.diff = "-antes\n+depois\n".into();
        let texts = render_minimum_window(&mut workbench, &mut terminal, &mut review, cwd);
        assert_visible(&texts, " M  src/main.rs");
        assert_visible(&texts, "-antes");
        assert_visible(&texts, "+depois");
        assert_visible(&texts, "Atualizar");
        assert_visible(&texts, "Copiar diff");
        assert_eq!(review.selected.as_deref(), Some("src/main.rs"));
        assert!(
            !review.loading && !review.diff_loading,
            "Renderizar dados existentes não deve iniciar processos Git"
        );
    }

    #[test]
    fn working_agent_shows_send_now_and_interrupt_together_at_minimum_size() {
        let mut workbench = WorkbenchUi::new();
        let mut terminal = Terminal::default();
        let mut review = WorkspaceReview::new();
        let mut state = UiState::new("/tmp/projeto".into());
        state.connected = true;
        state.busy = true;
        state.draft = "Complemento para o trabalho em execução".into();
        let texts =
            render_state_minimum_window(&mut workbench, &mut terminal, &mut review, &mut state);
        assert_visible(&texts, "Enviar agora");
        assert_visible(&texts, "Interromper");
        assert_visible(&texts, "Anexar");
        assert_eq!(state.draft, "Complemento para o trabalho em execução");
    }

    #[test]
    fn projects_tab_lists_projects_without_mixing_conversation_titles() {
        let mut workbench = WorkbenchUi::new();
        let mut terminal = Terminal::default();
        let mut review = WorkspaceReview::new();
        let mut state = UiState::new("/tmp/alfa".into());
        state.connected = true;
        state.sidebar_view = crate::view_model::SidebarView::Projects;
        state.projects = vec![crate::projects::Project {
            name: "Projeto Alfa".into(),
            path: "/tmp/alfa".into(),
        }];
        state.threads = vec![
            serde_json::from_value(serde_json::json!({
                "id":"hidden-thread", "name":"Conversa que deve ficar oculta", "cwd":"/tmp/alfa"
            }))
            .unwrap(),
        ];
        let texts =
            render_state_minimum_window(&mut workbench, &mut terminal, &mut review, &mut state);
        assert_visible(&texts, "Projeto Alfa");
        assert_visible(&texts, "+ Novo projeto");
        assert!(
            !texts
                .iter()
                .any(|text| text.text.contains("Conversa que deve ficar oculta"))
        );
    }

    #[test]
    fn conversations_project_filter_excludes_neighboring_directories() {
        let mut workbench = WorkbenchUi::new();
        let mut terminal = Terminal::default();
        let mut review = WorkspaceReview::new();
        let mut state = UiState::new("/tmp/alfa".into());
        state.connected = true;
        state.project_filter = Some("/tmp/alfa".into());
        state.projects = vec![crate::projects::Project {
            name: "Projeto Alfa".into(),
            path: "/tmp/alfa".into(),
        }];
        state.threads = serde_json::from_value(serde_json::json!([
            {"id":"inside", "name":"Dentro do projeto", "cwd":"/tmp/alfa/src", "updatedAt":20},
            {"id":"outside", "name":"Fora do projeto", "cwd":"/tmp/alfa-vizinho", "updatedAt":30}
        ]))
        .unwrap();
        let texts =
            render_state_minimum_window(&mut workbench, &mut terminal, &mut review, &mut state);
        assert_visible(&texts, "Dentro do projeto");
        assert_visible(&texts, "Todos");
        assert!(
            !texts
                .iter()
                .any(|text| text.text.contains("Fora do projeto"))
        );
    }
}
