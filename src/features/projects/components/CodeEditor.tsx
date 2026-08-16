import { useCallback, useEffect, useMemo, useState } from "react";
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
import { Save, X, AlertTriangle, RotateCcw } from "lucide-react";
import { useEditorStore } from "../stores/editorStore";
import { ConfirmDialog } from "../../../components/ConfirmDialog";

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

export function CodeEditor() {
  const { openFile, content, dirty, saving, conflict, openError, pendingDraft, setContent, save, forceSave, close, resolveClose, cancelPending, clearError, resolveDraft } =
    useEditorStore();
  const [showUnsaved, setShowUnsaved] = useState(false);

  const extensions = useMemo(
    () => [langFor(openFile?.resource.name ?? "")],
    [openFile?.resource.name],
  );

  const handleSave = useCallback(() => {
    save();
  }, [save]);

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

  if (!openFile) {
    return (
      <div className="empty-state" style={{ height: "100%" }}>
        <span>选择左侧文件开始编辑</span>
      </div>
    );
  }

  return (
    <div className="code-editor">
      {openError && (
        <div className="editor-error-bar">
          <span>{openError}</span>
          <button className="icon-btn" onClick={clearError} title="关闭">
            <X size={13} />
          </button>
        </div>
      )}
      <div className="editor-tabbar">
        <span className="tab-file" title={openFile.path}>
          {openFile.resource.name}
        </span>
        {dirty && <span className="dirty-dot" title="有未保存的修改" />}
        <div className="tab-actions">
          <button
            className="btn btn-ghost"
            disabled={!dirty || saving}
            onClick={handleSave}
            title="Ctrl+S"
          >
            <Save size={13} /> {saving ? "保存中…" : "保存"}
          </button>
          <button
            className="icon-btn"
            title="关闭"
            onClick={() => {
              void close().then((res) => {
                if (res === "confirm") setShowUnsaved(true);
              });
            }}
          >
            <X size={14} />
          </button>
        </div>
      </div>
      <div className="code-mirror-wrap">
        <CodeMirror
          value={content}
          onChange={setContent}
          extensions={extensions}
          height="100%"
          style={{ height: "100%" }}
          basicSetup={{
            lineNumbers: true,
            highlightActiveLine: true,
            foldGutter: true,
            autocompletion: true,
            searchKeymap: true,
          }}
        />
      </div>

      {conflict && (
        <div className="modal-mask">
          <div className="modal">
            <div className="modal-head">
              <h3 style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <AlertTriangle size={16} color="var(--warning)" /> 文件冲突
              </h3>
            </div>
            <p style={{ margin: 0, fontSize: 13, color: "var(--text-secondary)" }}>
              {conflict.message}。磁盘上的文件已改变，继续保存将覆盖外部修改。
            </p>
            <div className="modal-actions">
              <button className="btn" onClick={() => void resolveClose(false)}>
                <RotateCcw size={13} /> 放弃修改
              </button>
              <button
                className="btn btn-danger"
                onClick={async () => {
                  await forceSave();
                }}
                disabled={saving}
              >
                覆盖保存
              </button>
            </div>
          </div>
        </div>
      )}

      {showUnsaved && (
        <ConfirmDialog
          title="未保存的修改"
          message="当前文件有未保存的修改，关闭前是否保存？"
          onSave={() => {
            void resolveClose(true).then(() => setShowUnsaved(false));
          }}
          onDiscard={() => {
            void resolveClose(false).then(() => setShowUnsaved(false));
          }}
          onCancel={() => {
            cancelPending();
            setShowUnsaved(false);
          }}
        />
      )}

      {pendingDraft && (
        <ConfirmDialog
          title="发现未保存的草稿"
          message="检测到上次编辑未保存的内容，是否恢复草稿继续编辑？"
          onSave={() => resolveDraft(true)}
          onDiscard={() => resolveDraft(false)}
          onCancel={() => resolveDraft(false)}
        />
      )}
    </div>
  );
}
