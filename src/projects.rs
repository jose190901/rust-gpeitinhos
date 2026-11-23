//! Projetos locais: cria pastas novas ou registra diretórios existentes.
//! Nenhuma operação inicia Git, altera arquivos existentes ou grava preferências.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub path: String,
}

/// Um nome de pasta, sem componentes de caminho ou caracteres de controle.
pub fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || matches!(name, "." | "..") {
        return Err(
            "Digite um nome para o projeto sem usar os nomes reservados \".\" e \"..\".".into(),
        );
    }
    if name.contains(['/', '\\']) || name.chars().any(char::is_control) {
        return Err(
            "O nome deve ser uma única pasta, sem barras ou caracteres de controle.".into(),
        );
    }
    Ok(name.to_owned())
}

fn canonical_directory(path: &str) -> Result<std::path::PathBuf, String> {
    let directory = fs::canonicalize(path)
        .map_err(|error| format!("Não foi possível localizar a pasta: {error}"))?;
    if !directory.is_dir() {
        return Err("O caminho escolhido precisa ser uma pasta existente.".into());
    }
    if directory.to_str().is_none() {
        return Err("O caminho da pasta precisa estar em UTF-8.".into());
    }
    Ok(directory)
}

/// Cria somente o diretório solicitado, diretamente dentro do pai existente.
/// `create_dir` recusa também arquivos e links já existentes nesse nome.
pub fn create_project(parent: &str, name: &str) -> Result<Project, String> {
    let name = validate_name(name)?;
    let parent = canonical_directory(parent)?;
    let directory = parent.join(&name);
    fs::create_dir(&directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            "Já existe um arquivo ou pasta com esse nome. Escolha outro nome.".to_owned()
        } else {
            format!("Não foi possível criar a pasta do projeto: {error}")
        }
    })?;
    // O pai foi validado como UTF-8 e o nome vem de &str, antes de criar a pasta.
    let path = directory
        .to_str()
        .expect("pai e nome do projeto em UTF-8")
        .to_owned();
    Ok(Project { name, path })
}

/// Resolve links simbólicos para que o aplicativo possa deduplicar pela pasta.
pub fn existing_project(path: &str, name: Option<&str>) -> Result<Project, String> {
    let directory = canonical_directory(path)?;
    let name = match name {
        Some(name) => validate_name(name)?,
        None => directory
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("Raiz")
            .to_owned(),
    };
    Ok(Project {
        name,
        path: directory
            .to_str()
            .expect("pasta existente validada como UTF-8")
            .to_owned(),
    })
}

#[derive(Debug, Clone)]
pub enum ProjectOperation {
    Create { parent: String, name: String },
    AddExisting { path: String },
}

pub struct ProjectWorker {
    pending: Option<Receiver<Result<Project, String>>>,
}

impl Default for ProjectWorker {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectWorker {
    pub fn new() -> Self {
        Self { pending: None }
    }

    /// Submete no máximo uma operação até que seu resultado seja retirado.
    pub fn submit(&mut self, operation: ProjectOperation) -> Result<(), String> {
        if self.is_busy() {
            return Err("Aguarde a operação de projeto atual terminar.".into());
        }
        let (reply, result) = mpsc::channel();
        thread::Builder::new()
            .name("rustgpeitinhos-project".into())
            .spawn(move || {
                let result = match operation {
                    ProjectOperation::Create { parent, name } => create_project(&parent, &name),
                    ProjectOperation::AddExisting { path } => existing_project(&path, None),
                };
                let _ = reply.send(result);
            })
            .map_err(|error| format!("Não foi possível iniciar a operação de projeto: {error}"))?;
        self.pending = Some(result);
        Ok(())
    }

    /// Não bloqueia a interface. Retirar o resultado libera uma nova operação.
    pub fn poll(&mut self) -> Option<Result<Project, String>> {
        match self.pending.as_ref()?.try_recv() {
            Ok(result) => {
                self.pending = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                Some(Err(
                    "A operação de projeto encerrou sem retornar um resultado.".into(),
                ))
            }
        }
    }

    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "rustgpeitinhos-projects-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            Self(directory)
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn names_are_trimmed_and_cannot_traverse_or_create_nested_directories() {
        assert_eq!(validate_name("  Projeto Ágil  ").unwrap(), "Projeto Ágil");
        assert_eq!(validate_name("release..teste").unwrap(), "release..teste");
        for invalid in [
            "",
            " ",
            ".",
            "..",
            "../fora",
            "/absoluto",
            "a/b",
            "a\\b",
            "a\0b",
            "a\nb",
        ] {
            assert!(validate_name(invalid).is_err(), "{invalid:?}");
        }
        let fixture = Fixture::new();
        assert!(create_project(fixture.path(), "../fora").is_err());
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
    }

