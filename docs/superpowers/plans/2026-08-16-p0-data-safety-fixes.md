# P0 数据安全修复实施计划（第一阶段）

日期：2026-08-16
目标：修复审查确认的四个 P0 数据安全缺陷，全部采用 TDD（先写失败测试，再实现，最后提交）。

## 执行前提说明

当前工作区 `E:\work\新建文件夹` 有大量未提交改动（45 个已跟踪文件修改 + 大量未跟踪文件），无法迁移到 git worktree。本计划直接在当前 master 工作区执行，每次提交只 `git add` 计划涉及的文件，绝不 `git add -A`，避免把用户未提交的改动混入。

## 任务总览

| 任务 | 问题 | 涉及文件 |
|---|---|---|
| Task 1 | 编辑 >256KiB 文件保存后截断原文件 | `src-tauri/src/services/preview_service.rs`、`editor_service.rs`、前端 `editorStore.ts`、`CodeEditor.tsx` |
| Task 2 | 切换文件/页面/关闭编辑器静默丢弃未保存修改 | `pageStore.ts`、`editorStore.ts`、`src/components/ConfirmDialog.tsx`（新建）、`PagePage.tsx`、`ProjectPage.tsx`、`CodeEditor.tsx` |
| Task 3 | 备份恢复失败时数据库与文件不一致 | `src-tauri/src/services/backup_service.rs` |
| Task 4 | 全盘索引断点恢复漏掉目录 | `src-tauri/src/services/scan_service.rs` |

---

## Task 1：大文件编辑截断修复

### 根因

`editor_service.rs::open_session`（约 32 行）用 `preview_service::read_text_preview(path, 256 * 1024)` 读取，该函数只读前 256KiB 并追加"内容过长"提示；`save_session` 保存时把这段截断内容作为完整文件替换写回，256KiB 之后的内容永久丢失。

### 修复设计

1. `preview_service.rs` 新增 `read_text_full(path, max_bytes)`：完整读取 UTF-8 文本，超过上限返回 `file_too_large` 错误（不截断、不加提示）。编辑读取与预览读取彻底分离。
2. `editor_service.rs` 定义 `pub const EDITOR_MAX_BYTES: u64 = 10 * 1024 * 1024;`（10MiB），`open_session` 改用 `read_text_full`。
3. `save_session` 增加防御：若会话基准大小超过上限（历史遗留截断会话），拒绝保存并提示重新打开。
4. 前端 `editorStore.open` 捕获 `file_too_large`，设置 `openError` 展示只读提示，不清空当前编辑状态。

### Step 1.1 写失败测试（Rust）

在 `src-tauri/src/services/preview_service.rs` 的 `mod tests` 末尾追加：

```rust
#[test]
fn read_text_full_returns_entire_file() {
    let tmp = std::env::temp_dir().join(format!("nexus-full-{}", crate::db::models::new_id()));
    // 300KiB，超过原 256KiB 预览上限
    let content = vec![b'a'; 300 * 1024];
    std::fs::write(&tmp, &content).expect("write");

    let text = read_text_full(&tmp, 10 * 1024 * 1024).expect("read");
    assert_eq!(text.len(), 300 * 1024, "必须返回完整内容");
    assert!(!text.contains("内容过长"), "不应包含截断提示");

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn read_text_full_rejects_oversize() {
    let tmp = std::env::temp_dir().join(format!("nexus-over-{}", crate::db::models::new_id()));
    std::fs::write(&tmp, vec![b'a'; 2048]).expect("write");

    let err = read_text_full(&tmp, 1024).expect_err("should reject");
    assert_eq!(err.code, "file_too_large");

    let _ = std::fs::remove_file(&tmp);
}
```

在 `src-tauri/src/services/editor_service.rs` 的 `mod tests` 末尾追加：

```rust
#[test]
fn open_reads_full_content_beyond_preview_limit() {
    let conn = conn();
    seed_resource(&conn, "r1");
    let path = std::env::temp_dir().join(format!("nexus-edbig-{}", crate::db::models::new_id()));
    let content = vec![b'x'; 300 * 1024]; // 超过 256KiB 预览上限
    std::fs::write(&path, &content).expect("write");

    let (text, _session) = open_session(&conn, "r1", &path).expect("open");
    assert_eq!(text.len(), 300 * 1024, "编辑会话必须包含完整内容");
    assert!(!text.contains("内容过长"), "编辑内容不应带截断提示");

    // 保存后磁盘内容完整
    let outcome = save_session(&conn, "r1", &path, &text, false).expect("save");
    assert!(matches!(outcome, SaveOutcome::Saved));
    let on_disk = std::fs::read_to_string(&path).expect("read");
    assert_eq!(on_disk.len(), 300 * 1024, "保存后原文件必须保持完整");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn open_rejects_file_over_edit_limit() {
    let conn = conn();
    seed_resource(&conn, "r1");
    let path = std::env::temp_dir().join(format!("nexus-edhuge-{}", crate::db::models::new_id()));
    std::fs::write(&path, vec![b'x'; (EDITOR_MAX_BYTES + 1) as usize]).expect("write");

    let err = open_session(&conn, "r1", &path).expect_err("should reject");
    assert_eq!(err.code, "file_too_large");

    let _ = std::fs::remove_file(&path);
}
```

