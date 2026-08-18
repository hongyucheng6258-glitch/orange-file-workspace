import type { CleanupItemStatus, CleanupMode, CleanupRunResult, CleanupScanItem } from "../../../lib/types";

export interface CleanupSelectionSummary {
  items: number;
  files: number;
  bytes: number;
  includesRecycleBin: boolean;
}

export function defaultCleanupSelection(items: CleanupScanItem[]): Set<string> {
  return new Set(
    items
      .filter((item) => item.default_selected && (item.status === "ready" || item.status === "partial"))
      .map((item) => item.id),
  );
}

export function summarizeCleanupSelection(
  items: CleanupScanItem[],
  selected: ReadonlySet<string>,
): CleanupSelectionSummary {
  return items.reduce<CleanupSelectionSummary>(
    (summary, item) => {
      if (!selected.has(item.id)) return summary;
      summary.items += 1;
      summary.files += item.files;
      summary.bytes += item.bytes;
      summary.includesRecycleBin ||= item.id === "recycle_bin";
      return summary;
    },
    { items: 0, files: 0, bytes: 0, includesRecycleBin: false },
  );
}

export function summarizeCleanupResult(result: CleanupRunResult) {
  return {
    completed: result.items.filter((item) => item.status === "completed").length,
    failed: result.items.filter((item) => item.status === "failed").length,
    requiresAdmin: result.items.filter((item) => item.status === "requires_admin").length,
    partial: result.items.filter((item) => item.status === "partial").length,
    deletedFiles: result.deleted_files,
    freedBytes: result.freed_bytes,
    skippedFiles: result.skipped_files,
  };
}

export function closeCleanupConfirmation() {
  return { confirming: false, recycleConfirmed: false };
}

export function canApplyCleanupScan(
  requestId: number,
  latestRequestId: number,
  requestedMode: CleanupMode,
  currentMode: CleanupMode,
) {
  return requestId === latestRequestId && requestedMode === currentMode;
}

export function cleanupResultPresentation(status: CleanupItemStatus | "partial") {
  switch (status) {
    case "completed":
      return { icon: "completed", className: "ok" } as const;
    case "requires_admin":
      return { icon: "requires_admin", className: "admin" } as const;
    case "partial":
      return { icon: "partial", className: "partial" } as const;
    default:
      return { icon: "failed", className: "fail" } as const;
  }
}
