//! Transporte local para o Codex oficial. Não lê nem copia credenciais.
//!
//! Este módulo apenas transporta o protocolo. Antes de iniciar qualquer turno,
//! o cliente deve verificar `account/read` e exigir uma conta do tipo `chatgpt`.
//! A resposta de `initialize` também deve preceder `initialized` e outros RPCs.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;

const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
const QUEUE_CAPACITY: usize = 128;

const CODEX_ARGS: &[&str] = &[
    "app-server",
    "--listen",
    "stdio://",
    "-c",
    "model_provider=\"openai\"",
    "-c",
    "forced_login_method=\"chatgpt\"",
];

#[derive(Debug)]
pub enum RuntimeEvent {
    /// Resposta, notificação ou pedido do servidor, preservado integralmente.
    Message(Value),
    /// Falha terminal; nenhum pedido será reenviado automaticamente.
    Disconnected(String),
}

pub struct Runtime {
    pub events: Receiver<RuntimeEvent>,
    outgoing: SyncSender<Vec<u8>>,
    next_id: AtomicU64,
    closed: Arc<AtomicBool>,
    process: Arc<Process>,
}

struct Process {
    child: Mutex<Option<Child>>,
}

impl Process {
    fn terminate(&self) {
        // Retirar Child uma única vez também impede sinalizar um PID reutilizado.
        let mut child = match self.child.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(ref mut child) = child {
            #[cfg(unix)]
            {
                // O subprocesso recebe seu próprio grupo em spawn(). Assim, o
                // encerramento também alcança MCPs e ferramentas descendentes.
                unsafe extern "C" {
                    fn kill(pid: i32, signal: i32) -> i32;
                }
                if let Ok(pid) = i32::try_from(child.id()) {
                    if pid > 1 {
                        // SAFETY: PID positivo obtido de Child, grupo criado por
                        // nós; não há ponteiros. SIGKILL encerra o grupo inteiro.
                        unsafe { kill(-pid, 9) };
                    }
                }
            }
            // Cobre plataformas sem grupos e a saída do processo durante kill.
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.terminate();
    }
}

impl Runtime {
    #[cfg(test)]
    pub(crate) fn test_transport() -> (Self, Receiver<Vec<u8>>) {
        let (outgoing, incoming) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (_tx, events) = mpsc::channel();
        (
            Self {
                events,
                outgoing,
                next_id: AtomicU64::new(1),
                closed: Arc::new(AtomicBool::new(false)),
                process: Arc::new(Process {
                    child: Mutex::new(None),
                }),
            },
            incoming,
        )
    }

    pub fn spawn(binary: &Path, cwd: &Path) -> Result<Self, String> {
        if !binary.is_absolute() {
            return Err("O caminho do Codex deve ser absoluto.".into());
        }
        if !cwd.is_dir() {
            return Err("A pasta de trabalho não existe ou não é uma pasta.".into());
        }

        let mut command = Command::new(binary);
        command
            .args(CODEX_ARGS)
            .current_dir(cwd)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in std::env::vars_os() {
            if allowed_env(&name.to_string_lossy()) {
                command.env(name, value);
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }

        let mut child = command
            .spawn()
            .map_err(|error| format!("Não foi possível iniciar o Codex: {error}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let process = Arc::new(Process {
            child: Mutex::new(Some(child)),
        });
        let (Some(mut stdin), Some(stdout), Some(mut stderr)) = (stdin, stdout, stderr) else {
            process.terminate();
            return Err("Não foi possível abrir os canais do Codex.".into());
        };

        let (event_tx, events) = mpsc::channel();
        let (outgoing, incoming) = mpsc::sync_channel::<Vec<u8>>(QUEUE_CAPACITY);
        let closed = Arc::new(AtomicBool::new(false));
        // Construir antes das threads garante a limpeza se alguma não iniciar.
        let runtime = Self {
            events,
            outgoing,
            next_id: AtomicU64::new(1),
            closed: Arc::clone(&closed),
            process: Arc::clone(&process),
        };

        let writer_closed = Arc::clone(&closed);
        let writer_events = event_tx.clone();
        let writer_process = Arc::clone(&process);
        thread::Builder::new()
            .name("ponte-codex-write".into())
            .spawn(move || {
                while let Ok(bytes) = incoming.recv() {
                    if writer_closed.load(Ordering::Acquire) {
                        break;
                    }
                    if stdin.write_all(&bytes).and_then(|_| stdin.flush()).is_err() {
                        disconnected(
                            &writer_closed,
                            &writer_events,
                            "O canal de envio para o Codex foi encerrado.",
                        );
                        writer_process.terminate();
                        break;
                    }
                }
            })
            .map_err(|error| format!("Não foi possível iniciar o envio: {error}"))?;

        let reader_closed = Arc::clone(&closed);
        let reader_process = Arc::clone(&process);
        thread::Builder::new()
            .name("ponte-codex-read".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut bytes = Vec::new();
                    // take() limita a alocação mesmo se não houver uma quebra
                    // de linha num fluxo inesperado ou corrompido.
                    let read = (&mut reader)
                        .take((MAX_MESSAGE_BYTES + 1) as u64)
                        .read_until(b'\n', &mut bytes);
                    match read {
                        Ok(0) => {
                            disconnected(
                                &reader_closed,
                                &event_tx,
                                "A conexão com o Codex foi encerrada.",
                            );
                            break;
                        }
                        Ok(_) if bytes.len() > MAX_MESSAGE_BYTES => {
                            disconnected(
                                &reader_closed,
                                &event_tx,
                                "O Codex enviou uma mensagem maior que o limite local.",
                            );
                            break;
                        }
                        Ok(_) => match parse_message(&bytes) {
                            Ok(Some(value)) => {
                                if event_tx.send(RuntimeEvent::Message(value)).is_err() {
                                    break;
                                }
                            }
                            Ok(None) => continue,
                            Err(message) => {
                                disconnected(&reader_closed, &event_tx, message);
                                break;
                            }
                        },
                        Err(_) => {
                            disconnected(
                                &reader_closed,
                                &event_tx,
                                "Falha ao ler o protocolo do Codex.",
                            );
                            break;
                        }
                    }
                }
                reader_process.terminate();
            })
            .map_err(|error| format!("Não foi possível iniciar a leitura: {error}"))?;