### Step 1.2 运行确认失败

```powershell
cd src-tauri ; cargo test preview_service read_text_full
cd src-tauri ; cargo test editor_service open_reads_full
cd src-tauri ; cargo test editor_service open_rejects_file_over
```

预期：编译错误（函数不存在）或测试失败。

### Step 1.3 实现后端

`src-tauri/src/services/preview_service.rs`，在 `read_text_preview` 之后新增：

```rust
/// 完整读取 UTF-8 文本文件，供编辑器使用。
/// 与 read_text_preview 不同：不截断、不追加提示；超过 max_bytes 返回 file_too_large。
pub fn read_text_full(path: &Path, max_bytes: u64) -> Result<String, AppError> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > max_bytes {
        return Err(AppError::new(
            "file_too_large",
            format!("文件过大（{} 字节），超过编辑上限 {}", meta.len(), max_bytes),
        ));
    }
    let mut file = std::fs::File::open(path)?;
    let mut buf = Vec::with_capacity(meta.len() as usize);
    file.read_to_end(&mut buf)?;

    let bytes = if buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &buf[3..]
    } else {
        &buf[..]
    };
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => Err(AppError::new("not_utf8", "文件不是 UTF-8 文本")),
    }
}
```

`src-tauri/src/services/editor_service.rs` 修改：

```rust
/// 编辑器可处理的单文件大小上限（10 MiB）。
pub const EDITOR_MAX_BYTES: u64 = 10 * 1024 * 1024;
```

`open_session` 中替换：

```rust
let content = preview_service::read_text_full(path, EDITOR_MAX_BYTES)?;
```

`save_session` 中，在 `let existing = get_session(conn, resource_id)?;` 之后、`let (size, modified) = fsutil::stat_basic(path)?;` 之前插入防御：

```rust
// 防御：历史遗留的截断会话（基准大小超限）拒绝保存，避免覆盖造成数据丢失
if existing.as_ref().is_some_and(|s| s.base_size > EDITOR_MAX_BYTES as i64) {
    return Err(AppError::new(
        "file_too_large",
        "该文件超出编辑上限，请重新打开后操作",
    ));
}
```

### Step 1.4 运行确认通过

```powershell
cd src-tauri ; cargo test preview_service
cd src-tauri ; cargo test editor_service
```

预期：全部通过。

### Step 1.5 前端错误提示

`src/features/projects/stores/editorStore.ts` 修改接口与实现：

```ts
interface EditorState {
  openFile: OpenFile | null;
  content: string;
  dirty: boolean;
  saving: boolean;
  conflict: { message: string; current_size: number } | null;
  openError: string | null;

  open: (resourceId: string) => Promise<void>;
  setContent: (content: string) => void;
  save: () => Promise<"saved" | "conflict">;
  forceSave: () => Promise<void>;
  close: () => Promise<void>;
  clearError: () => void;
}

// store 初始化部分
openError: null,

open: async (resourceId) => {
  set({ openError: null });
  try {
    const data = await call<OpenFile & { session: unknown }>("open_file", { resourceId });
    set({
      openFile: { resource: data.resource, content: data.content, path: data.path },
      content: data.content,
      dirty: false,
      conflict: null,
    });
  } catch (e) {
    const err = e as { code?: string; message?: string };
    if (err.code === "file_too_large") {
      set({ openError: err.message ?? "文件过大，仅支持预览" });
      return;
    }
    throw e;
  }
},

clearError: () => set({ openError: null }),
```

`src/features/projects/components/CodeEditor.tsx`，在 `return` 之前（`if (!openFile)` 之后）插入错误条渲染：

```tsx
const { openError, clearError } = useEditorStore();

// 在组件 JSX 顶部（.code-editor 内第一行）：
{openError && (
  <div className="editor-error-bar">
    <span>{openError}</span>
    <button className="icon-btn" onClick={clearError} title="关闭">
      <X size={13} />
    </button>
  </div>
)}
```

从组件解构中补充 `openError, clearError`。`editor-error-bar` 样式在 `src/styles/app.css` 末尾追加：

```css
.editor-error-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 12px;
  font-size: 12px;
  color: #b42318;
  background: #fef3f2;
  border-bottom: 1px solid #fecdca;
}
```

### Step 1.6 前端测试

新建 `src/features/projects/stores/editorStore.test.ts`：

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockCall = vi.fn();
vi.mock("../../../lib/tauri", () => ({
  call: (...args: unknown[]) => mockCall(...args),
}));

import { useEditorStore } from "./editorStore";

const fileData = {
  resource: { id: "f1", kind: "file", name: "a.txt", created_at: 1, updated_at: 1 },
  content: "v1",
  session: {},
  path: "C:\\a.txt",
};

