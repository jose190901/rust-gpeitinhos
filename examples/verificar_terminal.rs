//! Verificação local explícita: usa o CLI, sem enviar mensagens ao modelo.
use rustgpeitinhos::{
    account::SubscriptionAccount,
    paths,
    runtime::{Runtime, RuntimeEvent},
    terminal::Terminal,
    view_model::AccessMode,
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn next(runtime: &Runtime, terminal: &mut Terminal, deadline: Instant) -> Result<Value, String> {
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        if wait.is_zero() {
            return Err("Prazo excedido na verificação do terminal.".into());
        }
        match runtime
            .events
            .recv_timeout(wait)
            .map_err(|_| "Sem resposta do CLI.")?
        {
            RuntimeEvent::Disconnected(error) => return Err(error),
            RuntimeEvent::Message(message) => {
                if message["method"] == "command/exec/outputDelta" {
                    terminal.notification(&message["params"]);
                }
                if message.get("method").is_some() && message.get("id").is_some() {
                    runtime.reject(
                        message["id"].clone(),
                        "A verificação não concede permissões adicionais.",
                    )?;
                } else {
                    return Ok(message);
                }
            }
        }
    }
}
fn wait(
    runtime: &Runtime,
    terminal: &mut Terminal,
    id: u64,
    deadline: Instant,
) -> Result<Value, String> {
    loop {
        let message = next(runtime, terminal, deadline)?;
        if message["id"].as_u64() == Some(id) {
            if let Some(error) = message.get("error") {
                return Err(error["message"].as_str().unwrap_or("Erro CLI").into());
            }
            return Ok(message["result"].clone());
        }
    }
}
fn ready(runtime: &Runtime, terminal: &mut Terminal, deadline: Instant) -> Result<(), String> {
    while !terminal.output.contains("READY\n") {
        next(runtime, terminal, deadline)?;
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let paths = paths::discover();
    let runtime = Runtime::spawn(&paths.codex, &paths.root)?;
    let mut terminal = Terminal::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let id=runtime.request("initialize",json!({"clientInfo":{"name":"rustgpeitinhos-terminal-check","version":"0.1.0"},"capabilities":{"experimentalApi":true}}))?;
    wait(&runtime, &mut terminal, id, deadline)?;
    runtime.notify("initialized", json!({}))?;
    let id = runtime.request("account/read", json!({"refreshToken":false}))?;
    SubscriptionAccount::verify(&wait(&runtime, &mut terminal, id, deadline)?)?;
    terminal.command = r#"printf '%s\n' 'ç'; printf 'stderr_ok\n' >&2; exit 7"#.into();
    let (process, params) = terminal
        .start(
            paths.root.to_str().ok_or("Caminho UTF-8 inválido")?,
            AccessMode::ReadOnly,
        )
        .unwrap();
    let id = runtime.request("command/exec", params)?;
    let result = wait(&runtime, &mut terminal, id, deadline)?;
    terminal.finish(&process, Ok(&result));
    if terminal.exit_code != Some(7)
        || !terminal.output.contains("ç\n")
        || !terminal.output.contains("stderr_ok\n")
    {
        return Err("Saída/exit code incorretos.".into());
    }
    terminal.clear();
    terminal.command = r#"printf 'READY\n'; cat"#.into();
    let (process, params) = terminal
        .start(paths.root.to_str().unwrap(), AccessMode::ReadOnly)
        .unwrap();
    let exec = runtime.request("command/exec", params)?;
    ready(&runtime, &mut terminal, deadline)?;
    terminal.input = "entrada_ação".into();
    let write = runtime.request("command/exec/write", terminal.write_params(false).unwrap())?;
    wait(&runtime, &mut terminal, write, deadline)?;
    let close = runtime.request("command/exec/write", terminal.write_params(true).unwrap())?;
    // A resposta final pode chegar antes da confirmação de EOF: esperar o exec
    // diretamente preserva os dados que importam para a verificação.
    let _ = close;
    let result = wait(&runtime, &mut terminal, exec, deadline)?;
    terminal.finish(&process, Ok(&result));
    if terminal.exit_code != Some(0) || !terminal.output.contains("entrada_ação\n") {
        return Err("Entrada padrão/EOF não funcionaram.".into());
    }
    terminal.clear();
    terminal.command = r#"printf 'READY\n'; exec sleep 30"#.into();
    let (process, params) = terminal
        .start(paths.root.to_str().unwrap(), AccessMode::ReadOnly)
        .unwrap();
    let exec = runtime.request("command/exec", params)?;
    ready(&runtime, &mut terminal, deadline)?;
    runtime.request("command/exec/terminate", json!({"processId":process}))?;
    let result = wait(&runtime, &mut terminal, exec, deadline)?;
    terminal.finish(&process, Ok(&result));
    if terminal.running {
        return Err("Comando não foi encerrado.".into());
    }
    println!(
        "{}",
        json!({"terminal":"ok","streaming":true,"utf8":true,"stderr":true,"stdinEof":true,"interrupcao":true,"sandbox":"readOnly","mensagensAoModelo":0})
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
