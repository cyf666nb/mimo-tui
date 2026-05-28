/// LSP integration — code intelligence after file edits.
/// Supports rust-analyzer, pyright, gopls, typescript-language-server, clangd.
use anyhow::Result;
use std::path::Path;

/// Detect the language of a file by extension
pub fn detect_language(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "go" => Some("go"),
        "ts" | "tsx" => Some("typescript"),
        "js" | "jsx" => Some("javascript"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
        "java" => Some("java"),
        _ => None,
    }
}

/// Build the LSP server command for a language
#[allow(dead_code)]
pub fn lsp_command(lang: &str) -> Option<(&'static str, Vec<&'static str>)> {
    match lang {
        "rust" => Some(("rust-analyzer", vec![])),
        "python" => Some(("pyright-langserver", vec!["--stdio"])),
        "go" => Some(("gopls", vec![])),
        "typescript" | "javascript" => Some(("typescript-language-server", vec!["--stdio"])),
        "c" | "cpp" => Some(("clangd", vec![])),
        _ => None,
    }
}

/// Check if an LSP server is available for a language
#[allow(dead_code)]
pub fn lsp_available(lang: &str) -> bool {
    if let Some((cmd, _)) = lsp_command(lang) {
        which::which(cmd).is_ok()
    } else {
        false
    }
}

/// Run a quick diagnostic check on a file using the LSP server.
/// This is a simplified version — full LSP would use tower-lsp.
/// For now, we run the language's linter/checker directly.
pub fn quick_check(path: &Path) -> Result<Vec<Diagnostic>> {
    let lang = match detect_language(path) {
        Some(l) => l,
        None => return Ok(vec![]),
    };

    let mut diagnostics = Vec::new();

    match lang {
        "rust" => {
            // Use cargo check
            if let Some(parent) = path.parent() {
                let output = std::process::Command::new("cargo")
                    .args(["check", "--message-format=short"])
                    .current_dir(find_cargo_root(parent).unwrap_or(parent))
                    .output();

                if let Ok(out) = output {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    for line in stderr.lines() {
                        if line.contains("error") || line.contains("warning") {
                            diagnostics.push(Diagnostic {
                                file: path.to_string_lossy().to_string(),
                                line: 0,
                                severity: if line.contains("error") { Severity::Error } else { Severity::Warning },
                                message: line.to_string(),
                            });
                        }
                    }
                }
            }
        }
        "python" => {
            // Use pyright or ruff
            let output = std::process::Command::new("python3")
                .args(["-m", "py_compile", &path.to_string_lossy()])
                .output();

            if let Ok(out) = output {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    diagnostics.push(Diagnostic {
                        file: path.to_string_lossy().to_string(),
                        line: 0,
                        severity: Severity::Error,
                        message: stderr.trim().to_string(),
                    });
                }
            }
        }
        "go" => {
            let output = std::process::Command::new("go")
                .args(["vet", &path.to_string_lossy()])
                .output();

            if let Ok(out) = output {
                if !out.status.success() {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    diagnostics.push(Diagnostic {
                        file: path.to_string_lossy().to_string(),
                        line: 0,
                        severity: Severity::Error,
                        message: stderr.trim().to_string(),
                    });
                }
            }
        }
        _ => {}
    }

    Ok(diagnostics)
}

fn find_cargo_root(path: &Path) -> Option<&Path> {
    let mut current = path;
    loop {
        if current.join("Cargo.toml").exists() {
            return Some(current);
        }
        current = current.parent()?;
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum Severity {
    Error,
    Warning,
    #[allow(dead_code)] Info,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let icon = match self.severity {
            Severity::Error => "❌",
            Severity::Warning => "⚠️",
            Severity::Info => "ℹ️",
        };
        write!(f, "{} {}:{}: {}", icon, self.file, self.line, self.message)
    }
}
