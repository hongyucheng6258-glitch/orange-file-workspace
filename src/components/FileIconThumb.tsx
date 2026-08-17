import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { call } from "../lib/tauri";

/** 懒加载文件应用图标（exe/lnk 等从系统提取）。未加载成功时显示 fallback。 */
function IconImage({
  url,
  size,
  fallback,
  onError,
}: {
  url: string | null;
  size: number;
  fallback: React.ReactNode;
  onError: () => void;
}) {
  if (!url) return <>{fallback}</>;
  return (
    <img
      src={url}
      alt=""
      width={size}
      height={size}
      style={{
        objectFit: "contain",
        borderRadius: 4,
        background: "transparent",
        display: "block",
      }}
      onError={onError}
    />
  );
}

export function FileIconThumb({
  resourceId,
  size = 28,
  fallback,
}: {
  resourceId: string;
  size?: number;
  fallback: React.ReactNode;
}) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setUrl(null);
    call<string | null>("get_file_icon", { resourceId })
      .then((p) => {
        if (!cancelled && p) {
          setUrl(p.startsWith("data:") ? p : convertFileSrc(p));
        }
      })
      .catch((e) => {
        console.error("[FileIconThumb] get_file_icon error:", e);
      });
    return () => {
      cancelled = true;
    };
  }, [resourceId]);

  return <IconImage url={url} size={size} fallback={fallback} onError={() => setUrl(null)} />;
}

/** 按物理路径加载系统图标，用于全局搜索等非资源库条目。 */
export function PathIconThumb({
  path,
  name,
  size = 28,
  fallback,
}: {
  path: string;
  name?: string;
  size?: number;
  fallback: React.ReactNode;
}) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setUrl(null);
    call<string | null>("get_path_icon", { path, name })
      .then((value) => {
        if (!cancelled && value) setUrl(value.startsWith("data:") ? value : convertFileSrc(value));
      })
      .catch((error) => console.error("[PathIconThumb] get_path_icon error:", error));
    return () => {
      cancelled = true;
    };
  }, [path, name]);

  return <IconImage url={url} size={size} fallback={fallback} onError={() => setUrl(null)} />;
}
