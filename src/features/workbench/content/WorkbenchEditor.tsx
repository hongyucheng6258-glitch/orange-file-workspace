/**
 * Workbench 编辑器渲染器 — 在面板中显示和编辑文件
 *
 * 从 tab.params 读取 path / resourceId，直接通过 Tauri 读写文件内容。
 * 不依赖全局 editorStore（支持多面板同时打开不同文件）。
 */

import { useState, useEffect, useCallback, useMemo } from "react";
import CodeMirror from "@uiw/react-codemirror";
import { javascript } from "@codemirror/lang-javascript";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { sql } from "@codemirror/lang-sql";
import { java } from "@codemirror/lang-java";
import { cpp } from "@codemirror/lang-cpp";
import { yaml } from "@codemirror/lang-yaml";
import { php } from "@codemirror/lang-php";
import { StreamLanguage } from "@codemirror/language";
import { go } from "@codemirror/legacy-modes/mode/go";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import { Save, Loader2, AlertCircle } from "lucide-react";
import { call } from "../../../lib/tauri";
import { useSettingsStore } from "../../settings/stores/settingsStore";

interface Props {
  params: Record<string, unknown>;
}

function langFor(name: string) {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "ts":
    case "tsx":
      return javascript({ typescript: true, jsx: true });
    case "js":
    case "jsx":
    case "mjs":
      return javascript({ jsx: true });
    case "py":
      return python();
    case "rs":
      return rust();
    case "json":
    case "jsonc":
      return json();
    case "md":
    case "markdown":
      return markdown();
    case "css":
      return css();
    case "html":
    case "htm":
      return html();
    case "sql":
      return sql();
    case "java":
      return java();
    case "c":
    case "cpp":
    case "h":
    case "hpp":
    case "cc":
      return cpp();
    case "go":
      return StreamLanguage.define(go);
    case "toml":
      return StreamLanguage.define(toml);
    case "yaml":
    case "yml":
      return yaml();
    case "php":
      return php();
    default:
      return [];
  }
}

export function WorkbenchEditor({ params }: Props) {
  const resourceId = params.resourceId as string | undefined;
  const filePath = (params.path as string) ?? "";
  const fileName = filePath.split(/[\\/]/).pop() ?? "untitled";

  const [content, setContent] = useState("");
  const [original, setOriginal] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const terminalSettings = useSettingsStore((s) => s.settings?.terminal);
  const fontSize = terminalSettings?.font_size ?? 14;
  const tabSize = 2;

  const extensions = useMemo(() => [langFor(fileName)], [fileName]);

  // 加载文件内容
  useEffect(() => {
    if (!resourceId) {
      setLoading(false);
      setError("缺少 resourceId 参数");
      return;
    }
    setLoading(true);
    setError(null);
    call<{
      content: string;
      path: string;
      draft: string | null;
    }>("open_file", { resourceId })
      .then((data) => {
        const text = data.draft ?? data.content;
        setContent(text);
        setOriginal(data.content);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [resourceId]);

  const dirty = content !== original;

  const save = useCallback(() => {
    if (!resourceId || !dirty) return;
    setSaving(true);
    call<{ status: string; message?: string }>("save_file", {
      resourceId,
      content,
    })
      .then((res) => {
        if (res.status === "saved") {
          setOriginal(content);
        } else if (res.status === "conflict") {
          setError(res.message ?? "文件已被外部修改，存在冲突");
        }
      })
      .catch((e) => setError(String(e)))
      .finally(() => setSaving(false));
  }, [resourceId, content, dirty]);

  // Ctrl+S 保存
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        save();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [save]);

  if (loading) {
    return (
      <div className="wb-editor-loading">
        <Loader2 size={20} className="spin" />
        <span>加载中…</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="wb-editor-error">
        <AlertCircle size={20} />
        <span>{error}</span>
      </div>
    );
  }

  return (
    <div className="wb-editor">
      <div className="wb-editor-toolbar">
        <span className="wb-editor-file" title={filePath}>
          {fileName}
        </span>
        {dirty && <span className="wb-editor-dirty" title="有未保存的修改" />}
        <button
          className="wb-editor-save"
          disabled={!dirty || saving}
          onClick={save}
          title="Ctrl+S"
        >
          {saving ? <Loader2 size={13} className="spin" /> : <Save size={13} />}
          {saving ? "保存中…" : "保存"}
        </button>
      </div>
      <CodeMirror
        value={content}
        onChange={setContent}
        extensions={extensions}
        indentWithTab
        basicSetup={{
          tabSize,
          lineNumbers: true,
          highlightActiveLine: true,
          foldGutter: true,
        }}
        style={{ height: "100%", fontSize: `${fontSize}px` }}
      />
    </div>
  );
}
