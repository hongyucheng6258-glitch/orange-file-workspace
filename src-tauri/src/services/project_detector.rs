//! 项目运行时识别：只读项目根元数据，识别 Node.js / Python / Rust 项目。
//!
//! 识别器不执行任何项目脚本，不读取项目源码。所有候选命令必须转换为
//! 结构化的程序和参数数组，不能包含多步骤 shell 流程。

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

/// 识别出的运行时类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeKind {
    Node,
    Python,
    Rust,
}

/// 候选运行命令：结构化程序和参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeCandidate {
    pub label: String,
    pub executable: String,
    pub args: Vec<String>,
    /// 0-100，用于前端排序推荐。
    pub confidence: u8,
}

/// 识别结果。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DetectionResult {
    pub runtime_kind: Option<RuntimeKind>,
    pub candidates: Vec<RuntimeCandidate>,
    pub diagnostics: Vec<String>,
}

impl DetectionResult {
    fn push_candidate(
        &mut self,
        label: impl Into<String>,
        executable: impl Into<String>,
        args: Vec<String>,
        confidence: u8,
    ) {
        self.candidates.push(RuntimeCandidate {
            label: label.into(),
            executable: executable.into(),
            args,
            confidence,
        });
    }

    fn push_diag(&mut self, msg: impl Into<String>) {
        self.diagnostics.push(msg.into());
    }
}

/// 项目根目录只读访问抽象；生产实现读取磁盘，测试实现记录读取路径。
pub trait ProjectFs: Send + Sync {
    fn read_to_string(&self, path: &Path) -> Option<String>;
    fn exists(&self, path: &Path) -> bool;
    fn is_file(&self, path: &Path) -> bool;
    /// 运行 `cargo metadata --no-deps --format-version 1` 并返回 stdout。
    fn cargo_metadata_json(&self, root: &Path) -> Option<String>;
    /// 在 PATH 中解析可执行文件。
    fn resolve_on_path(&self, name: &str) -> Option<PathBuf>;
}

/// 磁盘生产实现。
pub struct DiskProjectFs;

