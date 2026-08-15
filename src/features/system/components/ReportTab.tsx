import { useState } from "react";
import { FileJson, FileText, FolderOpen, Loader2 } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ReportOutput } from "../../../lib/types";
import { call } from "../../../lib/tauri";

export function ReportTab() {
  const [report, setReport] = useState<ReportOutput | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);

  const doExport = async () => {
    setExporting(true);
    setError(null);
    try {
      const out = await call<ReportOutput>("export_system_report", {});
      setReport(out);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setExporting(false);
    }
  };

  const reveal = async (path: string) => {
    try {
      await revealItemInDir(path);
    } catch (e) {
      setError(`打开位置失败：${(e as Error).message}`);
    }
  };

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>系统诊断报告</h3>
        <button className="btn btn-primary" onClick={doExport} disabled={exporting}>
          {exporting ? <Loader2 size={13} className="spin" /> : <FileText size={13} />}
          {exporting ? "生成中…" : "生成诊断报告"}
        </button>
      </div>
      <p className="system-hint">
        报告包含设备信息、操作系统、处理器、内存、磁盘分区、网络配置、当前性能快照与高占用进程。
        所有数据仅在本地生成，不会上传。报告保存在应用数据目录下的 system-reports 文件夹。
      </p>
      {error && <div className="system-error">{error}</div>}

      {report && (
        <div className="report-output">
          <div className="report-item">
            <FileText size={15} />
            <div className="report-info">
              <span className="report-label">HTML 报告（适合查看和打印）</span>
              <span className="report-path mono">{report.html_path}</span>
            </div>
            <button className="btn btn-ghost" onClick={() => reveal(report.html_path)}>
              <FolderOpen size={13} /> 显示
            </button>
          </div>
          <div className="report-item">
            <FileJson size={15} />
            <div className="report-info">
              <span className="report-label">JSON 报告（适合分析）</span>
              <span className="report-path mono">{report.json_path}</span>
            </div>
            <button className="btn btn-ghost" onClick={() => reveal(report.json_path)}>
              <FolderOpen size={13} /> 显示
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
