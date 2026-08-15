import { useCallback, useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { listen } from "@tauri-apps/api/event";
import {
  File as FileIcon,
  Folder,
  Image as ImageIcon,
  FileText,
  FileCode2,
  FileArchive,
  Film,
  Music,
  Star,
  Trash2,
  HardDrive,
  Upload,
  Boxes,
  Clock,
} from "lucide-react";
import { call, formatSize, formatTime } from "../../../lib/tauri";
import { FileIconThumb } from "../../../components/FileIconThumb";
import { useFileStore } from "../../files/stores/fileStore";
import type { DashboardStats, RecentItem, ResourceKind } from "../../../lib/types";

function kindIcon(kind: ResourceKind, name: string) {
  if (kind === "folder") return <Folder size={18} color="var(--folder)" />;
  const n = name.toLowerCase();
  if (/\.(png|jpe?g|gif|webp|svg|bmp|ico)$/.test(n))
    return <ImageIcon size={18} color="var(--image)" />;
  if (/\.(mp4|webm|mov|avi|mkv)$/.test(n))
    return <Film size={18} color="var(--video)" />;
  if (/\.(mp3|wav|ogg|flac|m4a)$/.test(n))
    return <Music size={18} color="var(--audio)" />;
  if (/\.(zip|rar|7z|tar|gz)$/.test(n))
    return <FileArchive size={18} color="var(--archive)" />;
  if (/\.(rs|py|js|ts|tsx|jsx|go|java|c|h|cpp|cs|rb|php|json|xml|toml|yaml|sh|sql|md)$/.test(n))
    return <FileCode2 size={18} color="var(--code)" />;
  if (/\.(txt|md|log|docx|pdf|csv)$/.test(n))
    return <FileText size={18} color="var(--file)" />;
  return <FileIcon size={18} color="var(--file)" />;
}

function todayGreeting(): string {
  const hour = new Date().getHours();
  if (hour < 6) return "夜深了";
  if (hour < 12) return "早上好";
  if (hour < 14) return "中午好";
  if (hour < 18) return "下午好";
  return "晚上好";
}

export function HomePage() {
  const navigate = useNavigate();
  const setImportPickerOpen = useFileStore((s) => s.setImportPickerOpen);
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [dataDir, setDataDir] = useState<string | null>(null);
  const [managedDir, setManagedDir] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [s, env] = await Promise.all([
        call<DashboardStats>("dashboard_stats"),
        call<{ data_dir: string; managed_dir: string }>("app_environment"),
      ]);
      setStats(s);
      setDataDir(env.data_dir);
      setManagedDir(env.managed_dir);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    load();
    let unlisten: (() => void) | undefined;
    listen("resource-changed", load).then((cleanup) => {
      unlisten = cleanup;
    });
    return () => unlisten?.();
  }, [load]);

  const openResource = (item: RecentItem) => {
    navigate("/files", { state: { openId: item.id } });
  };

  const today = new Date().toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "long",
    day: "numeric",
    weekday: "long",
  });

  const statsCards = [
    {
      key: "files",
      label: "文件",
      value: stats?.totalFiles ?? 0,
      icon: <FileIcon size={20} color="var(--primary)" />,
      onClick: () => navigate("/files"),
    },
    {
      key: "folders",
      label: "文件夹",
      value: stats?.totalFolders ?? 0,
      icon: <Folder size={20} color="var(--folder)" />,
      onClick: () => navigate("/files"),
    },
    {
      key: "favorites",
      label: "收藏",
      value: stats?.favorites ?? 0,
      icon: <Star size={20} color="var(--warning)" />,
      onClick: () => navigate("/favorites"),
    },
    {
      key: "trash",
      label: "回收站",
      value: stats?.trash ?? 0,
      icon: <Trash2 size={20} color="var(--danger)" />,
      onClick: () => navigate("/trash"),
    },
  ];

  return (
    <div className="home-page">
      <div className="home-head">
        <div>
          <h1 className="home-title">{todayGreeting()}，欢迎回来</h1>
          <p className="home-sub">{today}</p>
        </div>
        <button className="btn btn-primary" onClick={() => setImportPickerOpen(true)}>
          <Upload size={14} /> 导入文件
        </button>
      </div>

      {error && (
        <div className="home-error" role="alert">
          {error}
        </div>
      )}

      <div className="home-stats">
        {statsCards.map((card) => (
          <button key={card.key} className="home-stat-card" onClick={card.onClick}>
            <span className="home-stat-icon">{card.icon}</span>
            <span className="home-stat-body">
              <span className="home-stat-value">{card.value}</span>
              <span className="home-stat-label">{card.label}</span>
            </span>
          </button>
        ))}
      </div>

      <div className="home-cols">
        <section className="home-panel home-recent">
          <div className="home-panel-head">
            <span className="home-panel-title">
              <Clock size={15} /> 最近使用
            </span>
            <button className="btn-link" onClick={() => navigate("/files")}>
              查看全部
            </button>
          </div>
          {!stats || stats.recent.length === 0 ? (
            <div className="home-empty">
              <span>暂无内容，导入文件后这里会展示最近使用的项目</span>
            </div>
          ) : (
            <div className="home-recent-list">
              {stats.recent.map((item) => (
                <button
                  key={item.id}
                  className="home-recent-item"
                  onClick={() => openResource(item)}
                  title={item.name}
                >
                  <span className="home-recent-icon">
                    {item.kind === "folder" ? (
                      kindIcon(item.kind, item.name)
                    ) : (
                      <FileIconThumb
                        resourceId={item.id}
                        size={18}
                        fallback={kindIcon(item.kind, item.name)}
                      />
                    )}
                  </span>
                  <span className="home-recent-name">{item.name}</span>
                  <span className="home-recent-meta">
                    {item.kind === "folder" ? "文件夹" : formatSize(item.file_size ?? null)}
                  </span>
                  <span className="home-recent-time">{formatTime(item.updated_at)}</span>
                </button>
              ))}
            </div>
          )}
        </section>

        <section className="home-panel home-storage">
          <div className="home-panel-head">
            <span className="home-panel-title">
              <HardDrive size={15} /> 存储空间
            </span>
          </div>
          <div className="home-storage-main">
            <span className="home-storage-value">{formatSize(stats?.totalSize ?? 0)}</span>
            <span className="home-storage-label">已纳入管理的文件总大小</span>
          </div>
          <div className="home-storage-extra">
            <span className="home-storage-row">
              <span>页面</span>
              <span>{stats?.totalPages ?? 0}</span>
            </span>
            <span className="home-storage-row">
              <span>代码项目</span>
              <span>{stats?.totalProjects ?? 0}</span>
            </span>
          </div>
          <div className="home-data-dir" title={dataDir ?? ""}>
            <Boxes size={13} />
            <span>
              数据目录：{dataDir ?? "加载中…"}
            </span>
          </div>
          {managedDir && (
            <div className="home-data-dir" title={managedDir}>
              <Boxes size={13} />
              <span>
                托管文件目录：{managedDir}
              </span>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
