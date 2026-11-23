//! Revisão Git assíncrona; comandos são executados pelo sandbox do motor oficial.
use serde_json::Value;
use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(test)]
use std::{
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
};

const OUTPUT_LIMIT: usize = 1024 * 1024;
const PREVIEW_LIMIT: usize = 128 * 1024;
#[cfg(test)]
const COMMAND_TIMEOUT: Duration = Duration::from_secs(8);

/// O aplicativo deve entregar este argv ao `command/exec`, em sandbox readOnly,
/// rede desativada, timeout de 8s e limite de saída de 1 MiB. Nunca usar shell.
pub struct ReviewCommand {
    pub command: Vec<String>,
    pub cwd: String,
    pub env: Value,
    pub reply: mpsc::Sender<Result<Vec<u8>, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    /// Caminho relativo à pasta escolhida, sem escapar dessa pasta.
    pub path: String,
    /// Duas colunas XY do status porcelain: índice e diretório de trabalho.
    pub status: String,
}

pub struct WorkspaceReview {
    pub cwd: String,
    pub loading: bool,
    pub files: Vec<ChangedFile>,
    pub selected: Option<String>,
    pub diff: String,
    pub diff_loading: bool,
    pub error: Option<String>,
    pub repo_root: Option<String>,
    pub truncated: bool,
    generation: Arc<AtomicU64>,
    selection_generation: Arc<AtomicU64>,
    active: Arc<AtomicUsize>,
    tx: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
    command_tx: mpsc::Sender<ReviewCommand>,
    command_rx: mpsc::Receiver<ReviewCommand>,
    pending: Option<Task>,
    scope: Option<Scope>,
}

#[derive(Clone)]
struct Scope {
    cwd: PathBuf,
    root: PathBuf,
}

enum Task {
    Refresh {
        generation: u64,
        cwd: String,
    },
    Diff {
        generation: u64,
        selection: u64,
        scope: Scope,
        file: ChangedFile,
    },
}

enum Event {
    Refreshed {
        generation: u64,
        result: Result<(Scope, Vec<ChangedFile>), String>,
    },
    Diffed {
        generation: u64,
        selection: u64,
        result: Result<(String, bool), String>,
    },
}

struct Cancellation {
    generation: Arc<AtomicU64>,
    expected: u64,
    selection: Option<(Arc<AtomicU64>, u64)>,
    bridge: Option<mpsc::Sender<ReviewCommand>>,
}
impl Cancellation {
    fn cancelled(&self) -> bool {
        self.generation.load(Ordering::Acquire) != self.expected
            || self
                .selection
                .as_ref()
                .is_some_and(|(current, expected)| current.load(Ordering::Acquire) != *expected)
    }
    fn check(&self) -> Result<(), String> {
        if self.cancelled() {
            Err("Revisão cancelada.".into())
        } else {
            Ok(())
        }
    }
}

impl Default for WorkspaceReview {
    fn default() -> Self {
        Self::new()
    }
}
impl WorkspaceReview {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (command_tx, command_rx) = mpsc::channel();
        Self {
            cwd: String::new(),
            loading: false,
            files: vec![],
            selected: None,
            diff: String::new(),
            diff_loading: false,
            error: None,
            repo_root: None,
            truncated: false,
            generation: Arc::new(AtomicU64::new(0)),
            selection_generation: Arc::new(AtomicU64::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            tx,
            rx,
            command_tx,
            command_rx,
            pending: None,
            scope: None,
        }
    }

