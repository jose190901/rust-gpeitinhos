//! Exibição das cotas retornadas pelo Codex; nunca solicita ou consome resets.
use chrono::{Local, TimeZone};
use eframe::egui::{self, Color32, RichText};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const MUTED: Color32 = Color32::from_rgb(166, 176, 171);
const ACCENT: Color32 = Color32::from_rgb(144, 204, 171);
const ERROR: Color32 = Color32::from_rgb(245, 160, 150);

#[derive(Default)]
pub struct UsageState {
    buckets: BTreeMap<String, Value>,
    legacy: Option<Value>,
    reset_credits: Option<Value>,
    ordinary_usage_allowed: Option<bool>,
    updated_at: Option<i64>,
    error: Option<String>,
}

struct BucketView<'a> {
    key: &'a str,
    value: &'a Value,
}

struct SelectedBuckets<'a> {
    entries: Vec<BucketView<'a>>,
    fallback: bool,
}

impl UsageState {
    /// Leitura completa: campos ausentes ou nulos deixam de ser conhecidos.
    pub fn apply_snapshot(&mut self, value: &Value) {
        self.buckets.clear();
        if let Some(buckets) = value["rateLimitsByLimitId"].as_object() {
            for (key, bucket) in buckets {
                if bucket.is_object() {
                    self.buckets.insert(key.clone(), bucket.clone());
                }
            }
        }
        self.legacy = value.get("rateLimits").filter(|v| v.is_object()).cloned();
        self.reset_credits = value
            .get("rateLimitResetCredits")
            .filter(|v| v.is_object())
            .cloned();
        self.ordinary_usage_allowed = value["ordinaryUsageAllowed"].as_bool();
        self.updated_at = Some(Local::now().timestamp());
        self.error = None;
    }

    /// Notificações são esparsas: `null` não apaga metadados observados.
    /// Retorna `true` quando os buckets existentes exigem nova leitura, pois a
    /// atualização não pôde ser associada com segurança a uma cota específica.
    pub fn apply_update(&mut self, params: &Value) -> bool {
        let mut needs_snapshot = false;
        if let Some(update) = params.get("rateLimits").filter(|v| v.is_object()) {
            let incoming_id = string(update, "limitId");
            let incoming_model = string(update, "normalModelSlug");
            let legacy_compatible = self
                .legacy
                .as_ref()
                .is_some_and(|legacy| compatible_identity(legacy, incoming_id, incoming_model));
            let target_id = incoming_id.or_else(|| {
                self.legacy
                    .as_ref()
                    .filter(|_| legacy_compatible)
                    .and_then(|v| string(v, "limitId"))
            });
            let target = target_id
                .map(|id| {
                    self.buckets
                        .iter()
                        .find(|(key, bucket)| {
                            key.as_str() == id || string(bucket, "limitId") == Some(id)
                        })
                        .map(|(key, _)| key.clone())
                        .unwrap_or_else(|| id.to_owned())
                })
                .or_else(|| {
                    let model = incoming_model?;
                    let matches: Vec<_> = self
                        .buckets
                        .iter()
                        .filter(|(_, bucket)| string(bucket, "normalModelSlug") == Some(model))
                        .collect();
                    (matches.len() == 1).then(|| matches[0].0.clone())
                });

            needs_snapshot = target.is_none() && !self.buckets.is_empty();
            if let Some(key) = target {
                merge_available(
                    self.buckets
                        .entry(key)
                        .or_insert_with(|| Value::Object(Map::new())),
                    update,
                );
            }
            // Sem identidade suficiente, nem o bucket legado deve receber a
            // porcentagem: ele pode representar outro modelo. O chamador
            // consulta um snapshot completo em vez de adivinhar a associação.
            if !needs_snapshot {
                if legacy_compatible {
                    if let Some(legacy) = &mut self.legacy {
                        merge_available(legacy, update);
                    }
                } else if self.legacy.is_none() {
                    self.legacy = Some(update.clone());
                }
            }
        }
        // Suporta enriquecimento do mesmo contrato sem inferir contagens pelo
        // tamanho da lista, que pode ser limitada pelo backend.
        if let Some(update) = params
            .get("rateLimitResetCredits")
            .filter(|v| v.is_object())
        {
            merge_available(
                self.reset_credits
                    .get_or_insert_with(|| Value::Object(Map::new())),
                update,
            );
        }
        if let Some(allowed) = params["ordinaryUsageAllowed"].as_bool() {
            self.ordinary_usage_allowed = Some(allowed);
        }
        self.updated_at = Some(Local::now().timestamp());
        self.error = None;
        needs_snapshot
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
    }

