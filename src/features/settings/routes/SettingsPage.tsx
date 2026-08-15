import { useCallback, useEffect, useState } from "react";
import { Database, FolderOpen, HardDrive, Package, RotateCcw, Archive } from "lucide-react";
import { call, formatTime } from "../../../lib/tauri";

interface Env {
  name: string;
  version: string;
  data_dir: string;
  db_path: string;
}

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
}

export function SettingsPage() {
  const [env, setEnv] = useState<Env | null>(null);
  const [backups, setBackups] = useState<BackupRecord[]>([]);
  const [creating, setCreating] = useState(false);
  const [restoring, setRestoring] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const e = await call<Env>("app_environment", {});
      setEnv(e);
    } catch {
      // ignore
    }
    const list = await call<BackupRecord[]>("list_backups", {});
    setBackups(list);
  }, []);

  useEffect(() => {
    load();
  }, []);

  const doBackup = async (includeFiles: boolean) => {
    setCreating(true);
    setMessage(null);
    try {
      await call<BackupRecord>("create_backup", { includeFiles });
      setMessage("备份已创建");
      await load();
    } catch (e) {
      setMessage(`备份失败：${(e as Error).message}`);
    } finally {
      setCreating(false);
    }
  };

  const doRestore = async (rec: BackupRecord) => {
    if (!window.confirm("恢复将覆盖当前所有数据，确定继续？")) return;
    setRestoring(rec.id);
    setMessage(null);
    try {
      await call("restore_backup", { backupPath: rec.path });
      setMessage("恢复完成，请刷新界面");
      await load();
    } catch (e) {
      setMessage(`恢复失败：${(e as Error).message}`);
    } finally {
      setRestoring(null);
    }
  };

  return (
    <div className="settings-page">
      <h2>设置</h2>

      <section className="settings-card">
        <h3>
          <Package size={15} /> 应用信息
        </h3>
        {env && (
          <div className="settings-rows">
            <div className="settings-row">
              <span>名称</span>
              <span>{env.name}</span>
            </div>
            <div className="settings-row">
              <span>版本</span>
              <span>{env.version}</span>
            </div>
            <div className="settings-row">
              <span>
                <HardDrive size={13} /> 数据目录
              </span>
              <span className="mono">{env.data_dir}</span>
            </div>
            <div className="settings-row">
              <span>
                <Database size={13} /> 数据库
              </span>
              <span className="mono">{env.db_path}</span>
            </div>
          </div>
        )}
      </section>

      <section className="settings-card">
        <h3>
          <Archive size={15} /> 备份与恢复
        </h3>
        <div className="settings-actions">
          <button className="btn" disabled={creating} onClick={() => doBackup(false)}>
            <Archive size={14} /> 导出元数据备份
          </button>
          <button className="btn btn-primary" disabled={creating} onClick={() => doBackup(true)}>
            <Archive size={14} /> 完整备份（含托管文件）
          </button>
          {message && <span className="settings-message">{message}</span>}
        </div>

        {backups.length > 0 && (
          <div className="backup-list">
            {backups.map((b) => (
              <div key={b.id} className="backup-item">
                <div className="backup-info">
                  <span className="backup-type">
                    {b.backup_type === "full" ? "完整备份" : "元数据备份"}
                  </span>
                  <span className="backup-meta">
                    {formatTime(b.created_at)} · {b.resource_count ?? 0} 资源 · 状态 {b.status}
                  </span>
                </div>
                <button
                  className="btn btn-ghost"
                  disabled={restoring === b.id}
                  onClick={() => doRestore(b)}
                >
                  <RotateCcw size={13} /> 恢复
                </button>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="settings-card">
        <h3>
          <FolderOpen size={15} /> 提示
        </h3>
        <p className="settings-note">
          备份使用 SQLite 在线备份生成一致性快照，应用运行期间也可以安全创建。
          完整备份包含托管文件，恢复时数据库与文件会被整体替换。
        </p>
      </section>
    </div>
  );
}