    #[test]
    fn creation_is_one_new_directory_and_never_overwrites_existing_contents() {
        let fixture = Fixture::new();
        let project = create_project(fixture.path(), "  Meu projeto  ").unwrap();
        assert_eq!(project.name, "Meu projeto");
        assert_eq!(Path::new(&project.path).parent(), Some(fixture.0.as_path()));
        assert!(Path::new(&project.path).is_dir());
        assert_eq!(fs::read_dir(&project.path).unwrap().count(), 0);
        let original = Path::new(&project.path).join("original.txt");
        fs::write(&original, "preservar").unwrap();
        assert!(create_project(fixture.path(), "Meu projeto").is_err());
        assert_eq!(fs::read_to_string(original).unwrap(), "preservar");
        fs::write(fixture.0.join("arquivo"), "preservar arquivo").unwrap();
        assert!(create_project(fixture.path(), "arquivo").is_err());
        assert_eq!(
            fs::read_to_string(fixture.0.join("arquivo")).unwrap(),
            "preservar arquivo"
        );
    }

    #[test]
    fn existing_project_uses_canonical_directory_and_optional_name() {
        let fixture = Fixture::new();
        let directory = fixture.0.join("Existente");
        fs::create_dir(&directory).unwrap();
        let path = directory.join(".");
        let project = existing_project(path.to_str().unwrap(), None).unwrap();
        assert_eq!(project.name, "Existente");
        assert_eq!(
            Path::new(&project.path),
            fs::canonicalize(&directory).unwrap()
        );
        let named = existing_project(path.to_str().unwrap(), Some("  Nome local  ")).unwrap();
        assert_eq!(named.name, "Nome local");
        assert_eq!(named.path, project.path);
        assert!(existing_project(path.to_str().unwrap(), Some("../fora")).is_err());
        fs::write(fixture.0.join("arquivo"), "conteúdo").unwrap();
        assert!(existing_project(fixture.0.join("arquivo").to_str().unwrap(), None).is_err());
        assert!(create_project(fixture.0.join("arquivo").to_str().unwrap(), "filho").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_parent_and_existing_alias_resolve_to_the_real_directory() {
        use std::os::unix::fs::symlink;
        let fixture = Fixture::new();
        let directory = fixture.0.join("real");
        fs::create_dir(&directory).unwrap();
        let alias = fixture.0.join("atalho");
        symlink(&directory, &alias).unwrap();
        let project = create_project(alias.to_str().unwrap(), "Novo").unwrap();
        assert_eq!(
            Path::new(&project.path),
            fs::canonicalize(directory.join("Novo")).unwrap()
        );
        assert_eq!(
            existing_project(alias.to_str().unwrap(), None).unwrap(),
            existing_project(directory.to_str().unwrap(), None).unwrap()
        );
        let existing_link = directory.join("já existe");
        symlink(fixture.0.join("ausente"), &existing_link).unwrap();
        assert!(create_project(directory.to_str().unwrap(), "já existe").is_err());
        assert!(
            fs::symlink_metadata(existing_link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    fn await_result(worker: &mut ProjectWorker) -> Result<Project, String> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = worker.poll() {
                return result;
            }
            assert!(
                Instant::now() < deadline,
                "operação de projeto não terminou"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn worker_allows_one_pending_operation_and_recovers_after_an_error() {
        let fixture = Fixture::new();
        let mut worker = ProjectWorker::new();
        assert!(!worker.is_busy());
        assert!(worker.poll().is_none());
        worker
            .submit(ProjectOperation::Create {
                parent: fixture.path().into(),
                name: "../recusado".into(),
            })
            .unwrap();
        assert!(worker.is_busy());
        assert!(
            worker
                .submit(ProjectOperation::AddExisting {
                    path: fixture.path().into()
                })
                .is_err()
        );
        assert!(await_result(&mut worker).is_err());
        assert!(!worker.is_busy());
        worker
            .submit(ProjectOperation::Create {
                parent: fixture.path().into(),
                name: "Novo".into(),
            })
            .unwrap();
        let created = await_result(&mut worker).unwrap();
        assert!(!worker.is_busy());
        worker
            .submit(ProjectOperation::AddExisting {
                path: created.path.clone(),
            })
            .unwrap();
        assert_eq!(await_result(&mut worker).unwrap(), created);
    }
}