    /// Bloco compacto para a lateral. O único comando é solicitar atualização.
    pub fn render(&self, ui: &mut egui::Ui, model: &str) -> bool {
        let mut refresh = false;
        let now = Local::now().timestamp();
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Limite restante").strong().size(12.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                refresh = ui
                    .small_button("Atualizar")
                    .on_hover_text("Atualizar limites e resets disponíveis")
                    .clicked();
            });
        });
        let selected = self.selected(model);
        if selected.entries.is_empty() {
            ui.label(
                RichText::new("Cota deste modelo não informada")
                    .size(11.0)
                    .color(MUTED),
            );
        } else {
            if selected.fallback {
                ui.add(
                    egui::Label::new(
                        RichText::new("Cota do modelo indisponível")
                            .size(11.0)
                            .color(MUTED),
                    )
                    .truncate(),
                );
            }
            for bucket in selected.entries.iter().take(2) {
                let title = bucket_title(bucket.key, bucket.value, selected.fallback);
                ui.add(egui::Label::new(RichText::new(&title).size(11.0).color(MUTED)).truncate())
                    .on_hover_text(&title);
                self.render_windows(ui, bucket.value, now, false);
            }
            if selected.entries.len() > 2 {
                ui.label(
                    RichText::new(format!("+{} cotas em Detalhes", selected.entries.len() - 2))
                        .size(10.0)
                        .color(MUTED),
                );
            }
        }
        let resets = match self.available_resets() {
            Some(count) => format!("Resets disponíveis: {count}"),
            None => "Resets disponíveis: não informado".into(),
        };
        ui.add(egui::Label::new(RichText::new(&resets).size(11.0)).truncate()).on_hover_text("Contagem informada pela conta. A lista de detalhes pode ser parcial. Nenhum reset é usado automaticamente.");
        if self.ordinary_usage_allowed == Some(false) {
            ui.add(
                egui::Label::new(
                    RichText::new("Uso incluído bloqueado pela conta")
                        .size(11.0)
                        .color(ERROR),
                )
                .truncate(),
            );
        }
        if let Some(error) = &self.error {
            ui.add(
                egui::Label::new(
                    RichText::new("Falha ao atualizar limites")
                        .size(11.0)
                        .color(ERROR),
                )
                .truncate(),
            )
            .on_hover_text(error);
        }
        egui::CollapsingHeader::new(RichText::new("Detalhes de uso").size(11.0))
            .id_salt("rustgpeitinhos-usage-details")
            .show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("rustgpeitinhos-usage-scroll").max_height(210.0).auto_shrink([false, true]).show(ui, |ui| {
                    ui.set_max_width(ui.available_width());
                    for bucket in self.all_buckets() {
                        let title = bucket_title(bucket.key, bucket.value, false);
                        ui.add(egui::Label::new(RichText::new(&title).strong().size(11.0)).wrap());
                        if let Some(slug) = string(bucket.value, "normalModelSlug") {
                            ui.add(egui::Label::new(RichText::new(format!("Modelo: {slug}")).size(10.0).color(MUTED)).wrap());
                        }
                        self.render_windows(ui, bucket.value, now, true);
                        if let Some(kind) = string(bucket.value, "rateLimitReachedType") {
                            ui.add(egui::Label::new(RichText::new(format!("Estado da conta: {kind}")).size(10.0).color(MUTED)).wrap());
                        }
                        ui.add_space(4.0);
                    }
                    if let Some(credits) = self.reset_credits.as_ref().and_then(|v| v["credits"].as_array()) {
                        for credit in credits.iter().take(20) {
                            let title = string(credit, "title").unwrap_or("Reset");
                            ui.add(egui::Label::new(RichText::new(title).size(11.0)).wrap());
                            if let Some(description) = string(credit, "description") {
                                ui.add(egui::Label::new(RichText::new(description).size(10.0).color(MUTED)).wrap());
                            }
                            if let Some(expires) = credit["expiresAt"].as_i64() {
                                let text = format!("{} {}", if expires <= now { "Expiração informada:" } else { "Expira:" }, local_date(expires));
                                ui.add(egui::Label::new(RichText::new(text).size(10.0).color(MUTED)).wrap());
                            }
                        }
                    }
                    if let Some(updated) = self.updated_at {
                        ui.add(egui::Label::new(RichText::new(format!("Atualizado: {}", local_date(updated))).size(10.0).color(MUTED)).wrap());
                    }
                    ui.add(egui::Label::new(RichText::new("Datas no horário local do sistema. Valores antigos não garantem que a cota já tenha renovado.").size(10.0).color(MUTED)).wrap());
                });
            });
        refresh
    }

    fn available_resets(&self) -> Option<i64> {
        self.reset_credits
            .as_ref()?
            .get("availableCount")?
            .as_i64()
            .filter(|count| *count >= 0)
    }

    fn all_buckets(&self) -> Vec<BucketView<'_>> {
        let mut result: Vec<_> = self
            .buckets
            .iter()
            .map(|(key, value)| BucketView { key, value })
            .collect();
        if let Some(legacy) = &self.legacy {
            let id = string(legacy, "limitId");
            if !result.iter().any(|bucket| {
                id.is_some_and(|id| bucket.key == id || string(bucket.value, "limitId") == Some(id))
            }) {
                result.push(BucketView {
                    key: "",
                    value: legacy,
                });
            }
        }
        result
    }

    fn selected(&self, model: &str) -> SelectedBuckets<'_> {
        let all = self.all_buckets();
        let exact: Vec<_> = all
            .iter()
            .filter(|bucket| {
                !model.is_empty()
                    && (bucket.key == model || string(bucket.value, "limitId") == Some(model))
            })
            .map(|b| BucketView {
                key: b.key,
                value: b.value,
            })
            .collect();
        if !exact.is_empty() {
            return SelectedBuckets {
                entries: exact,
                fallback: false,
            };
        }
        let normal: Vec<_> = all
            .iter()
            .filter(|bucket| {
                !model.is_empty() && string(bucket.value, "normalModelSlug") == Some(model)
            })
            .map(|b| BucketView {
                key: b.key,
                value: b.value,
            })
            .collect();
        if !normal.is_empty() {
            return SelectedBuckets {
                entries: normal,
                fallback: false,
            };
        }
        let mut general: Vec<_> = all
            .iter()
            .filter(|bucket| {
                (bucket.key == "codex" || string(bucket.value, "limitId") == Some("codex"))
                    && string(bucket.value, "normalModelSlug").is_none()
            })
            .map(|bucket| BucketView {
                key: bucket.key,
                value: bucket.value,
            })
            .collect();
        if general.is_empty() {
            general = all
                .into_iter()
                .filter(|bucket| {
                    bucket.key.is_empty()
                        && string(bucket.value, "limitId").is_none()
                        && string(bucket.value, "normalModelSlug").is_none()
                })
                .collect();
        }
        SelectedBuckets {
            entries: general,
            fallback: true,
        }
    }

    fn render_windows(&self, ui: &mut egui::Ui, bucket: &Value, now: i64, detailed: bool) {
        let mut count = 0;
        for (key, default_label) in [("primary", "Principal"), ("secondary", "Secundária")] {
            let Some(window) = bucket.get(key).filter(|v| v.is_object()) else {
                continue;
            };
            count += 1;
            let label = window["windowDurationMins"]
                .as_i64()
                .filter(|mins| *mins > 0)
                .map(window_duration)
                .unwrap_or_else(|| default_label.into());
            let remaining = remaining_percent(window);
            let text = match remaining {
                Some(value) => format!("{label} · {value:.0}%"),
                None => format!("{label} · não informado"),
            };
            let reset = reset_description(window["resetsAt"].as_i64(), now);
            ui.add(
                egui::Label::new(RichText::new(&text).size(11.0).color(
                    if remaining.is_some_and(|v| v <= 10.0) {
                        ERROR
                    } else {
                        ACCENT
                    },
                ))
                .truncate(),
            )
            .on_hover_text(format!("Percentual restante\n{reset}"));
            if let Some(remaining) = remaining {
                ui.add(
                    egui::ProgressBar::new((remaining / 100.0) as f32)
                        .desired_width(ui.available_width())
                        .desired_height(3.0)
                        .fill(if remaining <= 10.0 { ERROR } else { ACCENT }),
                );
            }
            if detailed {
                ui.add(egui::Label::new(RichText::new(reset).size(10.0).color(MUTED)).wrap());
            }
        }
        if count == 0 {
            ui.label(
                RichText::new("Percentual não informado")
                    .size(11.0)
                    .color(MUTED),
            );
        }
    }
}