beforeEach(() => {
  mockCall.mockReset();
  mockCall.mockImplementation((cmd: string) => {
    if (cmd === "open_file") return Promise.resolve(fileData);
    if (cmd === "save_file") return Promise.resolve({ status: "saved" });
    if (cmd === "save_file_force") return Promise.resolve({ status: "saved" });
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  useEditorStore.setState({
    openFile: null,
    content: "",
    dirty: false,
    saving: false,
    conflict: null,
    openError: null,
  });
});

describe("editorStore", () => {
  it("open loads content and clears error", async () => {
    await useEditorStore.getState().open("f1");
    const s = useEditorStore.getState();
    expect(s.openFile?.content).toBe("v1");
    expect(s.dirty).toBe(false);
  });

  it("open sets openError when file too large", async () => {
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "open_file") return Promise.reject({ code: "file_too_large", message: "文件过大" });
      return Promise.resolve(undefined);
    });
    await useEditorStore.getState().open("f1");
    expect(useEditorStore.getState().openError).toBe("文件过大");
    expect(useEditorStore.getState().openFile).toBeNull();
  });
});
```

### Step 1.7 运行前端测试

```powershell
npx vitest run src/features/projects/stores/editorStore.test.ts
```

### Step 1.8 提交

```powershell
git add src-tauri/src/services/preview_service.rs src-tauri/src/services/editor_service.rs src/features/projects/stores/editorStore.ts src/features/projects/components/CodeEditor.tsx src/features/projects/stores/editorStore.test.ts src/styles/app.css
git commit -m "fix: 编辑器完整读取大文件，防止保存截断原文件"
```

---

## Task 2：未保存修改保护

### 根因

- `pageStore.openPage`（`pageStore.ts` 77 行）直接覆盖当前文档，不检查 `dirty`。
- `editorStore.open`（`editorStore.ts` 32 行）直接覆盖状态，不检查 `dirty`。
- `editorStore.close`（85 行）无确认丢弃会话。
- 触发点：`PagePage.tsx` 54 行树点击、`ProjectPage.tsx` 230 行 `onOpenFile`、`CodeEditor.tsx` 118 行关闭按钮。

### 修复设计（统一 pending 确认机制）

两个 store 都增加"待确认目标"状态，`open`/`close` 遇到 dirty 时不执行切换，返回 `"confirm"`，由 UI 显示 ConfirmDialog，用户选择保存/放弃/取消后再由 `resolveOpen` / `resolveClose` / `cancelOpen` 完成。

### Step 2.1 新建通用确认对话框

新建 `src/components/ConfirmDialog.tsx`：

```tsx
interface ConfirmDialogProps {
  title: string;
  message: string;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ title, message, onSave, onDiscard, onCancel }: ConfirmDialogProps) {
  return (
    <div className="modal-mask" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3 style={{ margin: "0 0 8px", fontSize: 14 }}>{title}</h3>
        <p style={{ margin: "0 0 16px", fontSize: 13, color: "#666" }}>{message}</p>
        <div className="modal-actions">
          <button className="btn" onClick={onCancel}>
            取消
          </button>
          <button className="btn" onClick={onDiscard}>
            放弃
          </button>
          <button className="btn btn-primary" onClick={onSave}>
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
```

### Step 2.2 写失败测试（pageStore）

新建 `src/features/pages/stores/pageStore.test.ts`：

```ts
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mockCall = vi.fn();
vi.mock("../../../lib/tauri", () => ({
  call: (...args: unknown[]) => mockCall(...args),
}));

import { EMPTY_DOC, usePageStore } from "./pageStore";

const detailP1 = {
  resource: { id: "p1", kind: "page", name: "P1", created_at: 1, updated_at: 1 },
  page: { resource_id: "p1", summary: null, content_version: 1, save_state: "clean", content_json: null },
};
const detailP2 = {
  resource: { id: "p2", kind: "page", name: "P2", created_at: 1, updated_at: 1 },
  page: { resource_id: "p2", summary: null, content_version: 1, save_state: "clean", content_json: null },
};

beforeEach(() => {
  mockCall.mockReset();
  mockCall.mockImplementation((cmd: string) => {
    if (cmd === "list_pages") return Promise.resolve([]);
    if (cmd === "get_page") return Promise.resolve(detailP1);
    if (cmd === "save_page_document") return Promise.resolve(undefined);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  usePageStore.setState({
    tree: [],
    currentPageId: null,
    detail: null,
    document: EMPTY_DOC,
    dirty: false,
    loading: false,
    saving: false,
    pendingTarget: null,
  });
});

describe("pageStore dirty guard", () => {
  it("openPage switches when clean", async () => {
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "get_page") return Promise.resolve(detailP2);
      return Promise.resolve([]);
    });
    const res = await usePageStore.getState().openPage("p2");
    expect(res).toBe("opened");
    expect(usePageStore.getState().currentPageId).toBe("p2");
  });

  it("openPage returns confirm and keeps current page when dirty", async () => {
    await usePageStore.getState().openPage("p1");
    usePageStore.getState().setDocument({ type: "doc", content: [{ type: "paragraph" }] });
    const res = await usePageStore.getState().openPage("p2");
    expect(res).toBe("confirm");
    expect(usePageStore.getState().currentPageId).toBe("p1");
    expect(usePageStore.getState().pendingTarget).toBe("p2");
  });

  it("resolveOpen(true) saves then opens pending target", async () => {
    await usePageStore.getState().openPage("p1");
    usePageStore.getState().setDocument({ type: "doc", content: [{ type: "paragraph" }] });
    await usePageStore.getState().openPage("p2");
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "get_page") return Promise.resolve(detailP2);
      return Promise.resolve(undefined);
    });
    await usePageStore.getState().resolveOpen(true);
    expect(mockCall).toHaveBeenCalledWith("save_page_document", expect.any(Object));
    expect(usePageStore.getState().currentPageId).toBe("p2");
    expect(usePageStore.getState().pendingTarget).toBeNull();
  });

  it("resolveOpen(false) discards and opens pending target", async () => {
    await usePageStore.getState().openPage("p1");
    usePageStore.getState().setDocument({ type: "doc", content: [{ type: "paragraph" }] });
    await usePageStore.getState().openPage("p2");
    mockCall.mockImplementation((cmd: string) => {
      if (cmd === "get_page") return Promise.resolve(detailP2);
      return Promise.resolve(undefined);
    });
    await usePageStore.getState().resolveOpen(false);
    expect(mockCall).not.toHaveBeenCalledWith("save_page_document", expect.any(Object));
    expect(usePageStore.getState().currentPageId).toBe("p2");
  });

  it("cancelOpen keeps current page and clears pending", async () => {
    await usePageStore.getState().openPage("p1");
    usePageStore.getState().setDocument({ type: "doc", content: [{ type: "paragraph" }] });
    await usePageStore.getState().openPage("p2");
    usePageStore.getState().cancelOpen();
    expect(usePageStore.getState().currentPageId).toBe("p1");
    expect(usePageStore.getState().pendingTarget).toBeNull();
  });
});
```

### Step 2.3 运行确认失败

```powershell
npx vitest run src/features/pages/stores/pageStore.test.ts
```

预期：类型/编译错误或断言失败（当前 store 没有 pendingTarget / resolveOpen / cancelOpen）。

### Step 2.4 实现 pageStore

`src/features/pages/stores/pageStore.ts` 修改：

```ts
interface PageState {
  tree: Resource[];
  currentPageId: string | null;
  detail: PageDetail | null;
  document: DocNode;
  dirty: boolean;
  loading: boolean;
  saving: boolean;
  pendingTarget: string | null;

