//! Testa a ponte Git real, inclusive um filtro que tenta escrever fora da pasta.
use rustgpeitinhos::{
    account::SubscriptionAccount,
    paths,
    runtime::{Runtime, RuntimeEvent},
    terminal::sandbox_policy,
    view_model::AccessMode,
    workspace::WorkspaceReview,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn git(cwd: &Path, args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    let result = cmd
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| e.to_string())?;
    if result.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&result.stderr).into())
    }
}
fn request(runtime: &Runtime, method: &str, params: Value) -> Result<Value, String> {
    let id = runtime.request(method, params)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match runtime
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| "Timeout")?
        {
            RuntimeEvent::Disconnected(e) => return Err(e),
            RuntimeEvent::Message(m) => {
                if m["id"].as_u64() == Some(id) {
                    if let Some(e) = m.get("error") {
                        return Err(e["message"].as_str().unwrap_or("Erro CLI").into());
                    }
                    return Ok(m["result"].clone());
                }
            }
        }
    }
}
fn drive(runtime: &Runtime, review: &mut WorkspaceReview) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut pending: HashMap<u64, Sender<Result<Vec<u8>, String>>> = HashMap::new();
    loop {
        review.pump();
        for command in review.take_commands() {
            let id=runtime.request("command/exec",json!({"command":command.command,"cwd":command.cwd,"env":command.env,"sandboxPolicy":sandbox_policy(AccessMode::ReadOnly,&command.cwd),"timeoutMs":8000,"outputBytesCap":1024*1024}))?;
            pending.insert(id, command.reply);
        }
        if !review.loading && !review.diff_loading {
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err("Revisão não encerrou.".into());
        }
        match runtime.events.recv_timeout(Duration::from_millis(20)) {
            Ok(RuntimeEvent::Disconnected(e)) => return Err(e),
            Ok(RuntimeEvent::Message(m)) => {
                if let Some(reply) = m["id"].as_u64().and_then(|id| pending.remove(&id)) {
                    let result = if let Some(error) = m.get("error") {
                        Err(error["message"].as_str().unwrap_or("Erro sandbox").into())
                    } else if m["result"]["exitCode"].as_i64() != Some(0) {
                        Err(m["result"]["stderr"]
                            .as_str()
                            .unwrap_or("Git falhou")
                            .into())
                    } else {
                        Ok(m["result"]["stdout"]
                            .as_str()
                            .unwrap_or("")
                            .as_bytes()
                            .to_vec())
                    };
                    let _ = reply.send(result);
                }
            }
            Err(_) => {}
        }
    }
}
fn run() -> Result<(), String> {
    let paths = paths::discover();
    let fixture =
        Fixture(std::env::temp_dir().join(format!("rustgpeitinhos-git-real-{}", std::process::id())));
    std::fs::create_dir(&fixture.0).map_err(|e| e.to_string())?;
    let selected = fixture.0.join("selected");
    std::fs::create_dir(&selected).map_err(|e| e.to_string())?;
    std::fs::write(selected.join("a.txt"), "old\n").map_err(|e| e.to_string())?;
    git(&fixture.0, &["init", "--quiet"])?;
    git(&fixture.0, &["add", "--", "selected/a.txt"])?;
    git(
        &fixture.0,
        &[
            "-c",
            "user.name=RustGPeitinhos Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    )?;
    std::fs::write(selected.join("a.txt"), "new\n").map_err(|e| e.to_string())?;
    let runtime = Runtime::spawn(&paths.codex, &paths.root)?;
    request(
        &runtime,
        "initialize",
        json!({"clientInfo":{"name":"rustgpeitinhos-review-check","version":"0.1.0"},"capabilities":{"experimentalApi":true}}),
    )?;
    runtime.notify("initialized", json!({}))?;
    SubscriptionAccount::verify(&request(
        &runtime,
        "account/read",
        json!({"refreshToken":false}),
    )?)?;
    let mut review = WorkspaceReview::new();
    review.refresh(selected.to_str().ok_or("Caminho inválido")?);
    drive(&runtime, &mut review)?;
    if review.error.is_some() || !review.files.iter().any(|f| f.path == "a.txt") {
        return Err(format!("Status falhou: {:?}", review.error));
    }
    review.select("a.txt");
    drive(&runtime, &mut review)?;
    if review.error.is_some() || !review.diff.contains("+new") {
        return Err(format!("Diff falhou: {:?}", review.error));
    }
    git(
        &fixture.0,
        &[
            "config",
            "filter.rustgpeitinhos-check.clean",
            "printf escaped > escaped-marker; cat",
        ],
    )?;
    std::fs::write(
        fixture.0.join(".gitattributes"),
        "selected/a.txt filter=rustgpeitinhos-check\n",
    )
    .map_err(|e| e.to_string())?;
    review.refresh(selected.to_str().unwrap());
    drive(&runtime, &mut review)?;
    if review.error.is_none() {
        review.select("a.txt");
        drive(&runtime, &mut review)?;
    }
    if fixture.0.join("escaped-marker").exists() {
        return Err("Filtro escapou da revisão somente leitura.".into());
    }
    // Controle positivo somente na fixture criada por este teste: confirma que
    // o filtro de fato tenta escrever quando o mesmo Git roda sem sandbox.
    git(
        &selected,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ],
    )?;
    if !fixture.0.join("escaped-marker").exists() {
        return Err("O controle do filtro não executou a tentativa de escrita.".into());
    }
    println!(
        "{}",
        json!({"revisaoGit":"ok","status":true,"diff":true,"escritaDoFiltroBloqueada":true,"controleDoFiltroConfirmado":true,"mensagensAoModelo":0})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
