import { useRef, useMemo, useState } from "react";
import { AlertTriangle, CheckCircle2, CircleAlert, HardDrive, Loader2, ShieldAlert, ScanSearch, Trash2 } from "lucide-react";
import type { CleanupMode, CleanupRunResult, CleanupScanItem } from "../../../lib/types";
import { call, formatSize } from "../../../lib/tauri";
import {
  defaultCleanupSelection,
  canApplyCleanupScan,
  cleanupResultPresentation,
  closeCleanupConfirmation,
  summarizeCleanupResult,
  summarizeCleanupSelection,
} from "../lib/cDriveCleaner";

const RISK_LABEL = { low: "低风险", medium: "中风险", high: "高风险" } as const;

export function CDriveCleaner() {
  const [mode, setMode] = useState<CleanupMode>("safe");
  const [items, setItems] = useState<CleanupScanItem[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [result, setResult] = useState<CleanupRunResult | null>(null);
  const [busy, setBusy] = useState<"scan" | "clean" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [recycleConfirmed, setRecycleConfirmed] = useState(false);
  const scanRequestId = useRef(0);

  const selection = useMemo(
    () => summarizeCleanupSelection(items, selected),
    [items, selected],
  );
  const runSummary = result ? summarizeCleanupResult(result) : null;

  const closeConfirmation = () => {
    const next = closeCleanupConfirmation();
    setConfirming(next.confirming);
    setRecycleConfirmed(next.recycleConfirmed);
  };

  const changeMode = (next: CleanupMode) => {
    if (busy !== null) return;
    scanRequestId.current += 1;
    setMode(next);
    setItems([]);
    setSelected(new Set());
    setResult(null);
    setConfirming(false);
    setRecycleConfirmed(false);
    setError(null);
  };

  const scan = async () => {
    const requestedMode = mode;
    const requestId = ++scanRequestId.current;
    setBusy("scan");
    setError(null);
    setResult(null);
    closeConfirmation();
    try {
      const scanned = await call<CleanupScanItem[]>("scan_c_drive_cleanup", { mode: requestedMode });
      if (canApplyCleanupScan(requestId, scanRequestId.current, requestedMode, mode)) {
        setItems(scanned);
        setSelected(defaultCleanupSelection(scanned));
      }
    } catch (e) {
      if (requestId === scanRequestId.current) setError((e as Error).message);
    } finally {
      if (requestId === scanRequestId.current) setBusy(null);
    }
  };

  const toggleItem = (item: CleanupScanItem) => {
    if (item.status !== "ready" && item.status !== "partial") return;
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(item.id)) next.delete(item.id);
      else next.add(item.id);
      return next;
    });
  };

  const clean = async () => {
    if (selection.includesRecycleBin && !recycleConfirmed) return;
    setBusy("clean");
    setError(null);
    try {
      const next = await call<CleanupRunResult>("clean_c_drive_items", {
        mode,
        itemIds: [...selected],
        confirmRecycleBin: selection.includesRecycleBin && recycleConfirmed,
      });
      setResult(next);
      setConfirming(false);
      setRecycleConfirmed(false);
      await scanAfterClean();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(null);
    }
  };

  const scanAfterClean = async () => {
    try {
      const scanned = await call<CleanupScanItem[]>("scan_c_drive_cleanup", { mode });
      setItems(scanned);
      setSelected(new Set());
    } catch {
      // 清理结果优先保留，刷新失败可由用户再次扫描。
    }
  };

  return (
    <section className="system-card c-drive-cleaner">
      <div className="cleaner-head">
        <div>
          <h3><HardDrive size={17} /> C 盘空间清理</h3>
          <p>仅扫描固定白名单目录；浏览器只清理普通缓存，不处理 Cookie、密码、历史记录或会话。</p>
        </div>
        <div className="cleaner-modes" aria-label="清理模式">
          <button disabled={busy !== null} className={mode === "safe" ? "active" : ""} onClick={() => changeMode("safe")}>安全清理</button>
          <button disabled={busy !== null} className={mode === "deep" ? "active" : ""} onClick={() => changeMode("deep")}>深度清理</button>
        </div>
      </div>

      <div className="cleaner-toolbar">
        <span>{mode === "safe" ? "常用低风险缓存与回收站" : "增加系统报告、转储和更新缓存"}</span>
        <button className="btn btn-primary" disabled={busy !== null} onClick={scan}>
          {busy === "scan" ? <Loader2 size={14} className="spin" /> : <ScanSearch size={14} />}
          {busy === "scan" ? "扫描中…" : items.length ? "重新扫描" : "开始扫描"}
        </button>
      </div>

      {error && <div className="system-error">{error}</div>}

      {items.length > 0 && (
        <>
          <div className="cleaner-list">
            {items.map((item) => (
              <label className={`cleaner-row ${item.status !== "ready" && item.status !== "partial" ? "disabled" : ""}`} key={item.id}>
                <input
                  type="checkbox"
                  checked={selected.has(item.id)}
                  disabled={item.status !== "ready"}
                  onChange={() => toggleItem(item)}
                />
                <span className="cleaner-item-main">
                  <span className="cleaner-item-title">
                    {item.name}
                    <span className={`tag tool-risk-${item.risk}`}>{RISK_LABEL[item.risk]}</span>
                    {item.requires_admin && <span className="tag">管理员</span>}
                    {item.status === "partial" && <span className="tag">部分扫描</span>}
                  </span>
                  <span className="cleaner-item-desc">{item.description}</span>
                </span>
                <span className="cleaner-item-size">
                  {item.status === "requires_admin" || item.status === "partial" ? item.message : `${item.files} 个文件 · ${formatSize(item.bytes)}`}
                </span>
              </label>
            ))}
          </div>

          <div className="cleaner-summary">
            <span>已选 {selection.items} 项，{selection.files} 个文件，预计 {formatSize(selection.bytes)}</span>
            <button
              className="btn btn-primary"
              disabled={busy !== null || selection.items === 0}
              onClick={() => setConfirming(true)}
            >
              <Trash2 size={14} /> 清理所选项
            </button>
          </div>
        </>
      )}

      {result && runSummary && (
        <div className="cleaner-result">
          <div className={`cleaner-result-summary ${runSummary.failed || runSummary.requiresAdmin || runSummary.partial ? "has-issues" : ""}`}>
            {runSummary.failed || runSummary.requiresAdmin ? <CircleAlert size={16} /> : runSummary.partial ? <ShieldAlert size={16} /> : <CheckCircle2 size={16} />}
            {runSummary.failed || runSummary.requiresAdmin ? "清理未完全执行" : runSummary.partial ? "清理部分完成" : "清理完成"}：已释放 {formatSize(runSummary.freedBytes)}，删除 {runSummary.deletedFiles} 个文件，跳过 {runSummary.skippedFiles} 个
          </div>
          {result.items.map((item) => {
            const presentation = cleanupResultPresentation(item.status);
            const Icon = presentation.icon === "completed" ? CheckCircle2 : presentation.icon === "requires_admin" ? ShieldAlert : CircleAlert;
            return (
              <div className="cleaner-result-row" key={item.id}>
                <span>{item.name}</span>
                <span className={presentation.className}>
                  <Icon size={13} />
                  {item.status === "completed" ? `完成 · ${formatSize(item.freed_bytes)}` : item.message ?? item.status}
                </span>
              </div>
            );
          })}
        </div>
      )}

      {confirming && (
        <div className="modal-mask" onClick={closeConfirmation}>
          <div className="modal cleaner-confirm" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
            <h3><AlertTriangle size={17} /> 确认清理</h3>
            <p>将清理 {selection.items} 项、约 {selection.files} 个文件，预计释放 {formatSize(selection.bytes)}。被占用或无权限文件会跳过。</p>
            {selection.includesRecycleBin && (
              <label className="cleaner-recycle-confirm">
                <input type="checkbox" checked={recycleConfirmed} onChange={(event) => setRecycleConfirmed(event.target.checked)} />
                我确认永久清空 C 盘回收站，其中内容不可恢复
              </label>
            )}
            <div className="modal-actions">
              <button className="btn" onClick={closeConfirmation}>取消</button>
              <button className="btn btn-primary" disabled={busy !== null || (selection.includesRecycleBin && !recycleConfirmed)} onClick={clean}>
                {busy === "clean" && <Loader2 size={14} className="spin" />} 确认清理
              </button>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}
