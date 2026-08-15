import { invoke } from "@tauri-apps/api/core";

/** 统一调用 Rust 命令，错误转为 Error。 */
export async function call<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    const err = e as { code?: string; message?: string };
    const error = new Error(err.message ?? String(e));
    (error as { code?: string }).code = err.code ?? "unknown";
    throw error;
  }
}

/** 文件大小格式化。 */
export function formatSize(bytes: number | null | undefined): string {
  if (bytes == null || bytes < 0) return "-";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = "B";
  for (const u of units) {
    value /= 1024;
    unit = u;
    if (value < 1024) break;
  }
  return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${unit}`;
}

/** 时间戳格式化（本地时间）。 */
export function formatTime(ts: number | null | undefined): string {
  if (!ts) return "-";
  const d = new Date(ts * 1000);
  const now = new Date();
  const sameDay = d.toDateString() === now.toDateString();
  const time = d.toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
  });
  if (sameDay) return time;
  const yesterday = new Date(now.getTime() - 86400_000);
  if (d.toDateString() === yesterday.toDateString()) return "昨天";
  return d.toLocaleDateString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
  });
}

/** 根据扩展名获取显示类型名。 */
export function fileTypeName(name: string): string {
  const idx = name.lastIndexOf(".");
  if (idx <= 0) return "文件";
  const ext = name.slice(idx + 1).toLowerCase();
  const map: Record<string, string> = {
    png: "PNG 图片",
    jpg: "JPEG 图片",
    jpeg: "JPEG 图片",
    gif: "GIF 图片",
    webp: "WebP 图片",
    svg: "SVG 矢量图",
    bmp: "BMP 图片",
    mp4: "MP4 视频",
    webm: "WebM 视频",
    mp3: "MP3 音频",
    wav: "WAV 音频",
    pdf: "PDF 文档",
    zip: "ZIP 压缩包",
    rar: "RAR 压缩包",
    "7z": "7z 压缩包",
    tar: "TAR 压缩包",
    gz: "GZIP 压缩包",
    md: "Markdown",
    txt: "文本文件",
    json: "JSON",
    xml: "XML",
    csv: "CSV 表格",
    docx: "Word 文档",
    xlsx: "Excel 表格",
    pptx: "PPT 演示",
  };
  return map[ext] ?? `${ext.toUpperCase()} 文件`;
}
