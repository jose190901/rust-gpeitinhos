//! Terminal de comandos pelo sandbox oficial, sem criar conversas ou turnos.
use crate::view_model::AccessMode;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

const OUTPUT_LIMIT: usize = 256 * 1024;

#[derive(Debug)]
pub enum TerminalAction {
    Run,
    Stop,
    Input,
    CloseInput,
    Clear,
}

pub struct Terminal {
    pub command: String,
    pub input: String,
    pub output: String,
    pub running: bool,
    pub error: Option<String>,
    pub exit_code: Option<i64>,
    pub cwd: String,
    pub access: AccessMode,
    process: Option<String>,
    sequence: u64,
    stdout_tail: Vec<u8>,
    stderr_tail: Vec<u8>,
    truncated: bool,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            command: String::new(),
            input: String::new(),
            output: String::new(),
            running: false,
            error: None,
            exit_code: None,
            cwd: String::new(),
            access: AccessMode::Folder,
            process: None,
            sequence: 0,
            stdout_tail: vec![],
            stderr_tail: vec![],
            truncated: false,
        }
    }
}

pub fn sandbox_policy(access: AccessMode, cwd: &str) -> Value {
    match access {
        AccessMode::ReadOnly => json!({"type":"readOnly","networkAccess":false}),
        AccessMode::Folder => {
            json!({"type":"workspaceWrite","writableRoots":[cwd],"networkAccess":false,"excludeSlashTmp":true,"excludeTmpdirEnvVar":true})
        }
        AccessMode::Full => json!({"type":"dangerFullAccess"}),
    }
}

impl Terminal {
    pub fn start(&mut self, cwd: &str, access: AccessMode) -> Option<(String, Value)> {
        if self.running || self.command.trim().is_empty() {
            return None;
        }
        self.sequence = self.sequence.checked_add(1)?;
        let process = format!("rustgpeitinhos-terminal-{}-{}", std::process::id(), self.sequence);
        self.cwd = cwd.to_owned();
        self.access = access;
        self.running = true;
        self.error = None;
        self.exit_code = None;
        self.input.clear();
        self.stdout_tail.clear();
        self.stderr_tail.clear();
        self.truncated = false;
        self.append(&format!(
            "\n[{} · {}]\n$ {}\n",
            cwd,
            access.label(),
            self.command
        ));
        self.process = Some(process.clone());
        Some((
            process.clone(),
            json!({
                "command":["/bin/bash","--noprofile","--norc","-c",self.command],
                "cwd":cwd,"sandboxPolicy":sandbox_policy(access,cwd),
                "processId":process,"streamStdoutStderr":true,"streamStdin":true,
                "disableTimeout":true,"outputBytesCap":OUTPUT_LIMIT,
            "env":{"TERM":"dumb","NO_COLOR":"1","CLICOLOR":"0"},
            }),
        ))
    }

    pub fn process_id(&self) -> Option<&str> {
        self.process.as_deref()
    }

    pub fn write_params(&self, close: bool) -> Option<Value> {
        let process = self.process.as_ref().filter(|_| self.running)?;
        if close {
            Some(json!({"processId":process,"closeStdin":true}))
        } else {
            Some(
                json!({"processId":process,"deltaBase64":STANDARD.encode(format!("{}\n",self.input))}),
            )
        }
    }

    pub fn notification(&mut self, params: &Value) {
        if !self.running || params["processId"].as_str() != self.process_id() {
            return;
        }
        let Some(encoded) = params["deltaBase64"].as_str() else {
            return;
        };
        match STANDARD.decode(encoded) {
            Ok(bytes) => {
                let tail = if params["stream"] == "stderr" {
                    &mut self.stderr_tail
                } else {
                    &mut self.stdout_tail
                };
                tail.extend(bytes);
                let text = decode_available(tail, false);
                self.append(&text);
                if params["capReached"] == true && !self.truncated {
                    self.append("\n[Saída limitada pelo Codex.]\n");
                    self.truncated = true;
                }
            }
            Err(_) => self.error = Some("O Codex retornou saída de terminal inválida.".into()),
        }
    }

    pub fn finish(&mut self, process: &str, result: Result<&Value, &str>) {
        if self.process_id() != Some(process) {
            return;
        }
        let stdout_tail = decode_available(&mut self.stdout_tail, true);
        let stderr_tail = decode_available(&mut self.stderr_tail, true);
        self.append(&stdout_tail);
        self.append(&stderr_tail);
        match result {
            Ok(value) => {
                self.append(value["stdout"].as_str().unwrap_or(""));
                self.append(value["stderr"].as_str().unwrap_or(""));
                self.exit_code = value["exitCode"].as_i64();
            }
            Err(error) => self.error = Some(error.to_owned()),
        }
        self.running = false;
        self.process = None;
    }

