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
      .then(async (p) => {
        console.log("[FileIconThumb] get_file_icon returned:", p, "for resourceId:", resourceId);
        if (!cancelled && p) {
          // Tauri convertFileSrc: Windows 下生成 http://asset.localhost/<path>
          const assetUrl = convertFileSrc(p);
          console.log("[FileIconThumb] convertFileSrc =>", assetUrl);

          // 额外测试：用 fetch 检查 asset URL 是否可访问
          try {
            const resp = await fetch(assetUrl);
            console.log("[FileIconThumb] fetch asset URL status:", resp.status, resp.statusText);
            if (!resp.ok) {
              console.error("[FileIconThumb] asset URL returned error:", resp.status);
            }
          } catch (fetchErr) {
            console.error("[FileIconThumb] fetch asset URL failed:", fetchErr);
          }

          if (!cancelled) setUrl(assetUrl);
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
        onLoad={() => console.log("[FileIconThumb] img loaded OK:", url)}
        onError={() => {
          console.error("[FileIconThumb] img onError:", url);
          setUrl(null);
        }}
      />
    );
  }
  return <>{fallback}</>;
}
