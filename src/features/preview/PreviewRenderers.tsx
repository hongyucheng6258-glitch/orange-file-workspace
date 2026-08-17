import { useEffect, useState } from "react";
import { FileText, FileWarning, Loader2, ChevronRight, ChevronDown } from "lucide-react";

import type { Tab } from "../workbench/lib/layoutModel";
import {
  getCsvPreview,
  getArchiveListing,
  getResourcePath,
  assetUrl,
  type CsvPreviewData,
  type ArchiveInfo,
  type ArchiveEntry,
} from "./previewApi";
import { formatSize } from "../../lib/tauri";

// ─── Image Preview ────────────────────────────────────────

export function ImagePreview({ tab }: { tab: Tab }) {
  const resourceId = tab.params.resourceId as string;
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getResourcePath(resourceId)
      .then((p: string | null) => {
        if (!cancelled && p) setUrl(assetUrl(p));
      })
      .catch((e: unknown) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [resourceId]);

  if (error) return <div className="pv-error">{error}</div>;
  if (!url) return <div className="pv-loading">加载图片…</div>;
  return (
    <div className="pv-image-wrap">
      <img src={url} alt={tab.title} className="pv-image" />
    </div>
  );
}

// ─── PDF Preview ──────────────────────────────────────────

export function PdfPreview({ tab }: { tab: Tab }) {
  const resourceId = tab.params.resourceId as string;
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getResourcePath(resourceId)
      .then((p: string | null) => {
        if (!cancelled && p) setUrl(assetUrl(p));
      })
      .catch((e: unknown) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [resourceId]);

  if (error) return <div className="pv-error">{error}</div>;
  if (!url) return <div className="pv-loading">加载 PDF…</div>;
  return (
    <div className="pv-pdf-wrap">
      <iframe src={url} title={tab.title} className="pv-pdf-iframe" />
    </div>
  );
}

// ─── CSV Preview ───────────────────────────────────────────

export function CsvPreview({ tab }: { tab: Tab }) {
  const resourceId = tab.params.resourceId as string;
  const [data, setData] = useState<CsvPreviewData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getCsvPreview(resourceId)
      .then((d: CsvPreviewData) => !cancelled && setData(d))
      .catch((e: Error) => !cancelled && setError(e.message));
    return () => {
      cancelled = true;
    };
  }, [resourceId]);

  if (error) return <div className="pv-error">{error}</div>;
  if (!data) return <div className="pv-loading"><Loader2 className="spin" /> 加载 CSV…</div>;

  return (
    <div className="pv-csv-wrap">
      <div className="pv-csv-toolbar">
        <span>{data.total_rows} 行</span>
        {data.truncated && <span className="pv-csv-truncated">（仅显示前 {data.rows.length} 行）</span>}
      </div>
      <div className="pv-csv-table-wrap">
        <table className="pv-csv-table">
          <thead>
            <tr>
              <th className="pv-csv-idx">#</th>
              {data.headers.map((h: string, i: number) => (
                <th key={i}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.rows.map((row: string[], ri: number) => (
              <tr key={ri}>
                <td className="pv-csv-idx">{ri + 1}</td>
                {row.map((cell: string, ci: number) => (
                  <td key={ci}>{cell}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ─── Archive Preview ───────────────────────────────────────

export function ArchivePreview({ tab }: { tab: Tab }) {
  const resourceId = tab.params.resourceId as string;
  const [info, setInfo] = useState<ArchiveInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(true);

  useEffect(() => {
    let cancelled = false;
    getArchiveListing(resourceId)
      .then((d: ArchiveInfo) => !cancelled && setInfo(d))
      .catch((e: Error) => !cancelled && setError(e.message));
    return () => {
      cancelled = true;
    };
  }, [resourceId]);

  if (error) return <div className="pv-error">{error}</div>;
  if (!info) return <div className="pv-loading"><Loader2 className="spin" /> 加载压缩包内容…</div>;

  const fileCount = info.entries.filter((e: ArchiveEntry) => !e.is_dir).length;
  const dirCount = info.entries.filter((e: ArchiveEntry) => e.is_dir).length;

  return (
    <div className="pv-archive-wrap">
      <div className="pv-archive-header">
        <button className="pv-archive-toggle" onClick={() => setExpanded(!expanded)}>
          {expanded ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
        </button>
        <div className="pv-archive-info">
          <span className="pv-archive-fmt">{info.format.toUpperCase()}</span>
          <span>{info.entry_count} 条</span>
          <span>{fileCount} 文件 / {dirCount} 目录</span>
          <span>解压大小: {formatSize(info.total_uncompressed)}</span>
        </div>
      </div>
      {expanded && (
        <div className="pv-archive-list">
          {info.entries.map((entry: ArchiveEntry, i: number) => (
            <div key={i} className={`pv-archive-entry ${entry.is_dir ? "is-dir" : ""}`}>
              <span className="pv-archive-icon">
                {entry.is_dir ? "📁" : <FileText size={14} />}
              </span>
              <span className="pv-archive-path">{entry.path}</span>
              {!entry.is_dir && (
                <span className="pv-archive-size">{formatSize(entry.size)}</span>
              )}
              {entry.modified && (
                <span className="pv-archive-modified">{entry.modified}</span>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── Video Preview ─────────────────────────────────────────

export function VideoPreview({ tab }: { tab: Tab }) {
  const resourceId = tab.params.resourceId as string;
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getResourcePath(resourceId)
      .then((p: string | null) => !cancelled && p && setUrl(assetUrl(p)))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [resourceId]);

  if (!url) return <div className="pv-loading">加载视频…</div>;
  return (
    <div className="pv-video-wrap">
      <video src={url} controls className="pv-video" />
    </div>
  );
}

// ─── Audio Preview ─────────────────────────────────────────

export function AudioPreview({ tab }: { tab: Tab }) {
  const resourceId = tab.params.resourceId as string;
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getResourcePath(resourceId)
      .then((p: string | null) => !cancelled && p && setUrl(assetUrl(p)))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [resourceId]);

  if (!url) return <div className="pv-loading">加载音频…</div>;
  return (
    <div className="pv-audio-wrap">
      <FileText size={48} className="pv-audio-icon" />
      <div className="pv-audio-title">{tab.title}</div>
      <audio src={url} controls className="pv-audio" />
    </div>
  );
}

// ─── Fallback / Unsupported ────────────────────────────────

export function UnsupportedPreview({ tab }: { tab: Tab }) {
  return (
    <div className="pv-unsupported">
      <FileWarning size={48} />
      <p>无法预览此文件类型</p>
      <p className="pv-unsupported-title">{tab.title}</p>
    </div>
  );
}
