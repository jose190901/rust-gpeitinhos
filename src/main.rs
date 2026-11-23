//! Interface nativa do RustGPeitinhos e diagnósticos do motor oficial.
use rustgpeitinhos::{
    account::SubscriptionAccount,
    runtime::{Runtime, RuntimeEvent},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn request(runtime: &Runtime, method: &str, params: Value) -> Result<Value, String> {
    let id = runtime.request(method, params)?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(format!("Prazo excedido em {method}."));
        }
        match runtime
            .events
            .recv_timeout(remaining)
            .map_err(|_| format!("Sem resposta para {method}."))?
        {
            RuntimeEvent::Disconnected(error) => return Err(error),
            RuntimeEvent::Message(message) => {
                if message.get("method").is_some() && message.get("id").is_some() {
                    runtime.reject(
                        message["id"].clone(),
                        "Este diagnóstico não executa ferramentas ou ações.",
                    )?;
                } else if message["id"].as_u64() == Some(id) {
                    if let Some(error) = message.get("error") {
                        return Err(format!(
                            "{method}: {}",
                            error["message"].as_str().unwrap_or("Erro do protocolo")
                        ));
                    }
                    return Ok(message["result"].clone());
                }
            }
        }
    }
}

fn run() -> Result<(), String> {
    let paths = rustgpeitinhos::paths::discover();
    let root = paths.root;
    let binary = paths.codex;
    let runtime = Runtime::spawn(&binary, &root)?;
    request(
        &runtime,
        "initialize",
        json!({"clientInfo":{"name":"rustgpeitinhos","title":"RUST GPeitinhos","version":"0.1.0"},"capabilities":{"experimentalApi":true}}),
    )?;
    runtime.notify("initialized", json!({}))?;
    let account = SubscriptionAccount::verify(&request(
        &runtime,
        "account/read",
        json!({"refreshToken":false}),
    )?)?;
    if std::env::args().any(|s| s == "--diagnostico-uso") {
        let limits = request(&runtime, "account/rateLimits/read", json!({}))?;
        let buckets:serde_json::Map<String,Value>=limits["rateLimitsByLimitId"].as_object().into_iter().flatten().map(|(key,value)| (key.clone(),json!({"modelo":value["normalModelSlug"],"primaria":value["primary"],"secundaria":value["secondary"]}))).collect();
        println!("{}",serde_json::to_string_pretty(&json!({"autenticacao":"chatgpt","cotas":buckets,"primaria":limits["rateLimits"]["primary"],"secundaria":limits["rateLimits"]["secondary"],"resetsDisponiveis":limits["rateLimitResetCredits"]["availableCount"],"mensagensEnviadas":0})).map_err(|e|e.to_string())?);
        return Ok(());
    }
    let models = request(
        &runtime,
        "model/list",
        json!({"limit":100,"includeHidden":false}),
    )?;
    let threads = request(
        &runtime,
        "thread/list",
        json!({"limit":100,"currentAccountOnly":true,"sortKey":"updated_at","useStateDbOnly":true}),
    )?;
    let skills = request(
        &runtime,
        "skills/list",
        json!({"cwds":[root],"forceReload":false}),
    )?;
    let model_ids: Vec<&str> = models["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["model"].as_str())
        .collect();
    let skills_count: usize = skills["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| entry["skills"].as_array().map_or(0, Vec::len))
        .sum();
    let thread_count = threads["data"].as_array().map_or(0, Vec::len);
    let mut history_items = Value::Null;
    if std::env::args().any(|s| s == "--diagnostico-historico") {
        let current = std::env::var("CODEX_THREAD_ID").unwrap_or_default();
        if let Some(thread) = threads["data"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["id"].as_str().is_some_and(|id| id != current))
        {
            let result = request(
                &runtime,
                "thread/read",
                json!({"threadId":thread["id"],"includeTurns":true}),
            )?;
            let count: usize = result["thread"]["turns"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| t["items"].as_array().map_or(0, Vec::len))
                .sum();
            history_items = json!(count);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status":"integração Rust conectada",
            "autenticacao":"chatgpt",
            "plano":account.plan,
            "modelos":model_ids,
            "conversasNaPrimeiraPagina":thread_count,
            "maisConversas":!threads["nextCursor"].is_null(),
            "skillsDescobertas":skills_count,
            "mensagensEnviadas":0,
            "itensDeConversaExistenteLidos":history_items,
        "interfaceGrafica":"nativa em Rust; este comando executa apenas o diagnóstico",
            "navegadorDesktop":"não implementado neste cliente"
        }))
        .map_err(|e| e.to_string())?
    );
    Ok(())
}

fn main() {
    let launched = Instant::now();
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .any(|s| s == "--diagnostico" || s == "--diagnostico-historico" || s == "--diagnostico-uso")
    {
        if let Err(error) = run() {
            eprintln!("RustGPeitinhos: {error}");
            std::process::exit(1);
        }
        return;
    }
    let paths = rustgpeitinhos::paths::discover();
    let root = paths.root;
    let binary = paths.codex;
    let smoke_chat = args
        .iter()
        .any(|s| s == "--smoke-chat" || s == "--smoke-steer");
    let smoke = smoke_chat
        || args
            .iter()
            .any(|s| s == "--smoke-test" || s == "--smoke-terminal");
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("RUST GPeitinhos — Codex")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([900.0, 640.0]);
    if let Ok(image) = image::load_from_memory(include_bytes!("../assets/logo.png")) {
        let rgba = image.to_rgba8();
        viewport = viewport.with_icon(std::sync::Arc::new(eframe::egui::IconData {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        }));
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "RustGPeitinhos",
        options,
        Box::new(move |cc| {
            Ok(Box::new(rustgpeitinhos::app::RustGPeitinhosApp::new(
                cc, root, binary, smoke, smoke_chat, launched, paths.data,
            )))
        }),
    ) {
        eprintln!("Não foi possível abrir o RustGPeitinhos: {error}");
        std::process::exit(1);
    }
}
