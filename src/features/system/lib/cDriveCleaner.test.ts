import { describe, expect, it } from "vitest";
import type { CleanupRunResult, CleanupScanItem } from "../../../lib/types";
import {
  canApplyCleanupScan,
  cleanupResultPresentation,
  closeCleanupConfirmation,
  defaultCleanupSelection,
  summarizeCleanupResult,
  summarizeCleanupSelection,
} from "./cDriveCleaner";

const items: CleanupScanItem[] = [
  {
    id: "user_temp",
    name: "用户临时文件",
    description: "",
    risk: "low",
    default_selected: true,
    requires_admin: false,
    files: 3,
    bytes: 100,
    status: "ready",
    message: null,
  },
  {
    id: "windows_temp",
    name: "Windows 临时文件",
    description: "",
    risk: "medium",
    default_selected: false,
    requires_admin: true,
    files: 0,
    bytes: 0,
    status: "requires_admin",
    message: "需要管理员权限",
  },
  {
    id: "recycle_bin",
    name: "C 盘回收站",
    description: "",
    risk: "medium",
    default_selected: true,
    requires_admin: false,
    files: 2,
    bytes: 50,
    status: "ready",
    message: null,
  },
];

describe("cDriveCleaner", () => {
  it("selects only ready default items", () => {
    expect([...defaultCleanupSelection(items)]).toEqual(["user_temp", "recycle_bin"]);
  });

  it("summarizes selected files, bytes and recycle bin", () => {
    expect(summarizeCleanupSelection(items, new Set(["user_temp", "recycle_bin"]))).toEqual({
      items: 2,
      files: 5,
      bytes: 150,
      includesRecycleBin: true,
    });
  });

  it("summarizes run statuses", () => {
    const result: CleanupRunResult = {
      status: "completed_with_errors",
      deleted_files: 4,
      freed_bytes: 120,
      skipped_files: 1,
      items: [
        { id: "a", name: "a", status: "completed", deleted_files: 4, freed_bytes: 120, skipped_files: 1, message: null },
        { id: "b", name: "b", status: "failed", deleted_files: 0, freed_bytes: 0, skipped_files: 0, message: "失败" },
        { id: "c", name: "c", status: "requires_admin", deleted_files: 0, freed_bytes: 0, skipped_files: 0, message: "需要管理员权限" },
      ],
    };
    expect(summarizeCleanupResult(result)).toEqual({
      completed: 1,
      failed: 1,
      requiresAdmin: 1,
      partial: 0,
      deletedFiles: 4,
      freedBytes: 120,
      skippedFiles: 1,
    });
  });

  it("resets recycle confirmation whenever confirmation closes", () => {
    expect(closeCleanupConfirmation()).toEqual({ confirming: false, recycleConfirmed: false });
  });

  it("applies only the latest scan for the current mode", () => {
    expect(canApplyCleanupScan(4, 4, "safe", "safe")).toBe(true);
    expect(canApplyCleanupScan(3, 4, "safe", "safe")).toBe(false);
    expect(canApplyCleanupScan(4, 4, "safe", "deep")).toBe(false);
  });

  it("uses distinct presentations for failed and admin-required results", () => {
    expect(cleanupResultPresentation("failed")).toEqual({ icon: "failed", className: "fail" });
    expect(cleanupResultPresentation("requires_admin")).toEqual({ icon: "requires_admin", className: "admin" });
    expect(cleanupResultPresentation("completed")).toEqual({ icon: "completed", className: "ok" });
    expect(cleanupResultPresentation("partial")).toEqual({ icon: "partial", className: "partial" });
  });
});
