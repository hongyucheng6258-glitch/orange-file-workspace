import { AlertTriangle } from "lucide-react";
import { ConfirmationPreview } from "../lib/projectRuntime";

/** 运行确认对话框：展示后端生成的规范化摘要与脱敏环境变量。 */
export function RunConfirmationDialog({
  preview,
  busy,
  onCancel,
  onConfirm,
}: {
  preview: ConfirmationPreview;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const s = preview.summary;
  const envEntries = Object.entries(s.env ?? {});
  const quoteArg = (a: string) => (/[\s"\\]/.test(a) ? `"${a.replace(/"/g, '\\"')}"` : a);
  const commandLine = [s.executable, ...(s.args ?? [])].map(quoteArg).join(" ");
  return (
    <div className="run-confirm-mask">
      <div className="run-confirm-dialog">
        <div className="run-confirm-title">
          <AlertTriangle size={15} color="var(--warning, #d97706)" />
          <span>确认运行命令</span>
        </div>
        <p className="run-confirm-hint">
          命令将使用当前 Windows 用户权限在本机执行，请确认命令与工作目录正确。
        </p>
        <div className="run-confirm-row">
          <span className="run-confirm-label">命令</span>
          <code className="run-confirm-code">{commandLine || "（空）"}</code>
        </div>
        <div className="run-confirm-row">
          <span className="run-confirm-label">工作目录</span>
          <code className="run-confirm-code">{s.cwd || "项目根目录"}</code>
        </div>
        {envEntries.length > 0 && (
          <div className="run-confirm-row">
            <span className="run-confirm-label">环境变量</span>
            <div className="run-confirm-env">
              {envEntries.map(([key, value]) => (
                <code key={key} className="run-confirm-env-item">
                  {key}={value ?? "（删除）"}
                </code>
              ))}
            </div>
          </div>
        )}
        <div className="run-confirm-row">
          <span className="run-confirm-label">确认有效期</span>
          <span className="run-confirm-value">{preview.expiresInSeconds} 秒</span>
        </div>
        <div className="run-confirm-actions">
          <button className="btn-secondary" onClick={onCancel} disabled={busy}>
            取消
          </button>
          <button className="btn-primary" onClick={onConfirm} disabled={busy}>
            {busy ? "确认中…" : "确认并运行"}
          </button>
        </div>
      </div>
    </div>
  );
}