  loadTree: () => Promise<void>;
  openPage: (id: string) => Promise<"opened" | "confirm">;
  createPage: (name: string, parentId?: string | null) => Promise<void>;
  renamePage: (id: string, name: string) => Promise<void>;
  deletePage: (id: string) => Promise<void>;
  setDocument: (doc: DocNode) => void;
  save: () => Promise<void>;
  resolveOpen: (save: boolean) => Promise<void>;
  cancelOpen: () => void;
}
```

实现体修改（`openPage`、新增 `resolveOpen`、`cancelOpen`，state 增加 `pendingTarget: null`）：

```ts
openPage: async (id) => {
  const { dirty, currentPageId } = get();
  if (dirty && currentPageId !== id) {
    set({ pendingTarget: id });
    return "confirm";
  }
  set({ loading: true, currentPageId: id });
  try {
    const detail = await call<PageDetail>("get_page", { resourceId: id });
    set({
      detail,
      document: parseDocument(detail.page.content_json),
      dirty: false,
      loading: false,
      pendingTarget: null,
    });
    return "opened";
  } catch (e) {
    set({ loading: false });
    throw e;
  }
},

resolveOpen: async (save) => {
  const { pendingTarget } = get();
  if (!pendingTarget) return;
  if (save) {
    await get().save();
    // 保存失败（异常）会中断，保持当前页面
  }
  await get().openPage(pendingTarget);
},

cancelOpen: () => set({ pendingTarget: null }),
```

注意：`renamePage` 内部 `openPage(id)` 返回值忽略即可（不 await 结果判断，现有逻辑不变）。`deletePage` 若删除当前页时 dirty，应在删除前先确认——本阶段保持现有二次点击确认（删除是显式操作），但删除前若 dirty 调用 `set({ dirty: false })` 丢弃内容可接受；为稳妥，删除当前 dirty 页面也走确认：删除前若 `dirty && currentPageId === id`，返回待确认。为控制范围，本阶段仅在 `deletePage` 内对删除当前 dirty 页面时先 `save()`：

```ts
deletePage: async (id) => {
  if (get().dirty && get().currentPageId === id) {
    await get().save();
  }
  await call("delete_page", { resourceId: id });
  // ...原有逻辑不变
},
```

### Step 2.5 实现 editorStore

`src/features/projects/stores/editorStore.ts` 修改接口：

```ts
interface EditorState {
  openFile: OpenFile | null;
  content: string;
  dirty: boolean;
  saving: boolean;
  conflict: { message: string; current_size: number } | null;
  openError: string | null;
  pendingOpenId: string | null;
  pendingClose: boolean;

