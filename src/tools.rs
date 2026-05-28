use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

/// Expand ~ to home directory
fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.join(path.strip_prefix("~/").unwrap_or("").strip_prefix("~").unwrap_or(""));
        }
    }
    PathBuf::from(path)
}

/// Safe string slice that won't panic on multi-byte boundaries
#[allow(dead_code)]
fn safe_truncate(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        return s;
    }
    // Find the nearest char boundary
    let mut end = max_chars;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Get all tool schemas for the MiMo API
pub fn get_tool_schemas(include_web_search: bool) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "type": "function",
            "function": {
                "name": "shell",
                "description": "Execute a shell command and return stdout+stderr. Use for running code, tests, git commands, installs.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "The shell command to execute"},
                        "workdir": {"type": "string", "description": "Working directory (optional)"}
                    },
                    "required": ["command"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file from the filesystem. Returns content with line numbers.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Absolute path to the file"},
                        "offset": {"type": "integer", "description": "Line number to start from (1-indexed)", "default": 1},
                        "limit": {"type": "integer", "description": "Max lines to read (default: 500)", "default": 500}
                    },
                    "required": ["path"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Write content to a file, creating it if needed or overwriting if it exists.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Absolute path to the file"},
                        "content": {"type": "string", "description": "Full content to write"}
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Perform exact string replacement in a file.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Absolute path to the file"},
                        "old_string": {"type": "string", "description": "Exact string to find"},
                        "new_string": {"type": "string", "description": "Replacement string"},
                        "replace_all": {"type": "boolean", "description": "Replace all occurrences", "default": false}
                    },
                    "required": ["path", "old_string", "new_string"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "grep",
                "description": "Search file contents using regex. Returns matching lines with file paths and line numbers.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string", "description": "Regex pattern to search for"},
                        "path": {"type": "string", "description": "Directory or file to search in", "default": "."},
                        "glob": {"type": "string", "description": "File glob filter (e.g. '*.py', '*.rs')"},
                        "max_results": {"type": "integer", "description": "Max results", "default": 50}
                    },
                    "required": ["pattern"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "List files and directories at a given path.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Directory path", "default": "."},
                        "show_hidden": {"type": "boolean", "description": "Include hidden files", "default": false}
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "project_index",
                "description": "Build a file index of the project. Shows all files with sizes. Use to understand project structure.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Project root directory", "default": "."},
                        "max_files": {"type": "integer", "description": "Max files to show", "default": 300}
                    }
                }
            }
        }),
    ];

    if include_web_search {
        tools.push(json!({
            "type": "web_search",
            "max_keyword": 3,
            "force_search": false,
            "limit": 5
        }));
    }

    tools
}

/// Execute a tool by name
pub fn execute_tool(name: &str, args: &Value) -> String {
    match name {
        "shell" => exec_shell(args),
        "read_file" => exec_read_file(args),
        "write_file" => exec_write_file(args),
        "edit_file" => exec_edit_file(args),
        "grep" => exec_grep(args),
        "list_dir" => exec_list_dir(args),
        "project_index" => exec_project_index(args),
        _ => format!("[ERROR] Unknown tool: {}", name),
    }
}

fn get_str<'a>(args: &'a Value, key: &str, default: &'a str) -> &'a str {
    args.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

fn get_u64(args: &Value, key: &str, default: u64) -> u64 {
    args.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

fn exec_shell(args: &Value) -> String {
    let command = get_str(args, "command", "");
    let workdir = get_str(args, "workdir", "");

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    if !workdir.is_empty() {
        cmd.current_dir(expand_path(workdir));
    }
    cmd.stdout(std::process::Stdio::piped())
       .stderr(std::process::Stdio::piped());

    // Use process::Child for proper kill on timeout
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return format!("[ERROR] Failed to spawn: {}", e),
    };

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);

    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return "[ERROR] Command timed out after 120s".into();
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return format!("[ERROR] Wait failed: {}", e),
        }
    }

    let out = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => return format!("[ERROR] {}", e),
    };

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut result = stdout.to_string();
    if !stderr.is_empty() {
        result.push_str(&format!("\n[stderr]\n{}", stderr));
    }
    if !out.status.success() {
        result.push_str(&format!("\n[exit code: {}]", out.status.code().unwrap_or(-1)));
    }
    if result.len() > 50000 {
        let boundary = result.char_indices().nth(50000).map(|(i, _)| i).unwrap_or(result.len());
        result.truncate(boundary);
        result.push_str("\n... (truncated)");
    }
    result
}

