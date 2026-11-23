//! Resolve o pacote em execução sem depender do caminho onde foi compilado.
use std::path::{Path, PathBuf};

const BUNDLED_CODEX: &str = "runtime/node_modules/.bin/codex";

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub data: PathBuf,
    pub codex: PathBuf,
}

pub fn discover() -> AppPaths {
    let executable = std::env::current_exe().ok();
    let override_binary = std::env::var_os("RUSTGPEITINHOS_CODEX_BIN")
        .or_else(|| std::env::var_os("PONTE_CODEX_BIN"))
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                std::env::current_dir().unwrap_or_default().join(path)
            }
        });
    resolve(
        executable.as_deref(),
        Path::new(env!("CARGO_MANIFEST_DIR")),
        override_binary,
    )
}

fn resolve(
    executable: Option<&Path>,
    manifest: &Path,
    override_binary: Option<PathBuf>,
) -> AppPaths {
    if let Some(directory) = executable.and_then(Path::parent) {
        let bundled = directory.join(BUNDLED_CODEX);
        // O marcador conserva o contexto do pacote mesmo se o runtime tiver
        // sido removido. Nesse caso spawn deve explicar o erro, sem recorrer
        // silenciosamente à instalação da máquina de desenvolvimento.
        if bundled.is_file() || directory.join(".rustgpeitinhos-package").is_file() {
            return AppPaths {
                root: directory.to_path_buf(),
                data: directory.join("data"),
                codex: override_binary.unwrap_or(bundled),
            };
        }
    }
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or(manifest)
        .to_path_buf();
    AppPaths {
        data: manifest.join("data"),
        codex: override_binary.unwrap_or_else(|| root.join("work/runtime/node_modules/.bin/codex")),
        root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "rustgpeitinhos-paths-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn moving_the_package_moves_runtime_and_preferences_with_it() {
        let directory = Directory::new();
        let original = directory.0.join("original");
        let bundled = original.join(BUNDLED_CODEX);
        std::fs::create_dir_all(bundled.parent().unwrap()).unwrap();
        std::fs::write(&bundled, "official-runtime-placeholder").unwrap();
        let manifest = Path::new("/unavailable/build/machine/outputs/RustGPeitinhos");
        let before = resolve(Some(&original.join("RustGPeitinhos")), manifest, None);
        assert_eq!(before.codex, bundled);
        let moved = directory.0.join("outro lugar");
        std::fs::rename(&original, &moved).unwrap();
        let after = resolve(Some(&moved.join("RustGPeitinhos")), manifest, None);
        assert_eq!(after.root, moved);
        assert_eq!(after.codex, moved.join(BUNDLED_CODEX));
        assert_eq!(after.data, moved.join("data"));
        assert!(!after.root.starts_with("/unavailable"));
    }

    #[test]
    fn missing_packaged_runtime_does_not_reuse_the_build_machine() {
        let directory = Directory::new();
        std::fs::write(directory.0.join(".rustgpeitinhos-package"), "1\n").unwrap();
        let paths = resolve(
            Some(&directory.0.join("RustGPeitinhos")),
            Path::new("/build/outputs/RustGPeitinhos"),
            None,
        );
        assert_eq!(paths.codex, directory.0.join(BUNDLED_CODEX));
        assert_eq!(paths.data, directory.0.join("data"));
    }

    #[test]
    fn development_layout_and_explicit_override_are_preserved() {
        let override_binary = PathBuf::from("/opt/custom/codex");
        let paths = resolve(
            None,
            Path::new("/project/outputs/RustGPeitinhos"),
            Some(override_binary.clone()),
        );
        assert_eq!(paths.root, Path::new("/project"));
        assert_eq!(paths.data, Path::new("/project/outputs/RustGPeitinhos/data"));
        assert_eq!(paths.codex, override_binary);
    }

    #[test]
    fn explicit_override_keeps_packaged_data_local() {
        let directory = Directory::new();
        std::fs::write(directory.0.join(".rustgpeitinhos-package"), "1\n").unwrap();
        let override_binary = PathBuf::from("/opt/custom/codex");
        let paths = resolve(
            Some(&directory.0.join("RustGPeitinhos")),
            Path::new("/build/outputs/RustGPeitinhos"),
            Some(override_binary.clone()),
        );
        assert_eq!(paths.data, directory.0.join("data"));
        assert_eq!(paths.codex, override_binary);
    }
}
