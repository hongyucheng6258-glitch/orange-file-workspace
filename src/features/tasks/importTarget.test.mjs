import assert from "node:assert/strict";
import test from "node:test";

import { resolveImportParentId } from "./importTarget.ts";

test("文件页导入使用当前目录", () => {
  assert.equal(resolveImportParentId("/files", "folder-123"), "folder-123");
  assert.equal(resolveImportParentId("/files", null), null);
});

test("非文件页导入忽略残留目录并回到根目录", () => {
  assert.equal(resolveImportParentId("/", "stale-folder"), null);
  assert.equal(resolveImportParentId("/tasks", "stale-folder"), null);
  assert.equal(resolveImportParentId("/favorites", "stale-folder"), null);
});
