import type { ReactNode } from "react";
import { call, formatSize } from "../../../lib/tauri";

/** 将运行秒数格式化为可读文本。 */
export function formatUptime(seconds: number): string {
  if (!seconds || seconds <= 0) return "-";
  const d = Math.floor(seconds / 86_400);
  const h = Math.floor((seconds % 86_400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (d > 0) return `${d} 天 ${h} 小时`;
  if (h > 0) return `${h} 小时 ${m} 分`;
  if (m > 0) return `${m} 分 ${s} 秒`;
  return `${s} 秒`;
}

/** 字节速率格式化（B/s）。 */
export function formatRate(bytesPerSec: number): string {
  if (!bytesPerSec || bytesPerSec < 0) return "0 B/s";
  if (bytesPerSec < 1024) return `${Math.round(bytesPerSec)} B/s`;
  const units = ["KB/s", "MB/s", "GB/s"];
  let value = bytesPerSec;
  let unit = "B/s";
  for (const u of units) {
    value /= 1024;
    unit = u;
    if (value < 1024) break;
  }
  return `${value >= 100 ? Math.round(value) : value.toFixed(1)} ${unit}`;
}

export { formatSize };

/** 百分比进度条。 */
export function PercentBar({ value, danger }: { value: number; danger?: boolean }) {
  const v = Math.max(0, Math.min(100, value || 0));
  return (
    <div className={`percent-bar${danger && v > 85 ? " danger" : ""}`}>
      <div className="percent-fill" style={{ width: `${v}%` }} />
    </div>
  );
}

/** 简单 SVG 迷你趋势图。 */
export function SvgSpark({
  values,
  width = 180,
  height = 40,
  color = "var(--primary)",
}: {
  values: number[];
  width?: number;
  height?: number;
  color?: string;
}) {
  if (values.length < 2) {
    return <div className="spark-empty" style={{ width, height }}>等待采样…</div>;
  }
  const max = Math.max(...values, 1);
  const min = Math.min(...values, 0);
  const range = max - min || 1;
  const step = width / (values.length - 1);
  const points = values
    .map((v, i) => {
      const x = i * step;
      const y = height - 3 - ((v - min) / range) * (height - 6);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  return (
    <svg
      className="spark"
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      aria-hidden="true"
    >
      <polyline points={points} fill="none" stroke={color} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** 信息行组件。 */
export function InfoRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="info-row">
      <span className="info-label">{label}</span>
      <span className="info-value">{children}</span>
    </div>
  );
}

export async function fetchOverview() {
  return call<import("../../../lib/types").SystemOverview>("get_system_overview", {});
}