  open: (resourceId: string) => Promise<"opened" | "confirm">;
  setContent: (content: string) => void;
  save: () => Promise<"saved" | "conflict">;
  forceSave: () => Promise<void>;
  close: () => Promise<"closed" | "confirm">;
  resolveOpen: (save: boolean) => Promise<void>;
  resolveClose: (save: boolean) => Promise<void>;
  cancelPending: () => void;
  clearError: () => void;
}
```

实现体修改：

```ts
open: async (resourceId) => {
  const { dirty } = get();
  if (dirty) {
    set({ pendingOpenId: resourceId });
    return "confirm";
  }
  set({ openError: null });
  try {
    const data = await call<OpenFile & { session: unknown }>("open_file", { resourceId });
    set({
      openFile: { resource: data.resource, content: data.content, path: data.path },
      content: data.content,
      dirty: false,
      conflict: null,
      pendingOpenId: null,
    });
    return "opened";
  } catch (e) {
    const err = e as { code?: string; message?: string };
    if (err.code === "file_too_large") {
      set({ openError: err.message ?? "文件过大，仅支持预览" });
      return "opened";
    }
    throw e;
  }
},

close: async () => {
  const { dirty, openFile } = get();
  if (dirty && openFile) {
    set({ pendingClose: true });
    return "confirm";
  }
  await get().discard();
  set({ openFile: null, content: "", dirty: false, conflict: null, pendingClose: false });
  return "closed";
},

resolveOpen: async (save) => {
  const { pendingOpenId } = get();
  if (!pendingOpenId) return;
  if (save) {
    const status = await get().save();
    if (status === "conflict") {
      set({ pendingOpenId: null });
      return;
    }
  }
  await get().open(pendingOpenId);
},

resolveClose: async (save) => {
  if (save) {
    const status = await get().save();
    if (status === "conflict") {
      set({ pendingClose: false });
      return;
    }
  }
  await get().close();
},

cancelPending: () => set({ pendingOpenId: null, pendingClose: false }),

