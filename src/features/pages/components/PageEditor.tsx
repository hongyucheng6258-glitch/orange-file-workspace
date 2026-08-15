import { useEffect, useState, type ReactNode } from "react";
import { useEditor, EditorContent, type Editor } from "@tiptap/react";
import { BubbleMenu } from "@tiptap/react/menus";
import StarterKit from "@tiptap/starter-kit";
import Underline from "@tiptap/extension-underline";
import { TextStyle, FontSize } from "@tiptap/extension-text-style";
import Color from "@tiptap/extension-color";
import Highlight from "@tiptap/extension-highlight";
import TextAlign from "@tiptap/extension-text-align";
import Image from "@tiptap/extension-image";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import { Table } from "@tiptap/extension-table";
import TableRow from "@tiptap/extension-table-row";
import TableHeader from "@tiptap/extension-table-header";
import TableCell from "@tiptap/extension-table-cell";
import Placeholder from "@tiptap/extension-placeholder";
import Link from "@tiptap/extension-link";
import {
  Bold,
  Italic,
  Underline as UnderlineIcon,
  Strikethrough,
  Highlighter,
  Palette,
  AlignLeft,
  AlignCenter,
  AlignRight,
  List,
  ListOrdered,
  ListTodo,
  Quote,
  Code2,
  Minus,
  Link2,
  Image as ImageIcon,
  Table2,
  Undo2,
  Redo2,
  Save,
} from "lucide-react";
import { usePageStore, type DocNode } from "../stores/pageStore";

