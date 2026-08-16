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
    Java,
    Makefile,
    Docker,
}

/// 候选运行命令：结构化程序和参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuntimeCandidate {
    pub label: String,
    pub executable: String,
    pub args: Vec<String>,
    /// 0-100，用于前端排序推荐。
    pub confidence: u8,
    /// 相对项目根的工作目录（None = 项目根）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
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
        self.push_candidate_in(label, executable, args, confidence, None);
    }

    /// 指定子目录工作目录的候选（子项目探测使用）。
    fn push_candidate_in(
        &mut self,
        label: impl Into<String>,
        executable: impl Into<String>,
        args: Vec<String>,
        confidence: u8,
        cwd: Option<String>,
    ) {
        self.candidates.push(RuntimeCandidate {
            label: label.into(),
            executable: executable.into(),
            args,
            confidence,
            cwd,
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
    /// 列出根目录直接子目录（1 层，不递归；噪音目录由调用方过滤）。
    fn list_child_dirs(&self, root: &Path) -> Vec<PathBuf>;
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

    fn list_child_dirs(&self, root: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        dirs.sort();
        dirs
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
        // Windows 上 runner 常以无扩展名 shim 与 .cmd 并存（如 npm / npm.cmd），
        // 必须解析到真实可执行（.cmd/.exe），否则 CreateProcess 会报错误 193。
        let runner_exe = fs
            .resolve_on_path(runner)
            .unwrap_or_else(|| PathBuf::from(runner));
        for (label, confidence) in named {
            let mut parts = label.split_whitespace();
            let _first = parts.next().unwrap_or(runner);
            let args: Vec<String> = parts.map(|s| s.to_string()).collect();
            result.push_candidate(
                label,
                runner_exe.to_string_lossy().to_string(),
                args,
                confidence,
            );
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

/// Java：Maven/Gradle + Spring Boot 插件可确认时生成候选；否则只出诊断，不猜测目标。
fn detect_java(fs: &dyn ProjectFs, root: &Path, result: &mut DetectionResult) {
    if fs.is_file(&root.join("pom.xml")) {
        let text = fs.read_to_string(&root.join("pom.xml")).unwrap_or_default();
        let has_boot = text.contains("spring-boot-maven-plugin");
        if let Some(mvn) = fs.resolve_on_path("mvn") {
            if has_boot {
                result.push_candidate(
                    "mvn spring-boot:run",
                    mvn.to_string_lossy().to_string(),
                    vec!["spring-boot:run".to_string()],
                    80,
                );
            } else {
                result.push_diag(
                    "pom.xml 未包含 spring-boot-maven-plugin，不自动猜测运行目标，请手动配置命令",
                );
            }
        } else {
            result.push_diag("检测到 Maven 项目但 PATH 中未解析到 mvn");
        }
    }
    let gradle_file = ["build.gradle", "build.gradle.kts"]
        .iter()
        .map(|f| root.join(f))
        .find(|p| fs.is_file(p));
    if let Some(gf) = gradle_file {
        let text = fs.read_to_string(&gf).unwrap_or_default();
        let has_boot = text.contains("org.springframework.boot");
        if let Some(gradle) = fs.resolve_on_path("gradle") {
            if has_boot {
                result.push_candidate(
                    "gradle bootRun",
                    gradle.to_string_lossy().to_string(),
                    vec!["bootRun".to_string()],
                    80,
                );
            } else {
                result.push_diag(
                    "build.gradle 未包含 spring-boot 插件，不自动猜测运行目标，请手动配置命令",
                );
            }
        } else {
            result.push_diag("检测到 Gradle 项目但 PATH 中未解析到 gradle");
        }
    }
}

/// Makefile：存在 make 时提供默认目标候选。
fn detect_makefile(fs: &dyn ProjectFs, _root: &Path, result: &mut DetectionResult) {
    if let Some(make) = fs.resolve_on_path("make") {
        result.push_candidate("make", make.to_string_lossy().to_string(), vec![], 60);
    } else {
        result.push_diag("检测到 Makefile 但 PATH 中未解析到 make；默认目标由 Makefile 决定");
    }
}

/// Docker：compose 配置可确认时生成候选；仅有 Dockerfile 时只出诊断。
fn detect_docker(fs: &dyn ProjectFs, root: &Path, result: &mut DetectionResult) {
    let compose = ["docker-compose.yml", "compose.yaml", "compose.yml"]
        .iter()
        .any(|f| fs.is_file(&root.join(f)));
    if compose {
        if let Some(docker) = fs.resolve_on_path("docker") {
            result.push_candidate(
                "docker compose up",
                docker.to_string_lossy().to_string(),
                vec!["compose".to_string(), "up".to_string()],
                70,
            );
        } else {
            result.push_diag("检测到 docker-compose 配置但 PATH 中未解析到 docker");
        }
    } else if fs.is_file(&root.join("Dockerfile")) {
        if fs.resolve_on_path("docker").is_some() {
            result.push_diag("检测到 Dockerfile：构建与运行方式依赖镜像配置，请手动配置命令");
        } else {
            result.push_diag("检测到 Dockerfile 但 PATH 中未解析到 docker");
        }
    }
}

/// 识别根目录直接配置文件；返回是否识别到任何支持的 marker。
fn detect_into(fs: &dyn ProjectFs, root: &Path, result: &mut DetectionResult) -> bool {
    let mut kinds: Vec<RuntimeKind> = Vec::new();
    if fs.exists(&root.join("package.json")) {
        kinds.push(RuntimeKind::Node);
    }
    if fs.exists(&root.join("Cargo.toml")) {
        kinds.push(RuntimeKind::Rust);
    }
    if has_python_marker(fs, root) {
        kinds.push(RuntimeKind::Python);
    }
    if fs.exists(&root.join("pom.xml"))
        || fs.exists(&root.join("build.gradle"))
        || fs.exists(&root.join("build.gradle.kts"))
    {
        kinds.push(RuntimeKind::Java);
    }
    if fs.exists(&root.join("Makefile"))
        || fs.exists(&root.join("makefile"))
        || fs.exists(&root.join("GNUmakefile"))
    {
        kinds.push(RuntimeKind::Makefile);
    }
    if fs.exists(&root.join("Dockerfile"))
        || fs.exists(&root.join("docker-compose.yml"))
        || fs.exists(&root.join("compose.yaml"))
        || fs.exists(&root.join("compose.yml"))
    {
        kinds.push(RuntimeKind::Docker);
    }
    if kinds.is_empty() {
        return false;
    }

    result.runtime_kind = kinds.first().copied();
    for kind in kinds {
        match kind {
            RuntimeKind::Node => detect_node(fs, root, result),
            RuntimeKind::Rust => detect_rust(fs, root, result),
            RuntimeKind::Python => detect_python(fs, root, result),
            RuntimeKind::Java => detect_java(fs, root, result),
            RuntimeKind::Makefile => detect_makefile(fs, root, result),
            RuntimeKind::Docker => detect_docker(fs, root, result),
        }
    }
    true
}

/// 子项目探测跳过的噪音目录。
const SKIP_SUBDIR_NAMES: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    ".venv",
    "dist",
    "build",
    "__pycache__",
    ".idea",
    ".vscode",
];

/// 子项目探测的最大深度（相对项目根的层数，如 web/frontend/admin 为 3 层）。
const MAX_SUBPROJECT_DEPTH: usize = 3;

/// 把一个子项目检测结果合并进主结果，cwd 与 label 前缀为相对项目根路径。
fn merge_subproject(
    result: &mut DetectionResult,
    found: &mut usize,
    rel_path: String,
    sub: DetectionResult,
) {
    *found += 1;
    if result.runtime_kind.is_none() {
        result.runtime_kind = sub.runtime_kind;
    }
    for cand in sub.candidates {
        let label = format!("{rel_path}: {}", cand.label);
        result.push_candidate_in(
            label,
            cand.executable,
            cand.args,
            cand.confidence,
            Some(rel_path.clone()),
        );
    }
    for d in sub.diagnostics {
        result.push_diag(format!("{rel_path}: {d}"));
    }
}

/// 递归扫描：目录无直接配置时下探其子目录，最多 `depth` 层；有配置即停（不递归）。
fn scan_subprojects(
    fs: &dyn ProjectFs,
    dir: &Path,
    depth: usize,
    rel_prefix: String,
    result: &mut DetectionResult,
    found: &mut usize,
) {
    if depth > MAX_SUBPROJECT_DEPTH {
        return;
    }
    for child in fs.list_child_dirs(dir) {
        let Some(name) = child.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        if SKIP_SUBDIR_NAMES.contains(&name.as_str()) {
            continue;
        }
        let rel = if rel_prefix.is_empty() {
            name.clone()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let mut sub = DetectionResult::default();
        if detect_into(fs, &child, &mut sub) {
            merge_subproject(result, found, rel, sub);
        } else {
            scan_subprojects(fs, &child, depth + 1, rel, result, found);
        }
    }
}

/// 仅当根目录没有直接配置文件时，有限深度扫描子目录识别子项目
/// （backend/frontend、web/backend、web/frontend/admin 等分离结构）。
/// 候选工作目录设为对应子目录相对路径。
fn detect_subprojects(fs: &dyn ProjectFs, root: &Path, result: &mut DetectionResult) {
    let mut found = 0usize;
    scan_subprojects(fs, root, 1, String::new(), result, &mut found);
    if found == 0 {
        result.push_diag(
            "未识别到受支持的运行时配置文件（package.json / Cargo.toml / pyproject.toml / requirements.txt / Python 入口 / pom.xml / build.gradle / Makefile / Dockerfile），子目录中也未发现受支持的项目"
                .to_string(),
        );
    } else {
        result.push_diag(format!(
            "在子目录中发现 {found} 个可运行项目，候选的工作目录已设为对应子目录"
        ));
    }
}

/// 主入口：检测项目根目录下的运行时并生成候选命令。
pub fn detect(fs: &dyn ProjectFs, root: &Path) -> DetectionResult {
    let root = normalize_lexical(root);
    let mut result = DetectionResult::default();
    if detect_into(fs, &root, &mut result) {
        return result;
    }
    // 根目录无直接配置：一层子目录探测（backend / frontend 等分离结构）。
    detect_subprojects(fs, &root, &mut result);
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

        fn list_child_dirs(&self, root: &Path) -> Vec<PathBuf> {
            let mut out: Vec<PathBuf> = self
                .dirs
                .iter()
                .filter(|d| d.parent().map(|p| p == root).unwrap_or(false))
                .cloned()
                .collect();
            out.sort();
            out
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
            let rel = p.strip_prefix(root()).unwrap();
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

    // ---- Java / Makefile / Docker ----

    #[test]
    fn java_maven_spring_boot_candidate() {
        let fs = FakeFs::new()
            .file(
                root().join("pom.xml"),
                "<project><build><plugins><plugin><artifactId>spring-boot-maven-plugin</artifactId></plugin></plugins></build></project>",
            )
            .path_exe("mvn");
        let r = detect(&fs, &root());
        assert_eq!(r.runtime_kind, Some(RuntimeKind::Java));
        let cand = r
            .candidates
            .iter()
            .find(|c| c.label == "mvn spring-boot:run");
        assert!(cand.is_some(), "应生成 mvn spring-boot:run 候选");
        assert_eq!(cand.unwrap().args, vec!["spring-boot:run"]);
        assert_eq!(cand.unwrap().confidence, 80);
    }

    #[test]
    fn java_maven_without_plugin_only_diag() {
        let fs = FakeFs::new()
            .file(
                root().join("pom.xml"),
                "<project><groupId>x</groupId></project>",
            )
            .path_exe("mvn");
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty(), "不猜测运行目标");
        assert!(r
            .diagnostics
            .iter()
            .any(|d| d.contains("spring-boot-maven-plugin")));
    }

    #[test]
    fn java_maven_without_mvn_diag() {
        let fs = FakeFs::new().file(root().join("pom.xml"), "<project/>");
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
        assert!(r.diagnostics.iter().any(|d| d.contains("mvn")));
    }

    #[test]
    fn java_gradle_spring_boot_candidate() {
        let fs = FakeFs::new()
            .file(
                root().join("build.gradle"),
                "plugins { id 'org.springframework.boot' }",
            )
            .path_exe("gradle");
        let r = detect(&fs, &root());
        let cand = r.candidates.iter().find(|c| c.label == "gradle bootRun");
        assert!(cand.is_some(), "应生成 gradle bootRun 候选");
        assert_eq!(cand.unwrap().confidence, 80);
    }

    #[test]
    fn makefile_candidate_with_make() {
        let fs = FakeFs::new()
            .file(root().join("Makefile"), "all:\n\t@echo hi")
            .path_exe("make");
        let r = detect(&fs, &root());
        assert_eq!(r.runtime_kind, Some(RuntimeKind::Makefile));
        let cand = r.candidates.iter().find(|c| c.label == "make");
        assert!(cand.is_some());
        assert_eq!(cand.unwrap().args.len(), 0);
    }

    #[test]
    fn makefile_without_make_diag() {
        let fs = FakeFs::new().file(root().join("Makefile"), "all:");
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
        assert!(r.diagnostics.iter().any(|d| d.contains("make")));
    }

    #[test]
    fn docker_compose_candidate() {
        let fs = FakeFs::new()
            .file(root().join("docker-compose.yml"), "services: {}")
            .path_exe("docker");
        let r = detect(&fs, &root());
        assert_eq!(r.runtime_kind, Some(RuntimeKind::Docker));
        let cand = r.candidates.iter().find(|c| c.label == "docker compose up");
        assert!(cand.is_some());
        assert_eq!(cand.unwrap().args, vec!["compose", "up"]);
    }

    #[test]
    fn dockerfile_only_diag() {
        let fs = FakeFs::new()
            .file(root().join("Dockerfile"), "FROM node:20")
            .path_exe("docker");
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty(), "仅 Dockerfile 不自动猜测构建命令");
        assert!(r.diagnostics.iter().any(|d| d.contains("Dockerfile")));
    }

    #[test]
    fn multi_marker_priority_node_first() {
        // package.json + pom.xml 同时存在 → 主类型为 Node，Java 也参与识别。
        let fs = FakeFs::new()
            .file(root().join("package.json"), node_pkg(r#"{"dev":"vite"}"#))
            .file(root().join("pom.xml"), "<project/>")
            .path_exe("mvn");
        let r = detect(&fs, &root());
        assert_eq!(r.runtime_kind, Some(RuntimeKind::Node));
        assert!(r.candidates.iter().any(|c| c.label.contains("npm run dev")));
    }

    // ---- 子项目探测 ----

    #[test]
    fn subproject_spring_backend_and_vue_frontend() {
        let fs = FakeFs::new()
            .dir(root().join("backend"))
            .file(
                root().join("backend").join("pom.xml"),
                "<project><build><plugins><plugin><artifactId>spring-boot-maven-plugin</artifactId></plugin></plugins></build></project>",
            )
            .path_exe("mvn")
            .dir(root().join("frontend"))
            .file(
                root().join("frontend").join("package.json"),
                node_pkg(r#"{"dev":"vite"}"#),
            )
            .path_exe("npm");
        let r = detect(&fs, &root());
        assert_eq!(r.runtime_kind, Some(RuntimeKind::Java));
        let labels: Vec<&str> = r.candidates.iter().map(|c| c.label.as_str()).collect();
        assert!(
            labels.contains(&"backend: mvn spring-boot:run"),
            "实际: {labels:?}"
        );
        assert!(
            labels.contains(&"frontend: npm run dev"),
            "实际: {labels:?}"
        );
        let backend = r
            .candidates
            .iter()
            .find(|c| c.label == "backend: mvn spring-boot:run")
            .unwrap();
        assert_eq!(backend.cwd.as_deref(), Some("backend"));
        let frontend = r
            .candidates
            .iter()
            .find(|c| c.label == "frontend: npm run dev")
            .unwrap();
        assert_eq!(frontend.cwd.as_deref(), Some("frontend"));
        assert!(r.diagnostics.iter().any(|d| d.contains("2 个可运行项目")));
    }

    #[test]
    fn subproject_skips_noise_dirs() {
        let fs = FakeFs::new()
            .dir(root().join("node_modules"))
            .file(
                root().join("node_modules").join("package.json"),
                node_pkg(r#"{"dev":"vite"}"#),
            )
            .dir(root().join("target"))
            .file(root().join("target").join("Cargo.toml"), "[package]")
            .dir(root().join(".git"))
            .dir(root().join(".venv"))
            .dir(root().join("build"))
            .dir(root().join("dist"));
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty(), "噪音目录不应产生候选");
        assert!(r.runtime_kind.is_none());
        assert!(r.diagnostics.iter().any(|d| d.contains("子目录中也未发现")));
    }

    #[test]
    fn root_config_skips_subproject_scan() {
        let fs = FakeFs::new()
            .file(root().join("package.json"), node_pkg(r#"{"dev":"vite"}"#))
            .dir(root().join("backend"))
            .file(
                root().join("backend").join("pom.xml"),
                "<project><build><plugins><plugin><artifactId>spring-boot-maven-plugin</artifactId></plugin></plugins></build></project>",
            )
            .path_exe("mvn");
        let r = detect(&fs, &root());
        assert_eq!(r.runtime_kind, Some(RuntimeKind::Node));
        assert!(
            r.candidates
                .iter()
                .all(|c| !c.label.starts_with("backend:")),
            "根目录有配置时不应触发子项目探测"
        );
        assert!(r.candidates.iter().all(|c| c.cwd.is_none()));
    }

    #[test]
    fn subproject_without_mvn_diag_is_prefixed() {
        let fs = FakeFs::new()
            .dir(root().join("backend"))
            .file(root().join("backend").join("pom.xml"), "<project/>");
        let r = detect(&fs, &root());
        assert!(r.candidates.is_empty());
        assert!(
            r.diagnostics.iter().any(|d| d.starts_with("backend:")),
            "子项目诊断应带子目录前缀"
        );
    }

    #[test]
    fn subproject_never_reads_source_files() {
        let fs = FakeFs::new()
            .dir(root().join("backend"))
            .file(root().join("backend").join("pom.xml"), "<project/>")
            .file(
                root()
                    .join("backend")
                    .join("src")
                    .join("main")
                    .join("java")
                    .join("App.java"),
                "class App {}",
            )
            .dir(root().join("frontend"))
            .file(
                root().join("frontend").join("package.json"),
                node_pkg(r#"{"dev":"vite"}"#),
            )
            .file(
                root().join("frontend").join("src").join("App.vue"),
                "<template/>",
            );
        let _ = detect(&fs, &root());
        let reads = fs.read_log();
        for p in &reads {
            let rel = p.strip_prefix(root()).unwrap();
            let parts: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            if parts.first().map(|d| d.as_str()) == Some("backend") {
                assert!(
                    parts.iter().any(|c| c == "pom.xml"),
                    "backend 只允许读取 pom.xml: {p:?}"
                );
            }
            if parts.first().map(|d| d.as_str()) == Some("frontend") {
                assert!(
                    parts.iter().any(|c| c == "package.json"),
                    "frontend 只允许读取 package.json: {p:?}"
                );
            }
        }
    }

    #[test]
    fn subproject_three_level_deep() {
        // web/backend（Spring Boot）+ web/frontend/admin（Vue3）→ 相对 cwd 候选。
        let fs = FakeFs::new()
            .dir(root().join("web"))
            .dir(root().join("web").join("backend"))
            .file(
                root().join("web").join("backend").join("pom.xml"),
                "<project><build><plugins><plugin><artifactId>spring-boot-maven-plugin</artifactId></plugin></plugins></build></project>",
            )
            .path_exe("mvn")
            .dir(root().join("web").join("frontend"))
            .dir(root().join("web").join("frontend").join("admin"))
            .file(
                root().join("web").join("frontend").join("admin").join("package.json"),
                node_pkg(r#"{"dev":"vite"}"#),
            )
            .path_exe("npm");
        let r = detect(&fs, &root());
        let backend = r
            .candidates
            .iter()
            .find(|c| c.label == "web/backend: mvn spring-boot:run");
        assert!(backend.is_some(), "应识别 web/backend: {:?}", r.candidates);
        assert_eq!(backend.unwrap().cwd.as_deref(), Some("web/backend"));
        let admin = r
            .candidates
            .iter()
            .find(|c| c.label == "web/frontend/admin: npm run dev");
        assert!(
            admin.is_some(),
            "应识别 web/frontend/admin: {:?}",
            r.candidates
        );
        assert_eq!(admin.unwrap().cwd.as_deref(), Some("web/frontend/admin"));
        assert!(r.diagnostics.iter().any(|d| d.contains("2 个可运行项目")));
    }

    #[test]
    fn subproject_depth_limit_stops_at_three() {
        // a/b/c/d 共 4 层 → 超出深度，不识别。
        let fs = FakeFs::new()
            .dir(root().join("a"))
            .dir(root().join("a").join("b"))
            .dir(root().join("a").join("b").join("c"))
            .dir(root().join("a").join("b").join("c").join("d"))
            .file(
                root()
                    .join("a")
                    .join("b")
                    .join("c")
                    .join("d")
                    .join("package.json"),
                node_pkg(r#"{"dev":"vite"}"#),
            );
        let r = detect(&fs, &root());
        assert!(
            r.candidates.is_empty(),
            "4 层超出深度限制: {:?}",
            r.candidates
        );
    }

    #[test]
    fn subproject_noise_dir_at_deep_level_skipped() {
        // web/node_modules/x/package.json → node_modules 在任何层都跳过。
        let fs = FakeFs::new()
            .dir(root().join("web"))
            .dir(root().join("web").join("node_modules"))
            .dir(root().join("web").join("node_modules").join("x"))
            .file(
                root()
                    .join("web")
                    .join("node_modules")
                    .join("x")
                    .join("package.json"),
                node_pkg(r#"{"dev":"vite"}"#),
            );
        let r = detect(&fs, &root());
        assert!(
            r.candidates.is_empty(),
            "噪音目录不应产生候选: {:?}",
            r.candidates
        );
    }
}