// 抽取原 close 中的 discard 逻辑为内部方法（在 create 体内定义）
discard: async () => {
  const { openFile } = get();
  if (openFile) {
    try {
      await call("discard_session", { resourceId: openFile.resource.id });
    } catch {
      // 忽略
    }
  }
},
```

接口中不导出 `discard`（内部使用）。state 初始化增加 `pendingOpenId: null, pendingClose: false`。

### Step 2.6 写失败测试（editorStore dirty guard）

在 `src/features/projects/stores/editorStore.test.ts` 追加：

```ts
describe("editorStore dirty guard", () => {
  it("open returns confirm and keeps current file when dirty", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    const res = await useEditorStore.getState().open("f2");
    expect(res).toBe("confirm");
    expect(useEditorStore.getState().openFile?.resource.id).toBe("f1");
    expect(useEditorStore.getState().pendingOpenId).toBe("f2");
  });

  it("close returns confirm when dirty", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    const res = await useEditorStore.getState().close();
    expect(res).toBe("confirm");
    expect(useEditorStore.getState().pendingClose).toBe(true);
    expect(useEditorStore.getState().openFile).not.toBeNull();
  });

  it("resolveClose(false) discards and closes", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    await useEditorStore.getState().close();
    await useEditorStore.getState().resolveClose(false);
    expect(useEditorStore.getState().openFile).toBeNull();
    expect(useEditorStore.getState().pendingClose).toBe(false);
  });

  it("resolveClose(true) saves then closes", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    await useEditorStore.getState().close();
    await useEditorStore.getState().resolveClose(true);
    expect(mockCall).toHaveBeenCalledWith("save_file", expect.objectContaining({ resourceId: "f1" }));
    expect(useEditorStore.getState().openFile).toBeNull();
  });

  it("cancelPending keeps current state", async () => {
    await useEditorStore.getState().open("f1");
    useEditorStore.getState().setContent("changed");
    await useEditorStore.getState().open("f2");
    useEditorStore.getState().cancelPending();
    expect(useEditorStore.getState().pendingOpenId).toBeNull();
    expect(useEditorStore.getState().openFile?.resource.id).toBe("f1");
    expect(useEditorStore.getState().content).toBe("changed");
  });
});
```

`afterEach` 的 setState 增加 `pendingOpenId: null, pendingClose: false`。

### Step 2.7 运行前端测试

```powershell
npx vitest run src/features/pages/stores/pageStore.test.ts src/features/projects/stores/editorStore.test.ts
```

### Step 2.8 UI 接线

`src/features/pages/routes/PagePage.tsx`：

- 增加 state：`const [showUnsaved, setShowUnsaved] = useState(false);`
- 树点击（54 行处）：

```tsx
onClick={() => {
  setConfirmDeleteId(null);
  void openPage(r.id).then((res) => {
    if (res === "confirm") setShowUnsaved(true);
  });
}}
```

- 从 `usePageStore()` 解构增加 `pendingTarget, resolveOpen, cancelOpen`；`usePageStore.getState()` 引用改为解构调用。
- JSX 末尾（`</div>` 前）追加：

```tsx
{showUnsaved && (
  <ConfirmDialog
    title="未保存的修改"
    message="当前页面有未保存的修改，切换前是否保存？"
    onSave={() => {
      void resolveOpen(true).then(() => setShowUnsaved(false));
    }}
    onDiscard={() => {
      void resolveOpen(false).then(() => setShowUnsaved(false));
    }}
    onCancel={() => {
      cancelOpen();
      setShowUnsaved(false);
    }}
  />
)}
```

- 文件头部 import：`import { ConfirmDialog } from "../../../components/ConfirmDialog";`

`src/features/projects/routes/ProjectPage.tsx`：

- 增加 state：`const [showUnsaved, setShowUnsaved] = useState(false);`
- `onOpenFile={openFile}` 替换为包装函数：

```tsx
const handleOpenFile = useCallback(
  async (id: string) => {
    const res = await openFile(id);
    if (res === "confirm") setShowUnsaved(true);
  },
  [openFile],
);
```

`onOpenFile={handleOpenFile}`。

- 从 `useEditorStore` 解构增加 `pendingOpenId, resolveOpen, cancelPending`（ProjectPage 中当前只有 `openFile`，需补 `useEditorStore((s) => s.pendingOpenId)` 等）。
- JSX 末尾追加：

```tsx
{showUnsaved && (
  <ConfirmDialog
    title="未保存的修改"
    message="当前文件有未保存的修改，切换前是否保存？"
    onSave={() => {
      void resolveOpen(true).then(() => setShowUnsaved(false));
    }}
    onDiscard={() => {
      void resolveOpen(false).then(() => setShowUnsaved(false));
    }}
    onCancel={() => {
      cancelPending();
      setShowUnsaved(false);
    }}
  />
)}
```

- import `ConfirmDialog`、`useCallback`（已 import）。

`src/features/projects/components/CodeEditor.tsx`：

- 增加 state：`const [showUnsaved, setShowUnsaved] = useState(false);`（需 import `useState`）
- 关闭按钮（118 行）：

```tsx
onClick={() => {
  void close().then((res) => {
    if (res === "confirm") setShowUnsaved(true);
  });
}}
```

- 解构增加 `pendingClose, resolveClose, cancelPending`。
- JSX 末尾追加：

```tsx
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
```

- import `ConfirmDialog`。

### Step 2.9 类型检查与测试

```powershell
npx tsc --noEmit
npx vitest run src/features/pages/stores/pageStore.test.ts src/features/projects/stores/editorStore.test.ts
```

### Step 2.10 提交

```powershell
git add src/components/ConfirmDialog.tsx src/features/pages/stores/pageStore.ts src/features/pages/stores/pageStore.test.ts src/features/pages/routes/PagePage.tsx src/features/projects/stores/editorStore.ts src/features/projects/stores/editorStore.test.ts src/features/projects/routes/ProjectPage.tsx src/features/projects/components/CodeEditor.tsx
git commit -m "feat: 切换文件/页面与关闭编辑器前未保存修改确认"
```

---

## Task 3：备份恢复事务性

### 根因

`backup_service.rs::restore_from_dir`：先 `restore_db_snapshot` 恢复数据库，再恢复托管文件；文件恢复失败或校验不一致时（208、222 行）只回滚文件目录，不回滚数据库 → 数据库指向备份版本、磁盘保留恢复前文件，出现不一致。此外 206 行 `std::fs::rename` 失败用 `?` 直接返回，同样不回滚数据库。

### 修复设计

1. 抽取 `restore_managed_files` 内部函数：移动当前托管目录、复制备份文件、校验（文件数 + 总字节数）、任何失败都回滚文件和数据库。
2. 校验增强：`count_files` 改为 `count_files_and_bytes`，返回 `(count, total_bytes)`，源与目标同时比较。
3. `restore_from_dir` 第 3 步改用新函数；失败信息统一含"数据库已回滚"。

### Step 3.1 写失败测试（Rust）

在 `src-tauri/src/services/backup_service.rs` 的 `mod tests` 末尾追加：

```rust
#[test]
fn restore_managed_move_failure_rolls_back_db() {
    let (state, dir) = test_state();

    // 当前托管目录：有一个文件
    let managed = state.managed_dir.lock().expect("lock").clone();
    std::fs::create_dir_all(&managed).unwrap();
    std::fs::write(managed.join("current.txt"), "cur").unwrap();

    // 备份源：managed-files 含一个文件
    let src = dir.join("backup-src").join("managed-files");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.txt"), "a").unwrap();

    // 恢复前数据库含 r2（应存在于保护快照）
    {
        let conn = state.conn.lock().expect("lock");
        conn.execute(
            "INSERT INTO resources (id, kind, name, created_at, updated_at)
             VALUES ('r2', 'file', 'b.txt', 1, 1)",
            [],
        )
        .expect("insert r2");
        // 保护快照 = 当前数据库（含 r2）
        let protect = dir.join("protect");
        std::fs::create_dir_all(&protect).unwrap();
        conn.backup("main", protect.join("workspace.db"), None).unwrap();
    }

    // 制造移动失败：protect 目标已存在同名非空目录 → rename 失败
    let protect = dir.join("protect");
    let protect_managed = protect.join("managed-files-current");
    std::fs::create_dir_all(&protect_managed).unwrap();
    std::fs::write(protect_managed.join("conflict.txt"), "x").unwrap();

    let err = restore_managed_files(&state, &src, &managed, &protect_managed, &protect)
        .expect_err("should fail");
    assert!(
        err.message.contains("数据库已回滚"),
        "移动失败必须回滚数据库，实际: {}",
        err.message
    );

    // 数据库回滚到保护快照：r2 仍在
    let conn = state.conn.lock().expect("lock");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM resources WHERE id='r2'", [], |r| r.get(0))
        .expect("count");
    assert_eq!(count, 1, "数据库应回滚，r2 必须保留");

    let _ = std::fs::remove_dir_all(&dir);
}
```

### Step 3.2 运行确认失败

```powershell
cd src-tauri ; cargo test backup_service restore_managed_move
```

预期：编译错误（`restore_managed_files` 不存在）。

### Step 3.3 实现

`src-tauri/src/services/backup_service.rs` 修改：

1. `count_files` 替换为：

```rust
/// 统计目录内文件数与总字节数（含子目录）。
fn count_files_and_bytes(dir: &Path) -> (u64, u64) {
    fn walk(d: &Path, n: &mut u64, b: &mut u64) {
        if let Ok(entries) = std::fs::read_dir(d) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, n, b);
                } else {
                    *n += 1;
                    *b += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
    }
    let mut n = 0;
    let mut b = 0;
    walk(dir, &mut n, &mut b);
    (n, b)
}
```

2. 新增 `restore_managed_files`（放在 `restore_from_dir` 之后）：

```rust
/// 恢复托管文件，任何失败都回滚文件与数据库，保证库与磁盘一致。
fn restore_managed_files(
    state: &AppState,
    managed_src: &Path,
    managed_dst: &Path,
    protect_managed: &Path,
    protect_dir: &Path,
) -> Result<(), AppError> {
    // 先把当前托管目录整体移动到保护位置（移动不占双份空间）
    if managed_dst.exists() {
        if let Err(e) = std::fs::rename(managed_dst, protect_managed) {
            let _ = restore_db_snapshot(state, protect_dir);
            return Err(AppError::new(
                "restore_failed",
                format!("托管目录移动失败，数据库已回滚: {e}"),
            ));
        }
    }
    if let Err(e) = copy_dir_all(managed_src, managed_dst) {
        // 回滚：删除不完整的恢复目录，把原目录移回，并回滚数据库
        let _ = std::fs::remove_dir_all(managed_dst);
        if protect_managed.exists() {
            let _ = std::fs::rename(protect_managed, managed_dst);
        }
        let _ = restore_db_snapshot(state, protect_dir);
        return Err(AppError::new(
            "restore_failed",
            format!("托管文件恢复失败，数据库已回滚: {e}"),
        ));
    }
    // 校验恢复结果：文件数与总字节数一致
    let (src_count, src_bytes) = count_files_and_bytes(managed_src);
    let (dst_count, dst_bytes) = count_files_and_bytes(managed_dst);
    if src_count != dst_count || src_bytes != dst_bytes {
        let _ = std::fs::remove_dir_all(managed_dst);
        if protect_managed.exists() {
            let _ = std::fs::rename(protect_managed, managed_dst);
        }
        let _ = restore_db_snapshot(state, protect_dir);
        return Err(AppError::new(
            "restore_failed",
            "托管文件恢复校验不一致，数据库已回滚",
        ));
    }
    Ok(())
}
```

3. `restore_from_dir` 第 3 步替换为：

```rust
    // 3. 恢复托管文件（若备份含托管文件）；任何失败都会同时回滚数据库
    let managed_src = backup_dir.join("managed-files");
    if managed_src.exists() {
        let protect_managed = protect_dir.join("managed-files-current");
        let managed_dst = state.managed_dir.lock().expect("dir lock").clone();
        if let Err(e) = restore_managed_files(state, &managed_src, &managed_dst, &protect_managed, &protect_dir) {
            return Err(e);
        }
    }
