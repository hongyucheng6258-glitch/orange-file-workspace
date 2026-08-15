import { useCallback, useEffect, useState } from "react";
import {
  Eraser,
  FileWarning,
  HardDrive,
  Loader2,
  MonitorDown,
  Network,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  Wrench,
} from "lucide-react";
import type { ToolCleanResult } from "../../../lib/types";
import { call, formatSize } from "../../../lib/tauri";

type Risk = "low" | "medium" | "high";

interface ToolDef {
  key: string;
  name: string;
  desc: string;
  risk: Risk;
  command: string;
  args?: Record<string, unknown>;
  confirm: string;
  admin?: boolean;
}

const LOW_TOOLS: ToolDef[] = [
  {
    key: "refresh_icons",
    name: "刷新桌面图标",
    desc: "通知资源管理器重新加载桌面与文件图标",
    risk: "low",
    command: "refresh_desktop_icons",
    confirm: "确定要刷新桌面图标吗？",
  },
  {
    key: "clear_icon",
    name: "清理图标缓存",
    desc: "删除系统图标缓存，修复图标显示异常、空白图标",
    risk: "low",
    command: "clear_icon_cache",
    confirm: "确定要清理图标缓存吗？\n清理后重启资源管理器可完全生效。",
  },
  {
    key: "clear_thumb",
    name: "清理缩略图缓存",
    desc: "删除缩略图缓存，修复图片/视频缩略图不刷新",
    risk: "low",
    command: "clear_thumb_cache",
    confirm: "确定要清理缩略图缓存吗？",
  },
  {
    key: "clear_temp",
    name: "清理临时文件",
    desc: "删除用户临时目录中可删除的文件，释放磁盘空间",
    risk: "low",
    command: "clear_temp_files",
    confirm: "确定要清理用户临时文件吗？\n正在被占用的文件会被自动跳过。",
  },
  {
    key: "flush_dns",
    name: "刷新 DNS 缓存",
    desc: "清空 DNS 解析缓存，修复网页无法打开、域名解析异常",
    risk: "low",
    command: "flush_dns_cache",
    confirm: "确定要刷新 DNS 解析缓存吗？",
  },
  {
    key: "restart_explorer",
    name: "重启资源管理器",
    desc: "结束并重新启动文件资源管理器，修复任务栏/桌面卡死",
    risk: "medium",
    command: "restart_explorer",
    confirm:
      "确定要重启资源管理器吗？\n警告：所有资源管理器窗口（含正在浏览的文件夹）都会被关闭。",
  },
];

const ADMIN_TOOLS: ToolDef[] = [
  {
    key: "sfc",
    name: "系统文件检查",
    desc: "以管理员身份运行 sfc /scannow，扫描并修复系统文件",
    risk: "high",
    command: "run_admin_tool",
    args: { tool: "sfc" },
    confirm: "将弹出 UAC 授权窗口，确认后以管理员身份运行系统文件检查。\n扫描可能需要数分钟。",
    admin: true,
  },
  {
    key: "chkdsk",
    name: "磁盘错误检查",
    desc: "以管理员身份运行 chkdsk C: /f，检查并修复磁盘错误",
    risk: "high",
    command: "run_admin_tool",
    args: { tool: "chkdsk" },
    confirm: "将弹出 UAC 授权窗口，确认后安排磁盘检查。\n注意：修复将在下次重启时执行。",
    admin: true,
  },
  {
    key: "winsock",
    name: "网络重置",
    desc: "以管理员身份运行 netsh winsock reset，修复网络异常",
    risk: "high",
    command: "run_admin_tool",
    args: { tool: "winsock" },
    confirm: "将弹出 UAC 授权窗口，确认后重置网络协议栈。\n完成后建议重启电脑生效。",
    admin: true,
  },
];

const RISK_LABEL: Record<Risk, string> = {
  low: "低风险",
  medium: "中风险",
  high: "需管理员",
};

interface LogEntry {
  time: string;
  name: string;
  ok: boolean;
  detail: string;
}