fn string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field)?.as_str().filter(|s| !s.is_empty())
}

fn compatible_identity(
    previous: &Value,
    incoming_id: Option<&str>,
    incoming_model: Option<&str>,
) -> bool {
    incoming_id.is_none_or(|id| string(previous, "limitId") == Some(id))
        && incoming_model.is_none_or(|model| string(previous, "normalModelSlug") == Some(model))
}

fn merge_available(previous: &mut Value, update: &Value) {
    if update.is_null() {
        return;
    }
    if let (Some(previous), Some(update)) = (previous.as_object_mut(), update.as_object()) {
        for (key, value) in update {
            if !value.is_null() {
                merge_available(previous.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
    } else {
        *previous = update.clone();
    }
}

fn remaining_percent(window: &Value) -> Option<f64> {
    window
        .get("usedPercent")?
        .as_f64()
        .filter(|used| used.is_finite())
        .map(|used| 100.0 - used.clamp(0.0, 100.0))
}

fn bucket_title(key: &str, value: &Value, fallback: bool) -> String {
    let id = string(value, "limitId").unwrap_or(key);
    let name = string(value, "limitName").unwrap_or(if id.is_empty() {
        "Conta · modelo não informado"
    } else {
        id
    });
    if fallback && (key == "codex" || id == "codex") {
        if name == "codex" {
            "Codex geral".into()
        } else {
            format!("Codex geral · {name}")
        }
    } else if !key.is_empty() && key != name {
        format!("{name} · {key}")
    } else {
        name.into()
    }
}

fn window_duration(minutes: i64) -> String {
    if minutes % 1440 == 0 {
        format!("{} d", minutes / 1440)
    } else if minutes % 60 == 0 {
        format!("{} h", minutes / 60)
    } else if minutes > 60 {
        format!("{} h {} min", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} min")
    }
}

fn local_date(timestamp: i64) -> String {
    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|date| date.format("%d/%m/%Y %H:%M %:z").to_string())
        .unwrap_or_else(|| "data indisponível".into())
}

fn reset_description(timestamp: Option<i64>, now: i64) -> String {
    match timestamp {
        Some(timestamp) if timestamp <= now => format!(
            "Reinício informado: {} · aguardando atualização",
            local_date(timestamp)
        ),
        Some(timestamp) => format!("Renova: {}", local_date(timestamp)),
        None => "Reinício não informado".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn percentage_is_remaining_clamped_and_missing_is_not_zero() {
        assert_eq!(remaining_percent(&json!({"usedPercent": 37})), Some(63.0));
        assert_eq!(remaining_percent(&json!({"usedPercent": 0})), Some(100.0));
        assert_eq!(remaining_percent(&json!({"usedPercent": 100})), Some(0.0));
        assert_eq!(remaining_percent(&json!({"usedPercent": 140})), Some(0.0));
        assert_eq!(remaining_percent(&json!({"usedPercent": -8})), Some(100.0));
        assert_eq!(remaining_percent(&json!({})), None);
    }

    #[test]
    fn reset_count_is_authoritative_and_independent_of_detail_rows() {
        let mut state = UsageState::default();
        assert_eq!(state.available_resets(), None);
        state.apply_snapshot(
            &json!({"rateLimits":{}, "rateLimitResetCredits":{"availableCount":3,"credits":[]}}),
        );
        assert_eq!(state.available_resets(), Some(3));
        state.apply_snapshot(
            &json!({"rateLimits":{}, "rateLimitResetCredits":{"availableCount":0,"credits":null}}),
        );
        assert_eq!(state.available_resets(), Some(0));
        state.apply_snapshot(&json!({"rateLimits":{}}));
        assert_eq!(state.available_resets(), None);
    }

    #[test]
    fn rolling_updates_preserve_metadata_but_full_snapshot_clears_it() {
        let mut state = UsageState::default();
        state.apply_snapshot(&json!({"rateLimits":{"limitId":"codex","planType":"plus","primary":{"usedPercent":10,"resetsAt":100,"windowDurationMins":300}},"rateLimitsByLimitId":{"codex":{"limitId":"codex","planType":"plus","primary":{"usedPercent":10,"resetsAt":100,"windowDurationMins":300}}},"rateLimitResetCredits":{"availableCount":2},"ordinaryUsageAllowed":false}));
        state.apply_update(&json!({"rateLimits":{"limitId":null,"planType":null,"primary":{"usedPercent":20,"resetsAt":null,"windowDurationMins":null}},"rateLimitResetCredits":null}));
        assert_eq!(state.buckets["codex"]["primary"]["usedPercent"], 20);
        assert_eq!(state.buckets["codex"]["primary"]["resetsAt"], 100);
        assert_eq!(state.buckets["codex"]["primary"]["windowDurationMins"], 300);
        assert_eq!(state.buckets["codex"]["planType"], "plus");
        assert_eq!(state.available_resets(), Some(2));
        assert_eq!(state.ordinary_usage_allowed, Some(false));
        state.apply_snapshot(&json!({"rateLimits":{"primary":{"usedPercent":30}}}));
        assert!(state.buckets.is_empty());
        assert!(state.available_resets().is_none());
        assert!(state.ordinary_usage_allowed.is_none());
        assert!(state.legacy.as_ref().unwrap()["primary"]["resetsAt"].is_null());
    }

    #[test]
    fn anonymous_update_requests_snapshot_without_changing_unidentified_buckets() {
        for buckets in [
            json!({"codex":{"limitId":null,"primary":{"usedPercent":10}}}),
            json!({
                "codex":{"limitId":null,"primary":{"usedPercent":10}},
                "sol":{"limitId":null,"normalModelSlug":"gpt-6.2-sol","primary":{"usedPercent":50}},
                "astra":{"limitId":null,"normalModelSlug":"gpt-6.2-astra","primary":{"usedPercent":70}}
            }),
        ] {
            let mut state = UsageState::default();
            let mut snapshot = json!({
                "rateLimits":{"limitId":null,"primary":{"usedPercent":10}},
                "rateLimitsByLimitId":buckets
            });
            state.apply_snapshot(&snapshot);
            let before = state.buckets.clone();
            assert!(state.apply_update(&json!({"rateLimits":{
                "limitId":null,"normalModelSlug":null,"primary":{"usedPercent":20}
            }})));
            assert_eq!(state.buckets, before);
            assert_eq!(state.legacy.as_ref().unwrap()["primary"]["usedPercent"], 10);

            // A nova leitura autoritativa atualiza a cota geral, preservando as
            // porcentagens específicas de Sol e Astra.
            snapshot["rateLimits"]["primary"]["usedPercent"] = json!(20);
            snapshot["rateLimitsByLimitId"]["codex"]["primary"]["usedPercent"] = json!(20);
            state.apply_snapshot(&snapshot);
            let general = state.selected("modelo-sem-cota");
            assert_eq!(general.entries.len(), 1);
            assert_eq!(
                remaining_percent(&general.entries[0].value["primary"]),
                Some(80.0)
            );
            for key in ["sol", "astra"] {
                if let Some(previous) = before.get(key) {
                    assert_eq!(&state.buckets[key], previous);
                }
            }
        }

        let mut legacy_only = UsageState::default();
        assert!(!legacy_only.apply_update(&json!({"rateLimits":{
            "primary":{"usedPercent":20}
        }})));
        assert_eq!(
            legacy_only.legacy.as_ref().unwrap()["primary"]["usedPercent"],
            20
        );
    }

    #[test]
    fn model_quotas_are_separate_and_general_fallback_is_labelled() {
        let mut state = UsageState::default();
        state.apply_snapshot(&json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":1}},"rateLimitsByLimitId":{
            "codex":{"limitId":"codex","primary":{"usedPercent":1}},
            "sol-boost":{"limitId":"sol-boost","normalModelSlug":"gpt-6.2-sol","primary":{"usedPercent":20}},
            "astra-fast":{"limitId":"astra-fast","normalModelSlug":"gpt-6.2-astra","primary":{"usedPercent":70}},
            "astra-deep":{"limitId":"astra-deep","normalModelSlug":"gpt-6.2-astra","primary":{"usedPercent":40}}
        }}));
        let sol = state.selected("gpt-6.2-sol");
        assert_eq!(sol.entries.len(), 1);
        assert_eq!(sol.entries[0].key, "sol-boost");
        assert!(!sol.fallback);
        let astra = state.selected("gpt-6.2-astra");
        assert_eq!(astra.entries.len(), 2);
        assert!(astra.entries.iter().all(|b| b.key.starts_with("astra-")));
        let exact = state.selected("astra-fast");
        assert_eq!(exact.entries.len(), 1);
        assert_eq!(exact.entries[0].key, "astra-fast");
        let unknown = state.selected("gpt-outro");
        assert_eq!(unknown.entries.len(), 1);
        assert!(unknown.fallback);
        assert!(
            bucket_title(unknown.entries[0].key, unknown.entries[0].value, true)
                .starts_with("Codex geral")
        );
        state.apply_update(&json!({"rateLimits":{"limitId":"astra-fast","normalModelSlug":null,"primary":{"usedPercent":80}}}));
        assert_eq!(state.buckets["astra-fast"]["primary"]["usedPercent"], 80);
        assert_eq!(
            state.buckets["astra-fast"]["normalModelSlug"],
            "gpt-6.2-astra"
        );
        assert_eq!(state.buckets["astra-deep"]["primary"]["usedPercent"], 40);
        assert_eq!(state.buckets["codex"]["primary"]["usedPercent"], 1);
    }

    #[test]
    fn past_reset_date_does_not_restore_usage_or_invent_reset_credits() {
        let window = json!({"usedPercent":100,"resetsAt":1});
        assert_eq!(remaining_percent(&window), Some(0.0));
        assert!(reset_description(Some(1), 10).contains("aguardando atualização"));
        assert!(!reset_description(Some(20), 10).contains("aguardando"));
        assert_eq!(reset_description(None, 10), "Reinício não informado");
        assert_eq!(local_date(i64::MAX), "data indisponível");
        let mut state = UsageState::default();
        state.apply_snapshot(&json!({"rateLimits":{"primary":window}}));
        assert_eq!(state.available_resets(), None);
        state.reset();
        assert!(state.updated_at.is_none());
        assert!(state.all_buckets().is_empty());
    }
}