    pub fn disconnected(&mut self) {
        if self.running {
            self.error =
                Some("A conexão encerrou. O comando foi interrompido; não será repetido.".into());
            self.running = false;
            self.process = None;
        }
    }

    pub fn clear(&mut self) {
        self.output.clear();
        self.error = None;
        self.exit_code = None;
        self.truncated = false;
    }

    fn append(&mut self, text: &str) {
        // Mantém a parte mais recente sem cortar um caractere UTF-8.
        self.output.extend(
            text.chars()
                .filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | '\r')),
        );
        if self.output.len() > OUTPUT_LIMIT {
            let mut start = self.output.len() - OUTPUT_LIMIT;
            while !self.output.is_char_boundary(start) {
                start += 1;
            }
            self.output.drain(..start);
        }
    }
}

fn decode_available(bytes: &mut Vec<u8>, flush: bool) -> String {
    let mut text = String::new();
    let mut offset = 0;
    loop {
        match std::str::from_utf8(&bytes[offset..]) {
            Ok(valid) => {
                text.push_str(valid);
                offset = bytes.len();
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                text.push_str(
                    std::str::from_utf8(&bytes[offset..offset + valid]).unwrap_or_default(),
                );
                offset += valid;
                match error.error_len() {
                    Some(invalid) => {
                        text.push('\u{fffd}');
                        offset += invalid;
                    }
                    None => {
                        if flush {
                            text.push('\u{fffd}');
                            offset = bytes.len();
                        }
                        break;
                    }
                }
            }
        }
    }
    bytes.drain(..offset);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_sandbox_and_command_are_preserved_without_a_model_turn() {
        let mut t = Terminal {
            command: "printf '%s' '$HOME'".into(),
            ..Default::default()
        };
        let (pid, p) = t.start("/project with spaces", AccessMode::Folder).unwrap();
        assert_eq!(p["command"][4], "printf '%s' '$HOME'");
        assert_eq!(
            p["sandboxPolicy"]["writableRoots"][0],
            "/project with spaces"
        );
        assert_eq!(p["sandboxPolicy"]["networkAccess"], false);
        assert!(t.start("/other", AccessMode::Full).is_none());
        assert_eq!(t.process_id(), Some(pid.as_str()));
    }
    #[test]
    fn streaming_preserves_utf8_across_chunks_and_ignores_other_processes() {
        let mut t = Terminal {
            command: "test".into(),
            ..Default::default()
        };
        let (pid, _) = t.start("/project", AccessMode::ReadOnly).unwrap();
        t.notification(
            &json!({"processId":"other","deltaBase64":STANDARD.encode("SECRET"),"stream":"stdout"}),
        );
        for bytes in [vec![0xc3], vec![0xa7, 0x0a]] {
            t.notification(&json!({"processId":pid,"deltaBase64":STANDARD.encode(bytes),"stream":"stdout","capReached":false}));
        }
        t.finish(&pid, Ok(&json!({"exitCode":0,"stdout":"","stderr":""})));
        assert!(t.output.ends_with("ç\n"));
        assert!(!t.output.contains("SECRET"));
        assert_eq!(t.exit_code, Some(0));
        assert!(!t.running);
    }
    #[test]
    fn stdin_uses_base64_and_repeated_runs_have_distinct_ids() {
        let mut t = Terminal {
            command: "cat".into(),
            ..Default::default()
        };
        let (first, _) = t.start("/project", AccessMode::ReadOnly).unwrap();
        t.input = "ação".into();
        let p = t.write_params(false).unwrap();
        assert_eq!(
            STANDARD.decode(p["deltaBase64"].as_str().unwrap()).unwrap(),
            "ação\n".as_bytes()
        );
        t.finish(&first, Err("interrompido"));
        let (next, _) = t.start("/project", AccessMode::ReadOnly).unwrap();
        assert_ne!(first, next);
        t.finish(&first, Ok(&json!({"exitCode":0})));
        assert!(t.running);
    }
    #[test]
    fn output_is_bounded_and_disconnect_never_retries() {
        let mut t = Terminal::default();
        t.append(&"á".repeat(OUTPUT_LIMIT));
        assert!(t.output.len() <= OUTPUT_LIMIT);
        assert!(t.output.is_char_boundary(0));
        t.command = "sleep 60".into();
        t.start("/project", AccessMode::ReadOnly).unwrap();
        t.disconnected();
        assert!(!t.running);
        assert!(t.error.is_some());
    }
    #[test]
    fn binary_output_is_decoded_without_repeatedly_shifting_the_buffer() {
        let mut bytes = vec![0xff; 256 * 1024];
        bytes.extend_from_slice("fim".as_bytes());
        let text = decode_available(&mut bytes, false);
        assert!(text.ends_with("fim"));
        assert!(bytes.is_empty());
    }
}
