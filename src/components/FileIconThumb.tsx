import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { call } from "../lib/tauri";

/** 懒加载文件应用图标（exe/lnk 等从系统提取）。未加载成功时显示 fallback。 */
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

  if (url) {
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
        onError={() => setUrl(null)}
      />
    );
  }
  return <>{fallback}</>;
}