function formatResult(name: string, r: unknown): string {
  if (r && typeof r === "object" && "deleted_files" in r) {
    const c = r as ToolCleanResult;
    let text = `已清理 ${c.deleted_files} 个文件`;
    if (c.freed_bytes > 0) text += `，释放 ${formatSize(c.freed_bytes)}`;
    if (c.skipped_files > 0) text += `，跳过 ${c.skipped_files} 个占用文件`;
    return text;
  }
  return `${name}执行成功`;
}

export function ToolsTab() {
  const [isAdmin, setIsAdmin] = useState(false);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  const loadAdmin = useCallback(async () => {
    try {
      setIsAdmin(await call<boolean>("get_admin_status", {}));
    } catch {
      setIsAdmin(false);
    }
  }, []);

  useEffect(() => {
    loadAdmin();
  }, [loadAdmin]);

  const addLog = (entry: LogEntry) => {
    setLogs((prev) => [entry, ...prev].slice(0, 20));
  };

  const run = async (tool: ToolDef) => {
    if (!window.confirm(tool.confirm)) {
      addLog({
        time: new Date().toLocaleTimeString(),
        name: tool.name,
        ok: true,
        detail: "已取消",
      });
      return;
    }
    setBusyKey(tool.key);
    setError(null);
    try {
      const result = await call(tool.command, tool.args ?? {});
      addLog({
        time: new Date().toLocaleTimeString(),
        name: tool.name,
        ok: true,
        detail: formatResult(tool.name, result),
      });
    } catch (e) {
      const msg = (e as Error).message;
      setError(msg);
      addLog({ time: new Date().toLocaleTimeString(), name: tool.name, ok: false, detail: msg });
    } finally {
      setBusyKey(null);
    }
  };

  const renderCard = (tool: ToolDef, icon: React.ReactNode) => (
    <section className="system-card tool-card" key={tool.key}>
      <h4>
        {icon} {tool.name}
        <span className={`tag tool-risk-${tool.risk}`}>{RISK_LABEL[tool.risk]}</span>
      </h4>
      <p className="tool-desc">{tool.desc}</p>
      <button
        className={`btn ${tool.risk === "high" ? "btn-primary" : "btn-ghost"}`}
        disabled={busyKey === tool.key}
        onClick={() => run(tool)}
      >
        {busyKey === tool.key ? <Loader2 size={13} className="spin" /> : <Wrench size={13} />}
        {busyKey === tool.key ? "执行中…" : "执行"}
      </button>
    </section>
  );

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>电脑修复工具</h3>
        <button className="btn btn-ghost" onClick={loadAdmin}>
          <RefreshCw size={13} /> 刷新身份
        </button>
      </div>

      <div className="admin-banner">
        {isAdmin ? (
          <span className="admin-chip ok">
            <ShieldCheck size={14} /> 当前以管理员身份运行，高级工具可直接使用
          </span>
        ) : (
          <span className="admin-chip">
            <ShieldAlert size={14} /> 当前为普通用户身份，高级工具执行时会弹出 UAC 授权窗口
          </span>
        )}
      </div>

      {error && <div className="system-error">{error}</div>}

      <h3 className="tool-section-title">常规修复</h3>
      <div className="system-cards">
        {LOW_TOOLS.map((t) =>
          renderCard(t, t.risk === "medium" ? <MonitorDown size={14} /> : <Eraser size={14} />),
        )}
      </div>

      <h3 className="tool-section-title">高级工具（管理员）</h3>
      <div className="system-cards">
        {ADMIN_TOOLS.map((t) =>
          renderCard(
            t,
            t.key === "sfc" ? <FileWarning size={14} /> : t.key === "chkdsk" ? <HardDrive size={14} /> : <Network size={14} />,
          ),
        )}
      </div>

      <section className="system-card tool-logs">
        <h4>
          <Trash2 size={14} /> 操作记录
        </h4>
        {logs.length === 0 ? (
          <div className="perf-note">暂无操作记录</div>
        ) : (
          <div className="tool-log-list">
            {logs.map((l, i) => (
              <div className="tool-log-row" key={i}>
                <span className="tool-log-time mono">{l.time}</span>
                <span className="tool-log-name">{l.name}</span>
                <span className={`tool-log-result ${l.ok ? "ok" : "fail"}`}>{l.detail}</span>
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
