import { useCallback, useEffect, useState } from "react";
import { FolderOpen, Loader2, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { call } from "../../../lib/tauri";
import { useSettingsStore } from "../stores/settingsStore";

type Which = "data_dir" | "managed_dir";

interface MigrationStatus {
  task_id?: string;
  stage?: string;
  target?: string;
  which?: string;
}

const STAGE_LABEL: Record<string, string> = {
  preparing: "准备中",
  copying: "复制数据",
  verifying: "校验数据",
  switching: "切换配置",
  cleaning: "清理旧目录",
};

export function StorageSettings() {
  const { settings } = useSettingsStore();
  const [status, setStatus] = useState<MigrationStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<Which | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await call<MigrationStatus | null>("get_migration_status", {});
      setStatus(s);
    } catch {
      // 应用未就绪时忽略
    }
  }, []);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus]);

  const migrate = async (which: Which, label: string) => {
    const dir = await open({ directory: true, title: `选择新的${label}` });
    if (!dir || typeof dir !== "string") return;
    if (
      !window.confirm(
        `将现有数据迁移到：\n${dir}\n\n迁移期间请勿关闭应用。确定继续？`,
      )
    )
      return;
    setBusy(which);
    setError(null);
    try {
      await call<string>("start_migration", { target: dir, which });
      await refreshStatus();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(null);
    }
  };

  const cancel = async (taskId: string) => {
    try {
      await call("cancel_migration", { taskId });
      await refreshStatus();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  if (!settings) return null;
  const s = settings.storage;
  const active = !!status && !!status.stage && status.stage !== "completed";
  const cancellable =
    active &&
    status?.stage !== "switching" &&
    status?.stage !== "cleaning";

  const dirCard = (which: Which, label: string, value: string) => (
    <div className="storage-dir-card">
      <div className="storage-dir-label">{label}</div>
      <div className="storage-dir-path mono">{value || "-"}</div>
      <div className="settings-actions">
        <button
          className="btn"
          disabled={!!busy || !!active}
          onClick={() => migrate(which, label)}
        >
          <FolderOpen size={14} /> 迁移目录…
        </button>
      </div>
    </div>
  );

  return (
    <div className="settings-section">
      <h3>存储</h3>
      {error && (
        <div className="settings-error" role="alert">
          {error}
        </div>
      )}

      {active && (
        <div className="migration-banner">
          <Loader2 size={14} className="spin" />
          <span>
            迁移进行中（阶段：{STAGE_LABEL[status!.stage!] ?? status!.stage}）
          </span>
          {cancellable && status?.task_id && (
            <button className="btn btn-ghost" onClick={() => cancel(status.task_id!)}>
              <X size={13} /> 取消
            </button>
          )}
        </div>
      )}

      {dirCard("data_dir", "数据目录", s.data_dir)}
      {dirCard("managed_dir", "托管文件目录", s.managed_dir)}

      <p className="settings-note">
        迁移会先复制并校验数据，再原子切换配置。切换前可取消；切换后不可取消，请等待完成。
        若迁移中断，应用将在下次启动时自动恢复。
      </p>
    </div>
  );
}