```

### Step 3.4 运行确认通过

```powershell
cd src-tauri ; cargo test backup_service
```

预期：全部通过（含新增测试与既有 `restore_rolls_back_on_corrupt_db` 等）。

### Step 3.5 提交

```powershell
git add src-tauri/src/services/backup_service.rs
git commit -m "fix: 备份恢复失败时同步回滚数据库，保证库与文件一致"
```

---

## Task 4：索引断点恢复

### 根因

`scan_service.rs::scan_one_volume`（390 行）从 checkpoint（最近出队目录）重建全新 BFS 队列，未持久化的兄弟目录丢失，扫描仍标记 completed → 索引不完整但状态显示完成。

### 修复设计

采用"从卷根重扫"方案：checkpoint 只用于进度展示，不再作为恢复起点。扫描完成后 `DELETE ... WHERE scan_generation < ?` 只清理旧代次，同代次 upsert 幂等（`flush_batch` 已用 `ON CONFLICT DO UPDATE`），中断恢复重扫不会丢目录也不会重复。

### Step 4.1 写失败测试（Rust）

在 `src-tauri/src/services/scan_service.rs` 的 `mod tests` 末尾追加：

```rust
#[test]
fn scan_start_ignores_checkpoint_as_resume_origin() {
    // checkpoint 只用于进度展示，绝不能作为恢复起点（会漏掉兄弟目录）
    let root = std::path::PathBuf::from("C:\\");
    assert_eq!(
        scan_start(Some("C:\\docs\\sub1"), &root),
        root,
        "恢复起点必须始终是卷根"
    );
    assert_eq!(scan_start(None, &root), root);
}