    pub fn refresh(&mut self, cwd: &str) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.selection_generation.fetch_add(1, Ordering::AcqRel);
        self.cwd = cwd.into();
        self.loading = true;
        self.files.clear();
        self.selected = None;
        self.diff.clear();
        self.diff_loading = false;
        self.error = None;
        self.repo_root = None;
        self.scope = None;
        self.truncated = false;
        self.pending = Some(Task::Refresh {
            generation,
            cwd: cwd.into(),
        });
        self.start_pending();
    }

    pub fn select(&mut self, path: &str) {
        let Some(file) = self.files.iter().find(|f| f.path == path).cloned() else {
            self.error = Some("O arquivo não pertence à revisão atual. Atualize a lista.".into());
            return;
        };
        let Some(scope) = self.scope.clone() else {
            return;
        };
        let selection = self.selection_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.selected = Some(path.into());
        self.diff.clear();
        self.error = None;
        self.diff_loading = true;
        self.truncated = false;
        self.pending = Some(Task::Diff {
            generation: self.generation.load(Ordering::Acquire),
            selection,
            scope,
            file,
        });
        self.start_pending();
    }

    /// Deve ser chamado a cada frame; nunca espera subprocessos.
    pub fn pump(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Refreshed { generation, result }
                    if generation == self.generation.load(Ordering::Acquire) =>
                {
                    self.loading = false;
                    match result {
                        Ok((scope, files)) => {
                            self.repo_root = Some(scope.root.to_string_lossy().into_owned());
                            self.scope = Some(scope);
                            self.files = files;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Event::Diffed {
                    generation,
                    selection,
                    result,
                } if generation == self.generation.load(Ordering::Acquire)
                    && selection == self.selection_generation.load(Ordering::Acquire) =>
                {
                    self.diff_loading = false;
                    match result {
                        Ok((diff, truncated)) => {
                            self.diff = diff;
                            self.truncated = truncated;
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                _ => {}
            }
        }
        self.start_pending();
    }

    /// Retira pedidos pendentes sem bloquear. Toda execução fica com o app-server.
    pub fn take_commands(&mut self) -> Vec<ReviewCommand> {
        self.command_rx.try_iter().collect()
    }

    fn start_pending(&mut self) {
        if self.pending.is_none() {
            return;
        }
        let mut active = self.active.load(Ordering::Acquire);
        loop {
            if active >= 2 {
                return;
            }
            match self.active.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => active = actual,
            }
        }
        let task = self.pending.take().expect("tarefa pendente");
        let tx = self.tx.clone();
        let active = self.active.clone();
        let generation = self.generation.clone();
        let selection_generation = self.selection_generation.clone();
        let bridge = self.command_tx.clone();
        thread::spawn(move || {
            struct Release(Arc<AtomicUsize>);
            impl Drop for Release {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _release = Release(active);
            let event = match task {
                Task::Refresh {
                    generation: expected,
                    cwd,
                } => {
                    let cancel = Cancellation {
                        generation,
                        expected,
                        selection: None,
                        bridge: Some(bridge),
                    };
                    Event::Refreshed {
                        generation: expected,
                        result: load_status(&cwd, &cancel),
                    }
                }
                Task::Diff {
                    generation: expected,
                    selection,
                    scope,
                    file,
                } => {
                    let cancel = Cancellation {
                        generation,
                        expected,
                        selection: Some((selection_generation, selection)),
                        bridge: Some(bridge),
                    };
                    Event::Diffed {
                        generation: expected,
                        selection,
                        result: load_diff(&scope, &file, &cancel),
                    }
                }
            };
            let _ = tx.send(event);
        });
    }
}
impl Drop for WorkspaceReview {
    fn drop(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

fn valid_relative(path: &str) -> Result<&Path, String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("O caminho do arquivo não pode sair da pasta escolhida.".into());
    }
    Ok(path)
}

fn load_status(cwd: &str, cancel: &Cancellation) -> Result<(Scope, Vec<ChangedFile>), String> {
    cancel.check()?;
    let cwd =
        fs::canonicalize(cwd).map_err(|_| "A pasta escolhida não está disponível.".to_string())?;
    if !cwd.is_dir() {
        return Err("Escolha uma pasta para revisar.".into());
    }
    let root_result = git(&cwd, &["rev-parse", "--show-toplevel"], cancel)?;
    // rev-parse termina com uma quebra de linha; não remove espaços do caminho.
    let root_text = std::str::from_utf8(&root_result)
        .map_err(|_| "O caminho do repositório precisa estar em UTF-8.".to_string())?;
    let root = PathBuf::from(root_text.strip_suffix('\n').unwrap_or(root_text));
    let root = fs::canonicalize(root)
        .map_err(|_| "Não foi possível localizar o repositório Git.".to_string())?;
    let prefix = cwd
        .strip_prefix(&root)
        .map_err(|_| "A pasta escolhida está fora do repositório.".to_string())?;
    let status = git(
        &cwd,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ],
        cancel,
    )?;
    let files = parse_status(&status, prefix)?;
    Ok((Scope { cwd, root }, files))
}

fn parse_status(bytes: &[u8], prefix: &Path) -> Result<Vec<ChangedFile>, String> {
    let mut records = bytes.split(|b| *b == 0).filter(|record| !record.is_empty());
    let mut files = vec![];
    while let Some(record) = records.next() {
        if record.len() < 4 || record[2] != b' ' {
            return Err("O Git retornou um status inválido.".into());
        }
        let renamed = record[..2].iter().any(|b| matches!(b, b'R' | b'C'));
        let path = std::str::from_utf8(&record[3..]).map_err(|_| {
            "Um arquivo tem nome fora de UTF-8; a revisão não pode representá-lo com segurança."
                .to_string()
        })?;
        valid_relative(path)?;
        if renamed && records.next().is_none() {
            return Err("O Git retornou um renome incompleto.".into());
        }
        // Porcelain usa caminhos relativos à raiz mesmo executado numa subpasta.
        let Ok(local) = Path::new(path).strip_prefix(prefix) else {
            continue;
        };
        let local = local.to_str().ok_or("Caminho de arquivo inválido.")?;
        valid_relative(local)?;
        files.push(ChangedFile {
            path: local.into(),
            status: String::from_utf8_lossy(&record[..2]).into_owned(),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn load_diff(
    scope: &Scope,
    file: &ChangedFile,
    cancel: &Cancellation,
) -> Result<(String, bool), String> {
    cancel.check()?;
    let local = valid_relative(&file.path)?;
    if file.status == "??" {
        return preview_untracked(&scope.cwd, local, cancel);
    }
    let full = scope.cwd.join(local);
    let relative = full
        .strip_prefix(&scope.root)
        .map_err(|_| "Arquivo fora da pasta escolhida.".to_string())?;
    let path = relative.to_str().ok_or("Caminho fora de UTF-8.")?;
    let staged = git(
        &scope.root,
        &[
            "diff",
            "--cached",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--",
            path,
        ],
        cancel,
    )?;
    let working = git(
        &scope.root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--",
            path,
        ],
        cancel,
    )?;
    let mut text = String::new();
    if !staged.is_empty() {
        text.push_str("ALTERAÇÕES PREPARADAS (STAGED)\n\n");
        text.push_str(&String::from_utf8_lossy(&staged));
    }
    if !working.is_empty() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str("ALTERAÇÕES NA PASTA (UNSTAGED)\n\n");
        text.push_str(&String::from_utf8_lossy(&working));
    }
    if text.is_empty() {
        text = "Não há diff textual disponível para este arquivo. Atualize a revisão se ele mudou."
            .into();
    }
    Ok((text, false))
}

fn preview_untracked(
    cwd: &Path,
    relative: &Path,
    cancel: &Cancellation,
) -> Result<(String, bool), String> {
    let mut current = cwd.to_path_buf();
    for part in relative.components() {
        current.push(part);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| "O arquivo não está mais disponível.".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("A prévia não segue links simbólicos.".into());
        }
    }
    if !current.is_file() {
        return Err("A prévia aceita somente arquivos regulares.".into());
    }
    let canonical = fs::canonicalize(&current)
        .map_err(|_| "Não foi possível localizar o arquivo.".to_string())?;
    if !canonical.starts_with(cwd) {
        return Err("Arquivo fora da pasta escolhida.".into());
    }
    let mut options = File::options();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NOFOLLOW | O_NONBLOCK: recusa troca por symlink e não bloqueia em FIFO.
        options.custom_flags(0x20000 | 0x800);
    }
    let file = options
        .open(&canonical)
        .map_err(|_| "Não foi possível abrir o arquivo para prévia.".to_string())?;
    if !file
        .metadata()
        .map_err(|_| "Não foi possível verificar o arquivo.")?
        .is_file()
    {
        return Err("A prévia aceita somente arquivos regulares.".into());
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        // Confere o descritor antes da leitura, inclusive se um diretório foi trocado.
        let opened = fs::canonicalize(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|_| "Não foi possível confirmar o arquivo aberto.".to_string())?;
        if !opened.starts_with(cwd) {
            return Err("Arquivo fora da pasta escolhida.".into());
        }
    }
    cancel.check()?;
    let mut bytes = Vec::new();
    file.take((PREVIEW_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "Não foi possível ler a prévia.".to_string())?;
    cancel.check()?;
    let truncated = bytes.len() > PREVIEW_LIMIT;
    bytes.truncate(PREVIEW_LIMIT);
    if bytes.contains(&0) {
        return Ok((
            "ARQUIVO NOVO (UNTRACKED)\n\nArquivo binário; prévia textual indisponível.".into(),
            truncated,
        ));
    }
    let mut text = format!(
        "ARQUIVO NOVO (UNTRACKED)\n\n{}",
        String::from_utf8_lossy(&bytes)
    );
    if truncated {
        text.push_str("\n\n[Prévia limitada a 128 KiB.]");
    }
    Ok((text, truncated))
}

fn git(cwd: &Path, args: &[&str], cancel: &Cancellation) -> Result<Vec<u8>, String> {
    cancel.check()?;
    let Some(bridge) = cancel.bridge.as_ref() else {
        #[cfg(test)]
        {
            return git_fixture(cwd, args, cancel);
        }
        #[cfg(not(test))]
        {
            return Err("O executor protegido da revisão não está disponível.".into());
        }
    };
    let mut command: Vec<String> = [
        "git",
        "--no-pager",
        "--literal-pathspecs",
        "-c",
        "core.quotepath=false",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.hooksPath=/dev/null",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    command.extend(args.iter().map(|s| (*s).to_owned()));
    let mut env = serde_json::Map::new();
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            env.insert(name.to_string_lossy().into_owned(), Value::Null);
        }
    }
    for (name, value) in [
        ("GIT_OPTIONAL_LOCKS", "0"),
        ("GIT_NO_LAZY_FETCH", "1"),
        ("GIT_TERMINAL_PROMPT", "0"),
        ("LC_ALL", "C"),
    ] {
        env.insert(name.into(), Value::String(value.into()));
    }
    let (reply, response) = mpsc::channel();
    bridge
        .send(ReviewCommand {
            command,
            cwd: cwd
                .to_str()
                .ok_or("O caminho da pasta precisa estar em UTF-8.")?
                .into(),
            env: Value::Object(env),
            reply,
        })
        .map_err(|_| "O executor protegido da revisão foi encerrado.".to_string())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cancel.check()?;
        if Instant::now() >= deadline {
            return Err(
                "A revisão não recebeu resposta do executor protegido em dez segundos.".into(),
            );
        }
        match response.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => {
                let bytes = result?;
                if bytes.len() > OUTPUT_LIMIT {
                    return Err("A saída Git excedeu 1 MiB. Revise uma subpasta menor.".into());
                }
                return Ok(bytes);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("O executor protegido da revisão foi desconectado.".into());
            }
        }
    }
}