fn exec_read_file(args: &Value) -> String {
    let path = expand_path(get_str(args, "path", ""));
    let offset = get_u64(args, "offset", 1).max(1) as usize;
    let limit = get_u64(args, "limit", 500) as usize;

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            let start = offset.saturating_sub(1);
            let end = (start + limit).min(total);

            if start >= total {
                return format!("[ERROR] offset {} beyond file length {}", offset, total);
            }

            let mut result = format!("File: {} ({} lines total, showing {}-{})\n", path.display(), total, start + 1, end);
            for (i, line) in lines[start..end].iter().enumerate() {
                result.push_str(&format!("{:4} | {}\n", start + i + 1, line));
            }
            result
        }
        Err(e) => format!("[ERROR] {}", e),
    }
}

fn exec_write_file(args: &Value) -> String {
    let path = expand_path(get_str(args, "path", ""));
    let content = get_str(args, "content", "");

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match std::fs::write(&path, content) {
        Ok(_) => {
            let lines = content.lines().count();
            format!("Wrote {} lines to {}", lines, path.display())
        }
        Err(e) => format!("[ERROR] {}", e),
    }
}

fn exec_edit_file(args: &Value) -> String {
    let path = expand_path(get_str(args, "path", ""));
    let old_string = get_str(args, "old_string", "");
    let new_string = get_str(args, "new_string", "");
    let replace_all = args.get("replace_all").and_then(|v| v.as_bool()).unwrap_or(false);

    match std::fs::read_to_string(&path) {
        Ok(content) => {
            if !content.contains(old_string) {
                return format!("[ERROR] old_string not found in {}", path.display());
            }
            let count = content.matches(old_string).count();
            if !replace_all && count > 1 {
                return format!("[ERROR] old_string appears {} times. Use replace_all=true or provide more context.", count);
            }
            let new_content = if replace_all {
                content.replace(old_string, new_string)
            } else {
                content.replacen(old_string, new_string, 1)
            };
            match std::fs::write(&path, &new_content) {
                Ok(_) => format!("Replaced {} occurrence(s) in {}", if replace_all { count } else { 1 }, path.display()),
                Err(e) => format!("[ERROR] {}", e),
            }
        }
        Err(e) => format!("[ERROR] {}", e),
    }
}

fn exec_grep(args: &Value) -> String {
    let pattern = get_str(args, "pattern", "");
    let path = get_str(args, "path", ".");
    let glob_filter = get_str(args, "glob", "");
    let max_results = get_u64(args, "max_results", 50) as usize;

    let re = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return format!("[ERROR] Invalid regex: {}", e),
    };

    let root = expand_path(path);
    let skip_dirs: Vec<&str> = vec![".git", "node_modules", "__pycache__", ".venv", "venv", "target", "dist", "build"];
    let mut results: Vec<String> = Vec::new();

    let walker = walkdir::WalkDir::new(&root).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        !skip_dirs.iter().any(|s| name == *s)
    });

    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.metadata().map(|m| m.len() > 1_000_000).unwrap_or(true) {
            continue;
        }
        // Apply glob filter
        if !glob_filter.is_empty() {
            let file_name = entry.file_name().to_string_lossy();
            let glob_pattern = glob_filter.replace("*.", "");
            if !file_name.ends_with(&format!(".{}", glob_pattern)) && !file_name.ends_with(glob_filter) {
                continue;
            }
        }
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            for (i, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path());
                    results.push(format!("{}:{}: {}", rel.display(), i + 1, line.trim()));
                    if results.len() >= max_results {
                        break;
                    }
                }
            }
        }
        if results.len() >= max_results {
            break;
        }
    }

    if results.is_empty() {
        format!("No matches found for pattern: {}", pattern)
    } else {
        format!("Found {} matches:\n{}", results.len(), results.join("\n"))
    }
}

fn exec_list_dir(args: &Value) -> String {
    let path = get_str(args, "path", ".");
    let show_hidden = args.get("show_hidden").and_then(|v| v.as_bool()).unwrap_or(false);

    let root = expand_path(path);
    if !root.is_dir() {
        return format!("[ERROR] Not a directory: {}", path);
    }

    let mut entries: Vec<(String, bool, u64)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&root) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                continue;
            }
            let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            entries.push((name, is_dir, size));
        }
    }
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let mut result = format!("Directory: {}\n", root.display());
    for (name, is_dir, size) in entries.iter().take(200) {
        if *is_dir {
            result.push_str(&format!("  📁 {}/\n", name));
        } else {
            let size_str = if *size > 1_000_000 {
                format!("{:.1}M", *size as f64 / 1_000_000.0)
            } else if *size > 1_000 {
                format!("{:.1}K", *size as f64 / 1_000.0)
            } else {
                format!("{}B", size)
            };
            result.push_str(&format!("  📄 {} ({})\n", name, size_str));
        }
    }
    result
}