#[test]
fn same_generation_rescan_from_root_keeps_all_entries() {
    let mut c = conn();
    upsert_volume_state(&mut c, "v9", "Z:\\", "scanning", 1).unwrap();
    let root = std::env::temp_dir().join(format!("nexus-resume-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("sub1")).unwrap();
    fs::create_dir_all(root.join("sub2")).unwrap();
    fs::write(root.join("sub1/a.txt"), "x").unwrap();
    fs::write(root.join("sub2/b.txt"), "y").unwrap();

    // 第一轮：模拟中断（只完成一部分，checkpoint 停在 sub1）
    let control = ScanControl {
        paused: Arc::new(AtomicBool::new(false)),
        cancelled: Arc::new(AtomicBool::new(true)),
    };
    // 直接构造：第一轮用 cancelled 提前终止
    let (_i1, _s1) = scan_directory(&mut c, &root, "v9", 1, &[], &control).unwrap();
    let _ = update_volume_progress(&mut c, "v9", Some("Z:\\sub1"), 1, 0);

    // 第二轮：从卷根重扫（同代次），不应漏掉任何目录
    let (indexed, _skipped) = scan_directory(&mut c, &root, "v9", 1, &[], &ScanControl::default()).unwrap();
    assert!(indexed >= 2, "重扫应覆盖所有文件，实际 {indexed}");

    let count: i64 = c
        .query_row(
            "SELECT count(*) FROM system_search_entries WHERE volume_id='v9'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(count >= 2, "恢复后索引必须包含全部目录，实际 {count}");

    let _ = fs::remove_dir_all(&root);
}
```

### Step 4.2 运行确认失败

```powershell
cd src-tauri ; cargo test scan_service scan_start
```

预期：编译错误（`scan_start` 不存在）。

### Step 4.3 实现

`src-tauri/src/services/scan_service.rs` 修改：

1. 新增辅助函数（放在 `scan_one_volume` 之前）：

```rust
/// 决定扫描起点：checkpoint 只用于进度展示，不能作为恢复起点。
/// 单个 checkpoint 无法表达完整待扫描队列，从中续扫会漏掉兄弟目录；
/// 中断/重启后一律从卷根重扫，依赖同代次 upsert 幂等保证不丢不重。
fn scan_start(_checkpoint: Option<&str>, root: &std::path::Path) -> std::path::PathBuf {
    root.to_path_buf()
}
```

2. `scan_one_volume` 中替换（390 行附近）：

```rust
    let control = app.state::<crate::AppState>().scan_control();
    // 恢复起点始终为卷根（checkpoint 仅用于进度展示，见 scan_start 注释）
    let start = scan_start(checkpoint.as_deref(), vol);
```

3. 更新 `scan_directory` 顶部注释中关于 checkpoint 的说明（172 行注释保留，另在函数注释补一句）：

```rust
// 注意：checkpoint 只用于进度展示；恢复续扫必须从卷根重新开始（见 scan_start），
// 否则未持久化的兄弟目录会永久丢失。
```

### Step 4.4 运行确认通过

```powershell
cd src-tauri ; cargo test scan_service
```

预期：全部通过（新增测试 + 既有 `scan_writes_checkpoint_for_resume` 等）。

### Step 4.5 提交

```powershell
git add src-tauri/src/services/scan_service.rs
git commit -m "fix: 索引中断后从卷根重扫，防止断点恢复漏目录"
```

---

## 验收清单（全部完成后执行）

```powershell
cd src-tauri ; cargo test
npx vitest run
npx tsc --noEmit
```

验收标准：
1. `open_session` 对 300KiB 文件返回完整内容，保存后磁盘完整；超过 10MiB 返回 `file_too_large`。
2. dirty 状态下切换页面/文件/关闭编辑器返回确认，保存/放弃/取消三种路径均不丢内容。
3. 托管文件恢复失败时数据库回滚到保护快照（r2 保留）。
4. 索引中断后从卷根重扫，兄弟目录不丢失，同代次重扫幂等。