impl ProjectFs for DiskProjectFs {
    fn read_to_string(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn cargo_metadata_json(&self, root: &Path) -> Option<String> {
        let manifest = root.join("Cargo.toml");
        if !manifest.is_file() {
            return None;
        }
        let out = std::process::Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .arg("--manifest-path")
            .arg(&manifest)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8(out.stdout).ok()
    }

    fn resolve_on_path(&self, name: &str) -> Option<PathBuf> {
        let path_var = std::env::var_os("PATH")?;
        let exts: Vec<String> = if cfg!(windows) {
            std::env::var_os("PATHEXT")
                .map(|e| {
                    e.to_string_lossy()
                        .split(';')
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_else(|| vec![String::new()])
        } else {
            vec![String::new()]
        };
        for dir in std::env::split_paths(&path_var) {
            for ext in &exts {
                let cand = dir.join(format!("{name}{ext}"));
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
        None
    }
}

/// 词法规范化：去除 `.` 与 `..`，用于候选路径的项目根边界校验。
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 候选路径必须位于项目根内（词法校验，真实文件系统校验在运行配置阶段完成）。
fn ensure_within(root: &Path, candidate: &Path) -> Option<PathBuf> {
    let root = normalize_lexical(root);
    let candidate = normalize_lexical(candidate);
    if candidate.strip_prefix(&root).is_ok() {
        Some(candidate)
    } else {
        None
    }
}

/// 检查是否存在任一 Python 入口标记文件。
fn has_python_marker(fs: &dyn ProjectFs, root: &Path) -> bool {
    const PY_FILES: [&str; 3] = ["main.py", "app.py", "manage.py"];
    fs.exists(&root.join("pyproject.toml"))
        || fs.exists(&root.join("requirements.txt"))
        || PY_FILES.iter().any(|f| fs.exists(&root.join(f)))
}

/// 根据锁文件选择 Node 包管理器。
fn pick_node_runner(fs: &dyn ProjectFs, root: &Path) -> &'static str {
    const LOCKS: [(&str, &str); 5] = [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lock", "bun"),
        ("bun.lockb", "bun"),
        ("package-lock.json", "npm"),
    ];
    for (file, runner) in LOCKS {
        if fs.exists(&root.join(file)) {
            return runner;
        }
    }
    "npm"
}

fn detect_node(fs: &dyn ProjectFs, root: &Path, result: &mut DetectionResult) {
    let pkg_path = root.join("package.json");
    let Some(text) = fs.read_to_string(&pkg_path) else {
        return;
    };
    let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&text) else {
        result.push_diag("package.json 解析失败");
        return;
    };
    let runner = pick_node_runner(fs, root);
    let scripts = pkg.get("scripts").and_then(|s| s.as_object());

    let mut named: Vec<(String, u8)> = Vec::new();
    if let Some(scripts) = scripts {
        if scripts.get("dev").and_then(|v| v.as_str()).is_some() {
            named.push((format!("{runner} run dev"), 100));
        }
        if scripts.get("start").and_then(|v| v.as_str()).is_some() {
            named.push((format!("{runner} run start"), 90));
        }
        for (name, value) in scripts {
            if name != "dev" && name != "start" && value.as_str().is_some() {
                named.push((format!("{runner} run {name}"), 50));
            }
        }
    }

    if !named.is_empty() {
        for (label, confidence) in named {
            let mut parts = label.split_whitespace();
            let executable = parts.next().unwrap_or(runner).to_string();
            let args: Vec<String> = parts.map(|s| s.to_string()).collect();
            result.push_candidate(label, executable, args, confidence);
        }
        return;
    }

    // 没有 dev/start 时回退到 package.json.main。
    if let Some(main) = pkg.get("main").and_then(|v| v.as_str()) {
        let main_path = root.join(main);
        if ensure_within(root, &main_path).is_some() && fs.is_file(&main_path) {
            result.push_candidate(format!("node {main}"), "node", vec![main.to_string()], 70);
        } else {
            result.push_diag(format!("package.json.main 入口不存在或越界: {main}"));
        }
    }
}

fn detect_rust(fs: &dyn ProjectFs, root: &Path, result: &mut DetectionResult) {
    if !fs.exists(&root.join("Cargo.toml")) {
        return;
    }
    let Some(json) = fs.cargo_metadata_json(root) else {
        result.push_diag("cargo 不可用或 Cargo.toml 无法解析，无法生成运行候选");
        return;
    };
    let Ok(meta) = serde_json::from_str::<serde_json::Value>(&json) else {
        result.push_diag("cargo metadata 输出解析失败");
        return;
    };
    let Some(packages) = meta.get("packages").and_then(|p| p.as_array()) else {
        result.push_diag("cargo metadata 缺少 packages");
        return;
    };
    if packages.len() > 1 {
        result.push_diag("workspace 含多个 package，请补充 --package 参数");
        return;
    }
    let Some(pkg) = packages.first() else {
        return;
    };
    let bin_count = pkg
        .get("targets")
        .and_then(|t| t.as_array())
        .map(|targets| {
            targets
                .iter()
                .filter(|t| {
                    t.get("kind")
                        .and_then(|k| k.as_array())
                        .map(|k| k.iter().any(|v| v.as_str() == Some("bin")))
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    match bin_count {
        0 => result.push_diag("项目没有可运行的二进制目标（可能是纯库）"),
        1 => result.push_candidate("cargo run", "cargo", vec!["run".to_string()], 100),
        _ => result.push_diag("存在多个二进制目标，请补充 --bin 参数"),
    }
}

fn detect_python(fs: &dyn ProjectFs, root: &Path, result: &mut DetectionResult) {
    if !has_python_marker(fs, root) {
        return;
    }

    // [project.scripts] 仅生成诊断：entry point 是 module:function，
    // 不能等价改写为 python -m，需要用户安装项目或手动填写命令。
    if let Some(text) = fs.read_to_string(&root.join("pyproject.toml")) {
        match toml::from_str::<toml::Value>(&text) {
            Ok(value) => {
                if let Some(scripts) = value
                    .get("project")
                    .and_then(|p| p.get("scripts"))
                    .and_then(|s| s.as_table())
                {
                    for (name, entry) in scripts {
                        if let Some(entry) = entry.as_str() {
                            result.push_diag(format!(
                                "pyproject 脚本 {name} = {entry}：请安装项目后运行对应可执行文件，或手动填写调用命令"
                            ));
                        }
                    }
                }
            }
            Err(_) => result.push_diag("pyproject.toml 解析失败"),
        }
    }
    if fs.exists(&root.join("requirements.txt")) {
        result.push_diag("检测到 requirements.txt：如需运行请确认项目依赖已安装");
    }

    // 解释器候选：项目虚拟环境 > PATH python > py -3。
    let venv = root.join(".venv").join("Scripts").join("python.exe");
    let interpreter: Option<(PathBuf, Vec<String>)> = if fs.is_file(&venv) {
        Some((venv, Vec::new()))
    } else if let Some(py) = fs.resolve_on_path("python") {
        Some((py, Vec::new()))
    } else if fs.resolve_on_path("py").is_some() {
        Some((PathBuf::from("py"), vec!["-3".to_string()]))
    } else {
        result.push_diag("未找到可用的 Python 解释器（已检查 .venv、PATH 中的 python 和 py -3）");
        None
    };
    let Some((interpreter, base_args)) = interpreter else {
        return;
    };

    // 入口文件顺序。
    const ENTRIES: [&str; 3] = ["main.py", "app.py", "manage.py"];
    for entry in ENTRIES {
        let entry_path = root.join(entry);
        if fs.is_file(&entry_path) {
            let mut args = base_args.clone();
            args.push(entry.to_string());
            result.push_candidate(
                format!("{} {}", interpreter.display(), entry),
                interpreter.to_string_lossy().to_string(),
                args,
                80,
            );
            return;
        }
    }
    result.push_diag("未找到 main.py / app.py / manage.py 入口文件");
}

/// 主入口：检测项目根目录下的运行时并生成候选命令。
pub fn detect(fs: &dyn ProjectFs, root: &Path) -> DetectionResult {
    let root = normalize_lexical(root);
    let mut result = DetectionResult::default();

    let mut kinds: Vec<RuntimeKind> = Vec::new();
    if fs.exists(&root.join("package.json")) {
        kinds.push(RuntimeKind::Node);
    }
    if fs.exists(&root.join("Cargo.toml")) {
        kinds.push(RuntimeKind::Rust);
    }
    if has_python_marker(fs, &root) {
        kinds.push(RuntimeKind::Python);
    }
    if kinds.is_empty() {
        result.diagnostics.push("未识别到受支持的运行时配置文件（package.json / Cargo.toml / pyproject.toml / requirements.txt / Python 入口）".to_string());
        return result;
    }

    result.runtime_kind = kinds.first().copied();
    for kind in kinds {
        match kind {
            RuntimeKind::Node => detect_node(fs, &root, &mut result),
            RuntimeKind::Rust => detect_rust(fs, &root, &mut result),
            RuntimeKind::Python => detect_python(fs, &root, &mut result),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    struct FakeFs {
        files: BTreeMap<PathBuf, String>,
        dirs: Vec<PathBuf>,
        cargo_meta: Option<String>,
        path_exes: Vec<String>,
        reads: Mutex<Vec<PathBuf>>,
    }

    impl FakeFs {
        fn new() -> Self {
            Self {
                files: BTreeMap::new(),
                dirs: Vec::new(),
                cargo_meta: None,
                path_exes: Vec::new(),
                reads: Mutex::new(Vec::new()),
            }
        }

        fn file(mut self, path: impl AsRef<Path>, content: impl Into<String>) -> Self {
            self.files
                .insert(path.as_ref().to_path_buf(), content.into());
            self
        }

        fn dir(mut self, path: impl AsRef<Path>) -> Self {
            self.dirs.push(path.as_ref().to_path_buf());
            self
        }

        fn cargo(mut self, json: impl Into<String>) -> Self {
            self.cargo_meta = Some(json.into());
            self
        }

        fn path_exe(mut self, name: impl Into<String>) -> Self {
            self.path_exes.push(name.into());
            self
        }

        fn read_log(&self) -> Vec<PathBuf> {
            self.reads.lock().unwrap().clone()
        }
    }

    impl ProjectFs for FakeFs {
        fn read_to_string(&self, path: &Path) -> Option<String> {
            self.reads.lock().unwrap().push(path.to_path_buf());
            self.files.get(path).cloned()
        }

        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path) || self.dirs.iter().any(|d| d == path)
        }

        fn is_file(&self, path: &Path) -> bool {
            self.files.contains_key(path)
        }

        fn cargo_metadata_json(&self, _root: &Path) -> Option<String> {
            self.cargo_meta.clone()
        }

        fn resolve_on_path(&self, name: &str) -> Option<PathBuf> {
            self.path_exes
                .iter()
                .find(|p| p.as_str() == name)
                .map(|_| PathBuf::from(format!("C:\\tools\\{name}.exe")))
        }
    }

    fn root() -> PathBuf {
        PathBuf::from("C:\\proj")
    }

    fn node_pkg(scripts: &str) -> String {
        format!(r#"{{"name":"demo","scripts":{scripts}}}"#)
    }

    #[test]
    fn node_dev_start_order_and_default_npm() {
        let fs = FakeFs::new().file(
            root().join("package.json"),
            node_pkg(r#"{"dev":"vite","start":"vite preview"}"#),
        );
        let r = detect(&fs, &root());
        assert_eq!(r.runtime_kind, Some(RuntimeKind::Node));
        let labels: Vec<&str> = r.candidates.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["npm run dev", "npm run start"]);
        assert_eq!(r.candidates[0].executable, "npm");
        assert_eq!(r.candidates[0].args, vec!["run", "dev"]);
    }

    #[test]
    fn node_pnpm_lock_preferred() {
        let fs = FakeFs::new()
            .file(root().join("package.json"), node_pkg(r#"{"dev":"vite"}"#))
            .file(root().join("pnpm-lock.yaml"), "");
        let r = detect(&fs, &root());
        assert_eq!(r.candidates[0].label, "pnpm run dev");
    }

    #[test]
    fn node_other_scripts_listed_last() {
        let fs = FakeFs::new().file(
            root().join("package.json"),
            node_pkg(r#"{"build":"tsc","lint":"eslint ."}"#),
        );
        let r = detect(&fs, &root());
        let labels: Vec<&str> = r.candidates.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, vec!["npm run build", "npm run lint"]);
    }

    #[test]
    fn node_main_fallback_when_no_scripts() {
        let fs = FakeFs::new()
            .file(
                root().join("package.json"),
                r#"{"name":"demo","main":"index.js"}"#,
            )
            .file(root().join("index.js"), "console.log(1)");
        let r = detect(&fs, &root());
        assert_eq!(r.candidates.len(), 1);
        assert_eq!(r.candidates[0].executable, "node");
        assert_eq!(r.candidates[0].args, vec!["index.js"]);
    }

    #[test]
    fn node_main_out_of_bounds_rejected() {
        let fs = FakeFs::new()
            .file(
                root().join("package.json"),
                r#"{"name":"demo","main":"../evil.js"}"#,
            )
            .file(PathBuf::from("C:\\evil.js"), "bad");
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
        assert!(r
            .diagnostics
            .iter()
            .any(|d| d.contains("越界") || d.contains("不存在")));
    }

    #[test]
    fn node_no_scripts_no_main_diag() {
        let fs = FakeFs::new().file(root().join("package.json"), r#"{"name":"demo"}"#);
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
    }

    #[test]
    fn python_entry_order_main_first() {
        let fs = FakeFs::new()
            .file(root().join("main.py"), "")
            .file(root().join("app.py"), "")
            .path_exe("python");
        let r = detect(&fs, &root());
        assert_eq!(r.runtime_kind, Some(RuntimeKind::Python));
        assert_eq!(r.candidates[0].args, vec!["main.py"]);
        assert_eq!(r.candidates[0].executable, "C:\\tools\\python.exe");
    }

    #[test]
    fn python_venv_interpreter_preferred() {
        let fs = FakeFs::new()
            .file(root().join("app.py"), "")
            .file(root().join(".venv").join("Scripts").join("python.exe"), "");
        let r = detect(&fs, &root());
        assert_eq!(
            r.candidates[0].executable,
            root()
                .join(".venv")
                .join("Scripts")
                .join("python.exe")
                .to_string_lossy()
        );
    }

    #[test]
    fn python_scripts_only_diagnostic() {
        let fs = FakeFs::new()
            .file(
                root().join("pyproject.toml"),
                r#"[project]
name = "demo"
[project.scripts]
cli = "demo.cli:main"
"#,
            )
            .path_exe("python");
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
        assert!(r.diagnostics.iter().any(|d| d.contains("cli")));
        assert!(r.diagnostics.iter().all(|d| !d.contains("python -m")));
    }

    #[test]
    fn python_no_interpreter_diag() {
        let fs = FakeFs::new().file(root().join("main.py"), "");
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
        assert!(r.diagnostics.iter().any(|d| d.contains("解释器")));
    }

    #[test]
    fn rust_single_package_cargo_run() {
        let meta = r#"{"packages":[{"name":"demo","targets":[{"name":"demo","kind":["bin"]}]}]}"#;
        let fs = FakeFs::new()
            .file(root().join("Cargo.toml"), "[package]\nname=\"demo\"")
            .cargo(meta);
        let r = detect(&fs, &root());
        assert_eq!(r.runtime_kind, Some(RuntimeKind::Rust));
        assert_eq!(r.candidates[0].label, "cargo run");
    }

    #[test]
    fn rust_workspace_multi_package_requires_choice() {
        let meta = r#"{"packages":[{"name":"a","targets":[{"name":"a","kind":["bin"]}]},{"name":"b","targets":[{"name":"b","kind":["bin"]}]}]}"#;
        let fs = FakeFs::new()
            .file(
                root().join("Cargo.toml"),
                "[workspace]\nmembers=[\"a\",\"b\"]",
            )
            .cargo(meta);
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
        assert!(r.diagnostics.iter().any(|d| d.contains("--package")));
    }

    #[test]
    fn rust_multi_bin_requires_choice() {
        let meta = r#"{"packages":[{"name":"demo","targets":[{"name":"a","kind":["bin"]},{"name":"b","kind":["bin"]}]}]}"#;
        let fs = FakeFs::new()
            .file(root().join("Cargo.toml"), "[package]\nname=\"demo\"")
            .cargo(meta);
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
        assert!(r.diagnostics.iter().any(|d| d.contains("--bin")));
    }

    #[test]
    fn rust_cargo_unavailable_diag() {
        let fs = FakeFs::new().file(root().join("Cargo.toml"), "[package]\nname=\"demo\"");
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
        assert!(r.diagnostics.iter().any(|d| d.contains("cargo")));
    }

    #[test]
    fn unknown_project_diag() {
        let fs = FakeFs::new().file(root().join("README.md"), "");
        let r = detect(&fs, &root());
        assert!(r.runtime_kind.is_none());
        assert!(r.candidates.is_empty());
        assert!(!r.diagnostics.is_empty());
    }

    #[test]
    fn detector_never_reads_source_files() {
        let fs = FakeFs::new()
            .file(root().join("package.json"), node_pkg(r#"{"dev":"vite"}"#))
            .file(root().join("src").join("index.ts"), "export {}")
            .file(root().join("src").join("lib.rs"), "fn main() {}")
            .file(root().join("src").join("main.py"), "print(1)")
            .file(
                root().join("node_modules").join("x").join("index.js"),
                "module.exports={}",
            );
        let _ = detect(&fs, &root());
        let reads = fs.read_log();
        for p in &reads {
            let rel = p.strip_prefix(&root()).unwrap();
            let first = rel
                .components()
                .next()
                .unwrap()
                .as_os_str()
                .to_string_lossy()
                .to_string();
            assert!(
                !matches!(
                    first.as_str(),
                    "src" | "node_modules" | "target" | ".git" | ".venv" | "Cargo.toml"
                ),
                "识别器不应读取 {p:?}"
            );
        }
    }
}
