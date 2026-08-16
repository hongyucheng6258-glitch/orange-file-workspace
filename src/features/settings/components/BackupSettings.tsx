import { useCallback, useEffect, useState } from "react";
import { Archive, FolderOpen, RotateCcw, Trash2, Download } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { call, formatTime } from "../../../lib/tauri";
import { useSettingsStore } from "../stores/settingsStore";
import { SaveBadge, SettingRow } from "./SettingRow";

interface BackupRecord {
  id: string;
  path: string;
  backup_type: string;
  database_version: number;
  resource_count: number | null;
  file_count: number | null;
  created_at: number;
  status: string;
  error_message: string | null;
  source: string;
}

const SOURCE_LABEL: Record<string, string> = {
  manual: "手动",
  auto: "自动",
  protect: "保护",
};

export function BackupSettings() {
  const { settings, saveState, update } = useSettingsStore();
  const [backups, setBackups] = useState<BackupRecord[]>([]);
  const [creating, setCreating] = useState(false);
  const [restoring, setRestoring] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const list = await call<BackupRecord[]>("list_backups", {});
      setBackups(list);
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const doBackup = async (includeFiles: boolean) => {
    setCreating(true);
    setError(null);
    try {
      await call<BackupRecord>("create_backup", { includeFiles });
      await load();
    } catch (e) {
      setError(`备份失败：${(e as Error).message}`);
    } finally {
      setCreating(false);
    }
  };

  const doRestore = async (rec: BackupRecord) => {
    setRestoring(rec.id);
    setError(null);
    try {
      // 预检：校验备份完整性并读取备份内容信息，失败则阻止恢复
      const manifest = await call<{
        created_at?: number;
        database_version?: number;
        include_files?: boolean;
      }>("validate_backup", { backupId: rec.id });
      const lines = [
        `数据库版本：${manifest.database_version ?? "?"}`,
        `内容：${manifest.include_files ? "完整备份（含托管文件）" : "仅元数据"}`,
        manifest.created_at ? `创建时间：${formatTime(manifest.created_at)}` : "",
      ].filter(Boolean);
      if (!window.confirm(`恢复将覆盖当前所有数据，确定继续？\n${rec.path}\n${lines.join("\n")}`)) {
        return;
      }
      await call("restore_backup", { backupPath: rec.path });
      await load();
      window.alert("恢复完成，请刷新界面");
    } catch (e) {
      setError(`恢复失败：${(e as Error).message}`);
    } finally {
      setRestoring(null);
    }
  };

  const doDelete = async (rec: BackupRecord) => {
    if (!window.confirm("删除后不可恢复，确定删除该备份？")) return;
    setBusy(rec.id);
    setError(null);
    try {
      await call("delete_backup", { backupId: rec.id });
      await load();
    } catch (e) {
      setError(`删除失败：${(e as Error).message}`);
    } finally {
      setBusy(null);
    }
  };

  const doExport = async (rec: BackupRecord) => {
    const dir = await open({ directory: true, title: "选择导出位置" });
    if (!dir || typeof dir !== "string") return;
    setBusy(rec.id);
    setError(null);
    try {
      await call("export_backup", { backupId: rec.id, destDir: dir });
    } catch (e) {
      setError(`导出失败：${(e as Error).message}`);
    } finally {
      setBusy(null);
    }
  };

  const doReveal = async () => {
    try {
      const dir = await call<string>("reveal_backups", {});
      await openPath(dir);
    } catch (e) {
      setError(`打开失败：${(e as Error).message}`);
    }
  };

  if (!settings) return null;
  const b = settings.backup;

  return (
    <div className="settings-section">
      <h3>自动备份</h3>
      <SaveBadge state={saveState} />

      <SettingRow label="启用自动备份" desc="按设定周期自动创建备份">
        <input
          type="checkbox"
          checked={b.enabled}
          onChange={(e) => update("backup.enabled", e.target.checked)}
          style={{ accentColor: "var(--primary)", width: 16, height: 16 }}
        />
      </SettingRow>

      <SettingRow label="备份周期">
        <select
          className="input"
          value={b.frequency}
          onChange={(e) => update("backup.frequency", e.target.value)}
        >
          <option value="daily">每天</option>
          <option value="weekly">每周</option>
        </select>
      </SettingRow>

      <SettingRow label="执行时间" desc="错过时间后将在下次启动时补执行一次">
        <input
          className="input"
          type="time"
          value={b.run_time}
          onChange={(e) => update("backup.run_time", e.target.value)}
        />
      </SettingRow>

      <SettingRow label="保留数量" desc="自动备份最多保留的数量，超出后清理最旧的">
        <input
          className="input"
          type="number"
          min={1}
          max={30}
          value={b.retention_count}
          onChange={(e) => {
            const n = Number(e.target.value);
            if (Number.isFinite(n) && n >= 1 && n <= 30) {
              update("backup.retention_count", n);
            }
          }}
        />
      </SettingRow>

      <SettingRow label="备份内容">
        <select
          className="input"
          value={b.backup_type}
          onChange={(e) => update("backup.backup_type", e.target.value)}
        >
          <option value="full">完整备份（含托管文件）</option>
          <option value="metadata">仅元数据</option>
        </select>
      </SettingRow>

      <h3 style={{ marginTop: 20 }}>立即备份</h3>
      {error && (
        <div className="settings-error" role="alert">
          {error}
        </div>
      )}
      <div className="settings-actions">
        <button className="btn" disabled={creating} onClick={() => doBackup(false)}>
          <Archive size={14} /> 元数据备份
        </button>
        <button className="btn btn-primary" disabled={creating} onClick={() => doBackup(true)}>
          <Archive size={14} /> 完整备份
        </button>
        <button className="btn" onClick={doReveal}>
          <FolderOpen size={14} /> 打开备份目录
        </button>
      </div>

      <h3 style={{ marginTop: 20 }}>备份记录</h3>
      {backups.length === 0 ? (
        <p className="settings-note">暂无备份记录</p>
      ) : (
        <div className="backup-list">
          {backups.map((rec) => (
            <div key={rec.id} className="backup-item">
              <div className="backup-info">
                <span className="backup-type">
                  {rec.backup_type === "full" ? "完整备份" : "元数据备份"}
                  <span className="backup-source">{SOURCE_LABEL[rec.source] ?? rec.source}</span>
                </span>
                <span className="backup-meta">
                  {formatTime(rec.created_at)} · {rec.resource_count ?? 0} 资源
                  {rec.status !== "completed" && ` · ${rec.status}`}
                  {rec.error_message && ` · ${rec.error_message}`}
                </span>
              </div>
              <div className="backup-item-actions">
                <button
                  className="icon-btn"
                  title="导出"
                  disabled={busy === rec.id}
                  onClick={() => doExport(rec)}
                >
                  <Download size={14} />
                </button>
                <button
                  className="icon-btn"
                  title="删除"
                  disabled={busy === rec.id}
                  onClick={() => doDelete(rec)}
                >
                  <Trash2 size={14} />
                </button>
                <button
                  className="btn btn-ghost"
                  disabled={restoring === rec.id || busy === rec.id}
                  onClick={() => doRestore(rec)}
                >
                  <RotateCcw size={13} /> 恢复
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
