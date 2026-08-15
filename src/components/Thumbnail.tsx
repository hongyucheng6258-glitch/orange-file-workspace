import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Image as ImageIcon } from "lucide-react";
import { call } from "../lib/tauri";

/** 懒加载图片缩略图。仅对图片资源有效。 */
export function Thumbnail({
  resourceId,
  name,
  size = 52,
}: {
  resourceId: string;
  name: string;
  size?: number;
}) {
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setUrl(null);
    setFailed(false);
    if (!/\.(png|jpe?g|gif|webp|bmp)$/i.test(name)) return;
    call<string | null>("get_thumbnail", { resourceId })
      .then((p) => {
        if (!cancelled && p) setUrl(convertFileSrc(p));
        else if (!cancelled) setFailed(true);
      })
      .catch(() => !cancelled && setFailed(true));
    return () => {
      cancelled = true;
    };
  }, [resourceId, name]);

  if (url) {
    return (
      <img
        src={url}
        alt={name}
        width={size}
        height={size}
        style={{
          objectFit: "cover",
          borderRadius: 6,
          background: "var(--bg)",
        }}
        onError={() => setFailed(true)}
      />
    );
  }
  if (failed) {
    return (
      <span style={{ display: "grid", placeItems: "center", width: size, height: size }}>
        <ImageIcon size={size * 0.55} color="var(--image)" />
      </span>
    );
  }
  return (
    <span
      style={{
        display: "grid",
        placeItems: "center",
        width: size,
        height: size,
        borderRadius: 6,
        background: "var(--bg)",
      }}
    >
      <ImageIcon size={size * 0.55} color="var(--image)" />
    </span>
  );
}