        thread::Builder::new()
            .name("ponte-codex-stderr".into())
            .spawn(move || {
                // Drenar sem armazenar ou imprimir: mensagens de dependências
                // podem conter URLs privadas ou outras informações sensíveis.
                let _ = std::io::copy(&mut stderr, &mut std::io::sink());
            })
            .map_err(|error| format!("Não foi possível iniciar o canal auxiliar: {error}"))?;

        Ok(runtime)
    }

    pub fn request(&self, method: &str, params: Value) -> Result<u64, String> {
        let id = self
            .next_id
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| "Os identificadores de pedidos se esgotaram.".to_string())?;
        self.send(json!({ "id": id, "method": method, "params": params }))?;
        Ok(id)
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.send(json!({ "method": method, "params": params }))
    }

    pub fn respond(&self, id: Value, result: Value) -> Result<(), String> {
        self.send(json!({ "id": id, "result": result }))
    }

    pub fn reject(&self, id: Value, message: &str) -> Result<(), String> {
        self.send(json!({
            "id": id,
            "error": { "code": -32601, "message": message }
        }))
    }

    fn send(&self, value: Value) -> Result<(), String> {
        if self.closed.load(Ordering::Acquire) {
            return Err("O Codex está desconectado. Reconecte antes de enviar.".into());
        }
        let mut bytes = serde_json::to_vec(&value)
            .map_err(|_| "Não foi possível codificar o pedido.".to_string())?;
        if bytes.len() >= MAX_MESSAGE_BYTES {
            return Err("A mensagem excede o limite local de 32 MiB.".into());
        }
        bytes.push(b'\n');
        self.outgoing.try_send(bytes).map_err(|error| match error {
            TrySendError::Full(_) => {
                "A fila de envio está cheia. Aguarde antes de tentar novamente.".into()
            }
            TrySendError::Disconnected(_) => "O canal de envio do Codex está encerrado.".into(),
        })
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // Não emitir falha ao encerrar intencionalmente. Também não aguardar
        // respostas RPC: o grupo é encerrado e o processo principal recolhido.
        self.closed.store(true, Ordering::Release);
        self.process.terminate();
    }
}

fn disconnected(closed: &AtomicBool, events: &mpsc::Sender<RuntimeEvent>, message: &str) {
    if !closed.swap(true, Ordering::AcqRel) {
        let _ = events.send(RuntimeEvent::Disconnected(message.into()));
    }
}

fn parse_message(bytes: &[u8]) -> Result<Option<Value>, &'static str> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| "O Codex enviou JSON inválido; a conexão foi encerrada.")?;
    if !value.is_object() {
        return Err("O Codex enviou uma mensagem de protocolo inválida.");
    }
    Ok(Some(value))
}