fn exec_project_index(args: &Value) -> String {
    let path = get_str(args, "path", ".");
    let max_files = get_u64(args, "max_files", 300) as usize;

    let root = expand_path(path);
    if !root.is_dir() {
        return format!("[ERROR] Not a directory: {}", path);
    }

    let skip_dirs: Vec<&str> = vec![".git", "node_modules", "__pycache__", ".venv", "venv", "target", "dist", "build", ".next", "coverage"];
    let skip_exts: Vec<&str> = vec![".pyc", ".pyo", ".so", ".dylib", ".dll", ".o", ".class", ".jar", ".lock"];
    let mut entries = Vec::new();

    for entry in walkdir::WalkDir::new(&root).into_iter().filter_entry(|e| {
        let name = e.file_name().to_string_lossy();
        !skip_dirs.iter().any(|s| name == *s)
    }).flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry.path().extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
        if skip_exts.iter().any(|s| ext.ends_with(s)) {
            continue;
        }
        if entry.metadata().map(|m| m.len() > 1_000_000).unwrap_or(true) {
            continue;
        }
        let rel = entry.path().strip_prefix(&root).unwrap_or(entry.path());
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let size_str = if size > 100_000 {
            format!("{}K", size / 1000)
        } else if size > 1000 {
            format!("{:.1}K", size as f64 / 1000.0)
        } else {
            format!("{}B", size)
        };
        entries.push(format!("  {} ({})", rel.display(), size_str));
        if entries.len() >= max_files {
            entries.push(format!("  ... ({} files shown)", max_files));
            break;
        }
    }

    format!("Project: {}\nFiles:\n{}", root.display(), entries.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_path_home() {
        let expanded = expand_path("~/test");
        let s = expanded.display().to_string();
        // Should not start with ~ (tilde expanded)
        assert!(!s.starts_with("~"), "Tilde not expanded: {}", s);
        // Should be an absolute path
        assert!(expanded.is_absolute(), "Should be absolute: {}", s);
    }

    #[test]
    fn test_expand_path_absolute() {
        let tmp = std::env::temp_dir().join("test");
        let expanded = expand_path(&tmp.display().to_string());
        assert_eq!(expanded, tmp);
    }

    #[test]
    fn test_expand_path_relative() {
        let expanded = expand_path("./test");
        assert!(expanded.display().to_string().ends_with("test"));
    }

    #[test]
    fn test_get_str_default() {
        let args = serde_json::json!({"key": "value"});
        assert_eq!(get_str(&args, "key", ""), "value");
        assert_eq!(get_str(&args, "missing", "default"), "default");
    }

    #[test]
    fn test_get_u64_default() {
        let args = serde_json::json!({"count": 42});
        assert_eq!(get_u64(&args, "count", 0), 42);
        assert_eq!(get_u64(&args, "missing", 10), 10);
    }

    #[test]
    fn test_execute_unknown_tool() {
        let args = serde_json::json!({});
        let result = execute_tool("nonexistent", &args);
        assert!(result.contains("ERROR"));
        assert!(result.contains("nonexistent"));
    }

    #[test]
    fn test_shell_echo() {
        let args = serde_json::json!({"command": "echo hello"});
        let result = execute_tool("shell", &args);
        assert!(result.contains("hello"));
    }

    #[test]
    fn test_read_file_basic() {
        // Create a temp file
        let path = std::env::temp_dir().join("mimo_test_read.txt");
        std::fs::write(&path, "line1\nline2\nline3").unwrap();
        let args = serde_json::json!({"path": path.display().to_string()});
        let result = execute_tool("read_file", &args);
        assert!(result.contains("line1"));
        assert!(result.contains("line2"));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_write_and_read() {
        let path = std::env::temp_dir().join("mimo_test_write.txt").display().to_string();
        let write_args = serde_json::json!({"path": path, "content": "test content"});
        let write_result = execute_tool("write_file", &write_args);
        assert!(write_result.contains("Wrote"));

        let read_args = serde_json::json!({"path": path});
        let read_result = execute_tool("read_file", &read_args);
        assert!(read_result.contains("test content"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_tool_schemas_valid_json() {
        let schemas = get_tool_schemas(false);
        assert!(!schemas.is_empty());
        for schema in &schemas {
            assert!(schema.get("type").is_some());
            assert!(schema.get("function").is_some());
        }
    }

    #[test]
    fn test_web_search_schema() {
        let with_search = get_tool_schemas(true);
        let without_search = get_tool_schemas(false);
        assert!(with_search.len() > without_search.len());
    }
}