/** 通用工具栏按钮。 */
function ToolBtn({
  active,
  onClick,
  title,
  children,
}: {
  active?: boolean;
  onClick: () => void;
  title: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={`tool-btn ${active ? "active" : ""}`}
      title={title}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

/** 分隔线。 */
function ToolSep() {
  return <span className="tool-sep" />;
}

/** 把二进制转为 data URL 用的 base64（分块避免栈溢出）。 */
function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

const IMAGE_FILTERS = [
  { name: "图片", extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"] },
];

/** 从本地文件插入图片（转为 base64 持久保存）。 */
async function insertLocalImage(editor: Editor) {
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const { readFile } = await import("@tauri-apps/plugin-fs");
    const file = await open({ multiple: false, filters: IMAGE_FILTERS });
    if (typeof file !== "string") return;
    const bytes = await readFile(file);
    const ext = file.split(".").pop()?.toLowerCase() ?? "png";
    const mime = ext === "jpg" ? "jpeg" : ext === "svg" ? "svg+xml" : ext;
    const base64 = bytesToBase64(bytes);
    editor
      .chain()
      .focus()
      .setImage({ src: `data:image/${mime};base64,${base64}`, alt: "" })
      .run();
  } catch {
    // 用户取消或读取失败时静默忽略
  }
}

/** 插入链接。 */
function insertLink(editor: Editor) {
  const prev = editor.getAttributes("link").href as string | undefined;
  const url = window.prompt("链接地址", prev ?? "https://");
  if (url === null) return;
  if (url.trim() === "") {
    editor.chain().focus().extendMarkRange("link").unsetLink().run();
    return;
  }
  editor.chain().focus().extendMarkRange("link").setLink({ href: url.trim() }).run();
}

const TEXT_COLORS = ["#e11d48", "#ea580c", "#ca8a04", "#16a34a", "#0d9488", "#2563eb", "#7c3aed", "#0f172a"];

export function PageEditor() {
  const detail = usePageStore((s) => s.detail);
  const document = usePageStore((s) => s.document);
  const setDocument = usePageStore((s) => s.setDocument);
  const save = usePageStore((s) => s.save);
  const saving = usePageStore((s) => s.saving);
  const [colorPicker, setColorPicker] = useState(false);
  const [tablePicker, setTablePicker] = useState(false);
  const [gridRow, setGridRow] = useState(1);
  const [gridCol, setGridCol] = useState(1);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [1, 2, 3] },
      }),
      Underline,
      TextStyle,
      FontSize,
      Color,
      Highlight.configure({ multicolor: true }),
      TextAlign.configure({ types: ["heading", "paragraph"] }),
      Image.configure({ inline: false }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Table.configure({ resizable: true }),
      TableRow,
      TableHeader,
      TableCell,
      Link.configure({ openOnClick: false, autolink: true }),
      Placeholder.configure({ placeholder: "开始书写…" }),
    ],
    content: document,
    onUpdate: ({ editor: ed }) => {
      setDocument(ed.getJSON() as DocNode);
    },
  });

  // 切换页面时加载对应文档
  useEffect(() => {
    if (!editor || !detail) return;
    editor.commands.setContent(document);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail?.resource.id]);

  if (!detail) return null;

  const headingSelect = () => {
    if (!editor) return "p";
    const { level } = editor.getAttributes("heading") as { level?: number };
    return level ? String(level) : "p";
  };

  const fontSizeValue = () => {
    if (!editor) return "";
    const attrs = editor.getAttributes("textStyle") as { fontSize?: string };
    return attrs.fontSize ?? "";
  };

  return (
    <div className="page-editor">
      <div className="editor-toolbar">
        <select
          className="tool-select"
          value={headingSelect()}
          title="块类型"
          onChange={(e) => {
            if (!editor) return;
            const v = e.target.value;
            if (v === "p") editor.chain().focus().setParagraph().run();
            else editor.chain().focus().toggleHeading({ level: Number(v) as 1 | 2 | 3 }).run();
          }}
        >
          <option value="p">正文</option>
          <option value="1">标题 1</option>
          <option value="2">标题 2</option>
          <option value="3">标题 3</option>
        </select>

        <select
          className="tool-select font-size-select"
          title="字号"
          value={fontSizeValue()}
          onChange={(e) => {
            if (!editor) return;
            const v = e.target.value;
            if (v) editor.chain().focus().setFontSize(v).run();
            else editor.chain().focus().unsetFontSize().run();
          }}
        >
          <option value="">字号</option>
          <option value="12px">12</option>
          <option value="14px">14</option>
          <option value="16px">16</option>
          <option value="18px">18</option>
          <option value="20px">20</option>
          <option value="24px">24</option>
          <option value="28px">28</option>
          <option value="32px">32</option>
          <option value="36px">36</option>
        </select>

        <ToolSep />

        <ToolBtn title="加粗 (Ctrl+B)" active={editor?.isActive("bold")} onClick={() => editor?.chain().focus().toggleBold().run()}>
          <Bold size={15} />
        </ToolBtn>
        <ToolBtn title="斜体 (Ctrl+I)" active={editor?.isActive("italic")} onClick={() => editor?.chain().focus().toggleItalic().run()}>
          <Italic size={15} />
        </ToolBtn>
        <ToolBtn title="下划线 (Ctrl+U)" active={editor?.isActive("underline")} onClick={() => editor?.chain().focus().toggleUnderline().run()}>
          <UnderlineIcon size={15} />
        </ToolBtn>
        <ToolBtn title="删除线" active={editor?.isActive("strike")} onClick={() => editor?.chain().focus().toggleStrike().run()}>
          <Strikethrough size={15} />
        </ToolBtn>
        <ToolBtn title="荧光笔" active={editor?.isActive("highlight")} onClick={() => editor?.chain().focus().toggleHighlight().run()}>
          <Highlighter size={15} />
        </ToolBtn>

        <ToolSep />

        <ToolBtn title="左对齐" active={editor?.isActive({ textAlign: "left" })} onClick={() => editor?.chain().focus().setTextAlign("left").run()}>
          <AlignLeft size={15} />
        </ToolBtn>
        <ToolBtn title="居中对齐" active={editor?.isActive({ textAlign: "center" })} onClick={() => editor?.chain().focus().setTextAlign("center").run()}>
          <AlignCenter size={15} />
        </ToolBtn>
        <ToolBtn title="右对齐" active={editor?.isActive({ textAlign: "right" })} onClick={() => editor?.chain().focus().setTextAlign("right").run()}>
          <AlignRight size={15} />
        </ToolBtn>

        <ToolSep />

        <ToolBtn title="无序列表" active={editor?.isActive("bulletList")} onClick={() => editor?.chain().focus().toggleBulletList().run()}>
          <List size={15} />
        </ToolBtn>
        <ToolBtn title="有序列表" active={editor?.isActive("orderedList")} onClick={() => editor?.chain().focus().toggleOrderedList().run()}>
          <ListOrdered size={15} />
        </ToolBtn>
        <ToolBtn title="任务清单" active={editor?.isActive("taskList")} onClick={() => editor?.chain().focus().toggleTaskList().run()}>
          <ListTodo size={15} />
        </ToolBtn>

        <ToolSep />

        <ToolBtn title="引用" active={editor?.isActive("blockquote")} onClick={() => editor?.chain().focus().toggleBlockquote().run()}>
          <Quote size={15} />
        </ToolBtn>
        <ToolBtn title="代码块" active={editor?.isActive("codeBlock")} onClick={() => editor?.chain().focus().toggleCodeBlock().run()}>
          <Code2 size={15} />
        </ToolBtn>
        <ToolBtn title="分割线" onClick={() => editor?.chain().focus().setHorizontalRule().run()}>
          <Minus size={15} />
        </ToolBtn>

        <ToolSep />

        <ToolBtn title="链接" active={editor?.isActive("link")} onClick={() => editor && insertLink(editor)}>
          <Link2 size={15} />
        </ToolBtn>
        <ToolBtn title="插入图片" onClick={() => editor && insertLocalImage(editor)}>
          <ImageIcon size={15} />
        </ToolBtn>
        <div className="tool-table">
          <ToolBtn title="插入表格" active={tablePicker} onClick={() => setTablePicker((v) => !v)}>
            <Table2 size={15} />
          </ToolBtn>
          {tablePicker && (
            <div className="table-picker" onMouseDown={(e) => e.preventDefault()}>
              <div className="table-picker-grid">
                {Array.from({ length: 8 * 8 }, (_, i) => {
                  const r = Math.floor(i / 8) + 1;
                  const c = (i % 8) + 1;
                  const on = r <= gridRow && c <= gridCol;
                  return (
                    <button
                      key={i}
                      type="button"
                      className={`tp-cell ${on ? "on" : ""}`}
                      onMouseEnter={() => {
                        setGridRow(r);
                        setGridCol(c);
                      }}
                      onClick={() => {
                        editor
                          ?.chain()
                          .focus()
                          .insertTable({ rows: r, cols: c, withHeaderRow: true })
                          .run();
                        setTablePicker(false);
                      }}
                    />
                  );
                })}
              </div>
              <div className="table-picker-label">
                {gridRow} 行 × {gridCol} 列
              </div>
            </div>
          )}
        </div>

        <div className="tool-color">
          <button
            type="button"
            className={`tool-btn ${colorPicker ? "active" : ""}`}
            title="文字颜色"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setColorPicker((v) => !v)}
          >
            <Palette size={15} />
          </button>
          {colorPicker && (
            <div className="tool-color-panel" onMouseDown={(e) => e.preventDefault()}>
              {TEXT_COLORS.map((c) => (
                <button
                  key={c}
                  type="button"
                  className="color-swatch"
                  style={{ background: c }}
                  title={c}
                  onClick={() => {
                    editor?.chain().focus().setColor(c).run();
                    setColorPicker(false);
                  }}
                />
              ))}
              <button
                type="button"
                className="color-swatch color-clear"
                title="清除颜色"
                onClick={() => {
                  editor?.chain().focus().unsetColor().run();
                  setColorPicker(false);
                }}
              >
                无
              </button>
            </div>
          )}
        </div>

        <span className="toolbar-spacer" />

        <ToolBtn title="撤销" onClick={() => editor?.chain().focus().undo().run()}>
          <Undo2 size={15} />
        </ToolBtn>
        <ToolBtn title="重做" onClick={() => editor?.chain().focus().redo().run()}>
          <Redo2 size={15} />
        </ToolBtn>

        <button
          type="button"
          className="btn btn-primary toolbar-save"
          disabled={saving}
          onClick={async () => {
            await save();
          }}
        >
          <Save size={14} /> {saving ? "保存中…" : "保存"}
        </button>
      </div>

      <div className="editor-scroll">
        <div className="editor-paper">
          <input
            className="editor-title"
            defaultValue={detail.resource.name}
            placeholder="页面标题"
            onBlur={(e) => {
              const name = e.target.value.trim();
              if (name && name !== detail.resource.name) {
                usePageStore.getState().renamePage(detail.resource.id, name);
              }
            }}
          />
          <EditorContent editor={editor} />
        </div>
      </div>

      {editor && (
        <BubbleMenu
          editor={editor}
          className="bubble-toolbar"
        >
          <select
            className="bubble-font-size"
            title="字号"
            value={fontSizeValue()}
            onChange={(e) => {
              const v = e.target.value;
              if (v) editor.chain().focus().setFontSize(v).run();
              else editor.chain().focus().unsetFontSize().run();
            }}
          >
            <option value="">字号</option>
            <option value="12px">12</option>
            <option value="14px">14</option>
            <option value="16px">16</option>
            <option value="18px">18</option>
            <option value="20px">20</option>
            <option value="24px">24</option>
            <option value="28px">28</option>
            <option value="32px">32</option>
            <option value="36px">36</option>
          </select>
          <ToolSep />
          <ToolBtn title="加粗" active={editor.isActive("bold")} onClick={() => editor.chain().focus().toggleBold().run()}>
            <Bold size={14} />
          </ToolBtn>
          <ToolBtn title="斜体" active={editor.isActive("italic")} onClick={() => editor.chain().focus().toggleItalic().run()}>
            <Italic size={14} />
          </ToolBtn>
          <ToolBtn title="下划线" active={editor.isActive("underline")} onClick={() => editor.chain().focus().toggleUnderline().run()}>
            <UnderlineIcon size={14} />
          </ToolBtn>
          <ToolBtn title="删除线" active={editor.isActive("strike")} onClick={() => editor.chain().focus().toggleStrike().run()}>
            <Strikethrough size={14} />
          </ToolBtn>
          <ToolBtn title="荧光笔" active={editor.isActive("highlight")} onClick={() => editor.chain().focus().toggleHighlight().run()}>
            <Highlighter size={14} />
          </ToolBtn>
          <ToolSep />
          <ToolBtn title="链接" active={editor.isActive("link")} onClick={() => insertLink(editor)}>
            <Link2 size={14} />
          </ToolBtn>
          <ToolBtn title="清除链接" onClick={() => editor.chain().focus().extendMarkRange("link").unsetLink().run()}>
            <Link2 size={14} className="link-off" />
          </ToolBtn>
          <label className="bubble-color">
            <Palette size={14} />
            <input
              type="color"
              title="文字颜色"
              onChange={(e) => editor.chain().focus().setColor(e.target.value).run()}
            />
          </label>
        </BubbleMenu>
      )}

      {editor && (
        <BubbleMenu
          editor={editor}
          shouldShow={({ editor: ed }) => ed.isActive("table")}
          className="bubble-toolbar table-menu"
        >
          <button
            type="button"
            className="table-menu-btn"
            title="在上方插入行"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().addRowBefore().run()}
          >
            行+上
          </button>
          <button
            type="button"
            className="table-menu-btn"
            title="在下方插入行"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().addRowAfter().run()}
          >
            行+下
          </button>
          <button
            type="button"
            className="table-menu-btn"
            title="在左侧插入列"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().addColumnBefore().run()}
          >
            列+左
          </button>
          <button
            type="button"
            className="table-menu-btn"
            title="在右侧插入列"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().addColumnAfter().run()}
          >
            列+右
          </button>
          <ToolSep />
          <button
            type="button"
            className="table-menu-btn"
            title="删除当前行"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().deleteRow().run()}
          >
            删行
          </button>
          <button
            type="button"
            className="table-menu-btn"
            title="删除当前列"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().deleteColumn().run()}
          >
            删列
          </button>
          <ToolSep />
          <button
            type="button"
            className="table-menu-btn"
            title="合并选中的单元格"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().mergeCells().run()}
          >
            合并
          </button>
          <button
            type="button"
            className="table-menu-btn"
            title="拆分单元格"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().splitCell().run()}
          >
            拆分
          </button>
          <ToolSep />
          <button
            type="button"
            className="table-menu-btn danger"
            title="删除整个表格"
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => editor.chain().focus().deleteTable().run()}
          >
            删表
          </button>
        </BubbleMenu>
      )}
    </div>
  );
}
