import { useState } from "react";
import { Plus, Trash2, GripVertical } from "lucide-react";
import { usePageStore, blockText, type PageBlock } from "../stores/pageStore";

/** 块类型选项。 */
const BLOCK_TYPES = [
  { type: "paragraph", label: "正文" },
  { type: "heading", label: "标题" },
  { type: "bullet", label: "列表" },
  { type: "numbered", label: "编号" },
  { type: "quote", label: "引用" },
  { type: "code", label: "代码" },
] as const;

type BlockType = (typeof BLOCK_TYPES)[number]["type"];

function makeBlock(type: string, text = ""): PageBlock {
  return {
    id: crypto.randomUUID(),
    page_id: "",
    parent_block_id: null,
    block_type: type,
    block_order: 0,
    content_json: JSON.stringify({ text }),
    plain_text: text,
    created_at: Date.now() / 1000,
    updated_at: Date.now() / 1000,
  };
}

export function PageEditor() {
  const { detail, blocks, setBlocks, save } = usePageStore();
  const [saving, setSaving] = useState(false);

  if (!detail) return null;

  const updateText = (id: string, text: string) => {
    setBlocks(
      blocks.map((b) =>
        b.id === id
          ? {
              ...b,
              content_json: JSON.stringify({ text }),
              plain_text: text,
            }
          : b,
      ),
    );
  };

  const updateType = (id: string, type: BlockType) => {
    setBlocks(
      blocks.map((b) =>
        b.id === id
          ? { ...b, block_type: type }
          : b,
      ),
    );
  };

  const addBlock = (afterId: string) => {
    const idx = blocks.findIndex((b) => b.id === afterId);
    const next = [...blocks];
    next.splice(idx + 1, 0, makeBlock("paragraph"));
    setBlocks(next);
  };

  const removeBlock = (id: string) => {
    setBlocks(blocks.filter((b) => b.id !== id));
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await save();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page-editor">
      <div className="editor-toolbar">
        <input
          className="editor-title"
          defaultValue={detail.resource.name}
          onBlur={(e) => {
            const name = e.target.value.trim();
            if (name && name !== detail.resource.name) {
              usePageStore.getState().renamePage(detail.resource.id, name);
            }
          }}
        />
        <button className="btn btn-primary" disabled={saving} onClick={handleSave}>
          {saving ? "保存中…" : "保存"}
        </button>
      </div>

      <div className="editor-canvas">
        {blocks.length === 0 && (
          <button
            className="editor-empty-hint"
            onClick={() => setBlocks([makeBlock("paragraph")])}
          >
            <Plus size={14} /> 开始书写…
          </button>
        )}

        {blocks.map((b) => (
          <div key={b.id} className="editor-block">
            <div className="block-actions">
              <GripVertical size={13} className="block-drag" />
              <select
                className="block-type"
                value={b.block_type}
                onChange={(e) => updateType(b.id, e.target.value as BlockType)}
                title="块类型"
              >
                {BLOCK_TYPES.map((t) => (
                  <option key={t.type} value={t.type}>
                    {t.label}
                  </option>
                ))}
              </select>
              <button className="icon-btn" title="添加块" onClick={() => addBlock(b.id)}>
                <Plus size={13} />
              </button>
              <button className="icon-btn danger" title="删除块" onClick={() => removeBlock(b.id)}>
                <Trash2 size={13} />
              </button>
            </div>
            <textarea
              className={`editor-input block-${b.block_type}`}
              value={blockText(b)}
              placeholder={b.block_type === "code" ? "输入代码…" : "输入内容…"}
              onChange={(e) => updateText(b.id, e.target.value)}
              rows={b.block_type === "code" ? 5 : 2}
              spellCheck={false}
            />
          </div>
        ))}
      </div>
    </div>
  );
}