fn allowed_env(name: &str) -> bool {
    matches!(
        name,
        "HOME"
            | "USER"
            | "LOGNAME"
            | "PATH"
            | "SHELL"
            | "CODEX_HOME"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "LANG"
            | "LANGUAGE"
            | "TZ"
            | "TERM"
            | "COLORTERM"
            | "NO_COLOR"
            | "DISPLAY"
            | "WAYLAND_DISPLAY"
            | "XAUTHORITY"
            | "DBUS_SESSION_BUS_ADDRESS"
            | "DESKTOP_SESSION"
            | "XDG_CONFIG_HOME"
            | "XDG_CACHE_HOME"
            | "XDG_DATA_HOME"
            | "XDG_STATE_HOME"
            | "XDG_RUNTIME_DIR"
            | "XDG_CURRENT_DESKTOP"
            | "XDG_SESSION_TYPE"
            | "XDG_SESSION_DESKTOP"
            | "SSL_CERT_FILE"
            | "SSL_CERT_DIR"
            | "CURL_CA_BUNDLE"
            | "REQUESTS_CA_BUNDLE"
            | "NODE_EXTRA_CA_CERTS"
            // Preservar proxies permite conexões em redes que os exigem. Os
            // valores nunca são armazenados nem incluídos em logs pelo runtime.
            | "HTTP_PROXY"
            | "HTTPS_PROXY"
            | "ALL_PROXY"
            | "NO_PROXY"
            | "http_proxy"
            | "https_proxy"
            | "all_proxy"
            | "no_proxy"
    ) || name.starts_with("LC_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_server_requests_and_string_ids() {
        let message = parse_message(
            br#"{"id":"approval-4","method":"item/commandExecution/requestApproval","params":{"threadId":"t"}}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(message["id"], "approval-4");
        assert_eq!(message["method"], "item/commandExecution/requestApproval");
    }

    #[test]
    fn handles_crlf_blank_lines_and_broken_json_without_echoing_content() {
        assert!(parse_message(b" \r\n").unwrap().is_none());
        assert_eq!(
            parse_message(b"{\"id\":1,\"result\":{}}\r\n")
                .unwrap()
                .unwrap()["id"],
            1
        );
        assert!(
            parse_message(b"{secret")
                .unwrap_err()
                .contains("JSON inválido")
        );
        assert!(parse_message(b"[]").is_err());
    }

    #[test]
    fn environment_removes_api_keys_and_provider_overrides() {
        for name in [
            "OPENAI_API_KEY",
            "OPENAI_BASE_URL",
            "AZURE_OPENAI_API_KEY",
            "CODEX_API_KEY",
            "CODEX_MODEL_PROVIDER",
            "NODE_OPTIONS",
            "LD_PRELOAD",
        ] {
            assert!(!allowed_env(name), "variável indevida: {name}");
        }
        for name in [
            "HOME",
            "CODEX_HOME",
            "PATH",
            "LC_ALL",
            "WAYLAND_DISPLAY",
            "HTTPS_PROXY",
        ] {
            assert!(allowed_env(name), "variável necessária removida: {name}");
        }
    }

    #[test]
    fn arguments_force_official_provider_and_subscription_login() {
        assert!(
            CODEX_ARGS
                .windows(2)
                .any(|args| args == ["-c", "model_provider=\"openai\""])
        );
        assert!(
            CODEX_ARGS
                .windows(2)
                .any(|args| args == ["-c", "forced_login_method=\"chatgpt\""])
        );
        assert!(
            CODEX_ARGS
                .windows(2)
                .any(|args| args == ["--listen", "stdio://"])
        );
    }

    #[test]
    fn envelopes_keep_rpc_direction_ids_and_nonblocking_backpressure() {
        let (outgoing, incoming) = mpsc::sync_channel(4);
        let (_events_tx, events) = mpsc::channel();
        let runtime = Runtime {
            events,
            outgoing,
            next_id: AtomicU64::new(1),
            closed: Arc::new(AtomicBool::new(false)),
            process: Arc::new(Process {
                child: Mutex::new(None),
            }),
        };
        assert_eq!(runtime.request("account/read", json!({})).unwrap(), 1);
        assert_eq!(runtime.request("model/list", json!({})).unwrap(), 2);
        runtime.notify("initialized", json!({})).unwrap();
        runtime
            .respond(json!("server-request"), json!({"decision":"decline"}))
            .unwrap();
        assert!(runtime.notify("overflow", json!({})).is_err());
        let messages: Vec<Value> = (0..4)
            .map(|_| serde_json::from_slice(&incoming.try_recv().unwrap()).unwrap())
            .collect();
        assert_eq!(messages[0]["id"], 1);
        assert_eq!(messages[1]["id"], 2);
        assert!(messages[2].get("id").is_none());
        assert_eq!(messages[3]["id"], "server-request");
        assert_eq!(messages[3]["result"]["decision"], "decline");
        assert!(
            messages
                .iter()
                .all(|message| message.get("jsonrpc").is_none())
        );
    }
}