#[cfg(test)]
fn reader(
    mut pipe: impl Read + Send + 'static,
    exceeded: Arc<AtomicBool>,
) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buffer = [0; 8192];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let remaining = OUTPUT_LIMIT.saturating_sub(bytes.len());
                    bytes.extend_from_slice(&buffer[..count.min(remaining)]);
                    if count > remaining {
                        exceeded.store(true, Ordering::Release);
                    }
                }
            }
        }
        bytes
    })
}

#[cfg(test)]
fn terminate(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        if let Ok(pid) = i32::try_from(child.id()) {
            unsafe {
                kill(-pid, 9);
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Executor exclusivo das fixtures. Não existe em binários de produção.
#[cfg(test)]
fn git_fixture(cwd: &Path, args: &[&str], cancel: &Cancellation) -> Result<Vec<u8>, String> {
    cancel.check()?;
    let mut command = Command::new("git");
    command
        .args([
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "core.quotepath=false",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Variáveis externas não podem redirecionar Git para outro índice/repositório.
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|_| {
        "Não foi possível iniciar o Git. Verifique se ele está instalado.".to_string()
    })?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let out = reader(child.stdout.take().expect("stdout pipe"), exceeded.clone());
    let err = reader(child.stderr.take().expect("stderr pipe"), exceeded.clone());
    let start = Instant::now();
    let result: Result<std::process::ExitStatus, String> = loop {
        if cancel.cancelled() {
            break Err("Revisão cancelada.".into());
        }
        if exceeded.load(Ordering::Acquire) {
            break Err("A saída Git excedeu 1 MiB. Revise uma subpasta menor.".into());
        }
        if start.elapsed() > COMMAND_TIMEOUT {
            break Err("O Git não respondeu em oito segundos. Tente atualizar a revisão.".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(15)),
            Err(_) => break Err("Não foi possível consultar o processo Git.".into()),
        }
    };
    terminate(&mut child);
    let bytes = out.join().unwrap_or_default();
    let errors = err.join().unwrap_or_default();
    let status = result?;
    if exceeded.load(Ordering::Acquire) {
        return Err("A saída Git excedeu 1 MiB. Revise uma subpasta menor.".into());
    }
    if !status.success() {
        let message = String::from_utf8_lossy(&errors);
        if message.contains("not a git repository") {
            return Err("Esta pasta não pertence a um repositório Git.".into());
        }
        return Err(format!(
            "O Git não conseguiu concluir a revisão: {}",
            message.chars().take(1000).collect::<String>().trim()
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    static FIXTURE_ID: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "rustgpeitinhos-review-{}-{}",
                std::process::id(),
                FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&root).unwrap();
            let fixture = Self(root);
            fixture.git(&["init", "-q"]);
            fixture
        }
        fn git(&self, args: &[&str]) {
            let mut command = Command::new("git");
            command
                .args([
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .current_dir(&self.0);
            for (name, _) in std::env::vars_os() {
                if name.to_string_lossy().starts_with("GIT_") {
                    command.env_remove(name);
                }
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
        fn commit(&self) {
            self.git(&["add", "."]);
            self.git(&["commit", "-qm", "fixture"]);
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn cancel() -> Cancellation {
        Cancellation {
            generation: Arc::new(AtomicU64::new(0)),
            expected: 0,
            selection: None,
            bridge: None,
        }
    }
    fn pump_until(review: &mut WorkspaceReview) {
        let start = Instant::now();
        while review.loading || review.diff_loading {
            for request in review.take_commands() {
                assert_eq!(request.command[0], "git");
                let args: Vec<&str> = request.command[1..].iter().map(String::as_str).collect();
                let result = git_fixture(Path::new(&request.cwd), &args, &cancel());
                let _ = request.reply.send(result);
            }
            review.pump();
            assert!(start.elapsed() < Duration::from_secs(5), "timeout");
            thread::sleep(Duration::from_millis(10));
        }
        assert!(review.error.is_none(), "{:?}", review.error);
    }

    #[test]
    fn status_and_diff_keep_spaces_renames_and_both_git_layers() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("arquivo com espaço.txt"), "original\n").unwrap();
        fs::write(fixture.0.join("antes.txt"), "conteúdo\n").unwrap();
        fixture.commit();
        fixture.git(&["mv", "antes.txt", "depois nome.txt"]);
        fs::write(fixture.0.join("arquivo com espaço.txt"), "preparado\n").unwrap();
        fixture.git(&["add", "arquivo com espaço.txt"]);
        fs::write(fixture.0.join("arquivo com espaço.txt"), "na pasta\n").unwrap();
        fs::write(fixture.0.join("novo.txt"), "novo\n").unwrap();
        let mut review = WorkspaceReview::new();
        review.refresh(fixture.path());
        pump_until(&mut review);
        assert!(review.files.contains(&ChangedFile {
            path: "depois nome.txt".into(),
            status: "R ".into()
        }));
        assert!(review.files.contains(&ChangedFile {
            path: "arquivo com espaço.txt".into(),
            status: "MM".into()
        }));
        assert!(review.files.contains(&ChangedFile {
            path: "novo.txt".into(),
            status: "??".into()
        }));
        assert_eq!(review.files.len(), 3);
        review.select("arquivo com espaço.txt");
        pump_until(&mut review);
        assert!(review.diff.contains("STAGED") && review.diff.contains("UNSTAGED"));
        assert!(review.diff.contains("+preparado") && review.diff.contains("+na pasta"));
    }

    #[test]
    fn subfolder_scope_and_literal_pathspec_do_not_include_neighbors() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("pasta")).unwrap();
        for name in ["pasta/wild[a].txt", "pasta/wilda.txt", "vizinho.txt"] {
            fs::write(fixture.0.join(name), "base\n").unwrap();
        }
        fixture.commit();
        for name in ["pasta/wild[a].txt", "pasta/wilda.txt", "vizinho.txt"] {
            fs::write(fixture.0.join(name), format!("{name}\n")).unwrap();
        }
        let cwd = fixture.0.join("pasta");
        let (scope, files) = load_status(cwd.to_str().unwrap(), &cancel()).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| !f.path.contains("/")));
        let selected = files.iter().find(|f| f.path == "wild[a].txt").unwrap();
        let (diff, _) = load_diff(&scope, selected, &cancel()).unwrap();
        assert!(diff.contains("+pasta/wild[a].txt"));
        assert!(!diff.contains("+pasta/wilda.txt"));
        assert!(!diff.contains("vizinho"));
    }

    #[test]
    fn untracked_preview_is_bounded_and_refuses_traversal_and_symlinks() {
        let fixture = Fixture::new();
        let cancellation = cancel();
        fs::write(fixture.0.join("grande.txt"), vec![b'x'; PREVIEW_LIMIT + 20]).unwrap();
        let (text, truncated) =
            preview_untracked(&fixture.0, Path::new("grande.txt"), &cancellation).unwrap();
        assert!(truncated);
        assert!(text.contains("128 KiB"));
        assert!(valid_relative("../fora.txt").is_err());
        assert!(valid_relative("/etc/passwd").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("grande.txt", fixture.0.join("link.txt")).unwrap();
            assert!(
                preview_untracked(&fixture.0, Path::new("link.txt"), &cancellation)
                    .unwrap_err()
                    .contains("links simbólicos")
            );
        }
    }

    #[test]
    fn stale_folder_and_selection_results_cannot_replace_current_review() {
        let first = Fixture::new();
        let second = Fixture::new();
        fs::write(second.0.join("atual.txt"), "atual\n").unwrap();
        let mut review = WorkspaceReview::new();
        review.refresh(first.path());
        review.refresh(second.path());
        review
            .tx
            .send(Event::Refreshed {
                generation: 1,
                result: Err("antigo".into()),
            })
            .unwrap();
        pump_until(&mut review);
        assert_eq!(review.cwd, second.path());
        assert_eq!(review.files[0].path, "atual.txt");
        review.select("atual.txt");
        let generation = review.generation.load(Ordering::Acquire);
        review
            .tx
            .send(Event::Diffed {
                generation,
                selection: 0,
                result: Ok(("antigo".into(), false)),
            })
            .unwrap();
        pump_until(&mut review);
        assert!(review.diff.contains("atual"));
        assert!(!review.diff.contains("antigo"));
        review.select("../fora");
        assert!(review.error.is_some());
    }

    #[test]
    fn porcelain_zero_records_keep_newlines_and_skip_rename_source() {
        let files = parse_status(
            b"R  novo nome\0nome antigo\0?? linha\nquebrada.txt\0",
            Path::new(""),
        )
        .unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|f| f.path == "linha\nquebrada.txt"));
        assert!(!files.iter().any(|f| f.path == "nome antigo"));
        assert!(parse_status(b"R  destino\0", Path::new("")).is_err());
    }

    #[test]
    fn git_review_requires_the_bridge_and_propagates_its_failure() {
        let fixture = Fixture::new();
        let (bridge, requests) = mpsc::channel();
        let cancellation = Cancellation {
            bridge: Some(bridge),
            ..cancel()
        };
        let cwd = fixture.0.clone();
        let handle = thread::spawn(move || {
            git(
                &cwd,
                &["status", "--porcelain=v1", "-z", "--", "."],
                &cancellation,
            )
        });
        let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(request.command[0], "git");
        assert_eq!(request.cwd, fixture.path());
        assert!(
            request
                .command
                .iter()
                .any(|arg| arg == "core.fsmonitor=false")
        );
        assert!(
            request
                .command
                .iter()
                .any(|arg| arg == "--literal-pathspecs")
        );
        assert_eq!(request.env["GIT_OPTIONAL_LOCKS"], "0");
        assert_eq!(request.env["GIT_NO_LAZY_FETCH"], "1");
        request
            .reply
            .send(Err("sandbox recusou a execução".into()))
            .unwrap();
        assert_eq!(
            handle.join().unwrap().unwrap_err(),
            "sandbox recusou a execução"
        );
    }

    #[test]
    fn cancelled_bridge_wait_does_not_need_a_subprocess_response() {
        let fixture = Fixture::new();
        let (bridge, requests) = mpsc::channel();
        let cancellation = Cancellation {
            bridge: Some(bridge),
            ..cancel()
        };
        let generation = cancellation.generation.clone();
        let cwd = fixture.0.clone();
        let handle = thread::spawn(move || git(&cwd, &["status"], &cancellation));
        let request = requests.recv_timeout(Duration::from_secs(1)).unwrap();
        generation.fetch_add(1, Ordering::AcqRel);
        assert!(handle.join().unwrap().unwrap_err().contains("cancelada"));
        drop(request);
    }
}
