import { useCallback, useEffect, useState } from "react";
import {
  Clock,
  Pause,
  Play,
  RefreshCw,
  Trash2,
  Timer,
} from "lucide-react";
import type { AppUsageSummary, AppUsageStatus } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { formatTime } from "../../../lib/tauri";
import { InfoRow, SvgSpark } from "../components/shared";

/** 将秒数格式化为可读时长。 */
function formatDuration(seconds: number): string {
  if (!seconds || seconds <= 0) return "0 秒";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h} 时 ${m} 分`;
  if (m > 0) return `${m} 分 ${s} 秒`;
  return `${s} 秒`;
}

type RangeKey = 1 | 7 | 30;

const RANGES: { key: RangeKey; label: string }[] = [
  { key: 1, label: "今日" },
  { key: 7, label: "近 7 天" },
  { key: 30, label: "近 30 天" },
];

export function AppUsageTab() {
  const [data, setData] = useState<AppUsageSummary | null>(null);
  const [status, setStatus] = useState<AppUsageStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [range, setRange] = useState<RangeKey>(1);
  const [idleInput, setIdleInput] = useState("");
  const [actionMsg, setActionMsg] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [summary, st] = await Promise.all([
        call<AppUsageSummary>("get_app_usage", { days: range }),
        call<AppUsageStatus>("get_app_usage_status", {}),
      ]);
      setData(summary);
      setStatus(st);
      setIdleInput(String(Math.round(st.idle_threshold_secs / 60)));
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, [range]);

  useEffect(() => {
    load();
  }, [load]);

  // 每 30 秒自动刷新统计数据
  useEffect(() => {
    const timer = window.setInterval(() => load(), 30000);
    return () => window.clearInterval(timer);
  }, [load]);

  const togglePause = async () => {
    try {
      if (status?.paused) {
        await call("resume_app_usage", {});
      } else {
        await call("pause_app_usage", {});
      }
      await load();
      setActionMsg(status?.paused ? "已恢复统计" : "已暂停统计");
      window.setTimeout(() => setActionMsg(null), 3000);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const applyIdleThreshold = async () => {
    const mins = parseInt(idleInput, 10);
    if (isNaN(mins) || mins < 1) return;
    try {
      await call("set_app_usage_idle_threshold", { seconds: mins * 60 });
      await load();
      setActionMsg(`空闲阈值已设为 ${mins} 分钟`);
      window.setTimeout(() => setActionMsg(null), 3000);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const clearData = async () => {
    if (!confirm("确定要清除所有软件使用时间统计数据吗？此操作不可撤销。")) return;
    try {
      await call("clear_app_usage", {});
      await load();
      setActionMsg("已清除全部统计数据");
      window.setTimeout(() => setActionMsg(null), 3000);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const topApps = data?.apps.slice(0, 20) ?? [];
  const maxSeconds = topApps.length > 0 ? topApps[0].active_seconds : 1;
  const sparkValues = (data?.daily_totals ?? []).map((d) => d.active_seconds);

  return (
    <div className="system-tab">
      <div className="system-tab-head">
        <h3>
          <Clock size={15} /> 软件使用时间
        </h3>
        <div className="app-usage-actions">
          {RANGES.map((r) => (
            <button
              key={r.key}
              className={`btn btn-ghost ${range === r.key ? "active" : ""}`}
              onClick={() => setRange(r.key)}
            >
              {r.label}
            </button>
          ))}
          <button
            className="btn btn-ghost"
            onClick={togglePause}
            title={status?.paused ? "恢复统计" : "暂停统计"}
          >
            {status?.paused ? <Play size={13} /> : <Pause size={13} />}
            {status?.paused ? "恢复" : "暂停"}
          </button>
          <button className="btn btn-ghost" onClick={load} disabled={loading}>
            <RefreshCw size={13} className={loading ? "spin" : ""} /> 刷新
          </button>
        </div>
      </div>

      {error && <div className="system-error">{error}</div>}
      {actionMsg && <div className="system-hint">{actionMsg}</div>}

      <div className="system-cards">
        {/* 总计与趋势 */}
        <section className="system-card">
          <h4>
            <Timer size={14} /> {data?.range_label ?? "总览"}
          </h4>
          {status?.paused && (
            <div className="perf-note">统计已暂停，不会记录新的使用时间。</div>
          )}
          <div className="perf-big">
            {formatDuration(data?.total_active_seconds ?? 0)}
          </div>
          <InfoRow label="统计范围">{data?.range_label ?? "-"}</InfoRow>
          <InfoRow label="应用数量">{data?.apps.length ?? 0}</InfoRow>
          {sparkValues.length >= 2 && (
            <div className="app-usage-spark">
              <span className="info-label">每日趋势</span>
              <SvgSpark values={sparkValues} width={220} height={48} />
            </div>
          )}
        </section>

        {/* 设置 */}
        <section className="system-card">
          <h4>统计设置</h4>
          <InfoRow label="当前状态">
            {status?.paused ? (
              <span style={{ color: "var(--warning)" }}>已暂停</span>
            ) : (
              <span style={{ color: "var(--success)" }}>统计中</span>
            )}
          </InfoRow>
          <InfoRow label="空闲阈值">
            {status ? `${Math.round(status.idle_threshold_secs / 60)} 分钟` : "-"}
          </InfoRow>
          <div className="app-usage-idle-setter">
            <input
              type="number"
              min={1}
              max={120}
              value={idleInput}
              onChange={(e) => setIdleInput(e.target.value)}
              className="app-usage-input"
            />
            <span className="info-label">分钟</span>
            <button
              className="btn btn-ghost"
              onClick={applyIdleThreshold}
              disabled={loading}
            >
              应用
            </button>
          </div>
          <p className="system-hint">
            超过空闲阈值无操作时停止计时。默认 5 分钟。
          </p>
          <button className="btn btn-danger-outline" onClick={clearData} disabled={loading}>
            <Trash2 size={13} /> 清除全部数据
          </button>
        </section>
      </div>

      {/* 应用排行 */}
      {topApps.length > 0 && (
        <section className="system-card" style={{ marginTop: 12 }}>
          <h4>软件排行（{data?.range_label}）</h4>
          <div className="app-usage-list">
            {topApps.map((app, i) => (
              <div key={app.app_id} className="app-usage-row">
                <span className="app-usage-rank">{i + 1}</span>
                <div className="app-usage-info">
                  <span className="app-usage-name">{app.display_name}</span>
                  <span className="app-usage-time">{formatDuration(app.active_seconds)}</span>
                </div>
                <div className="app-usage-bar-wrap">
                  <div
                    className="app-usage-bar"
                    style={{ width: `${(app.active_seconds / maxSeconds) * 100}%` }}
                  />
                </div>
                <span className="app-usage-pct">{app.percentage.toFixed(1)}%</span>
                <span className="app-usage-last">
                  {app.last_active_at > 0 ? formatTime(app.last_active_at) : "-"}
                </span>
              </div>
            ))}
          </div>
        </section>
      )}

      {topApps.length === 0 && !loading && (
        <section className="system-card" style={{ marginTop: 12 }}>
          <div className="perf-note">
            暂无使用时间数据。保持 Orange 运行，后台会自动统计前台活跃时间。
          </div>
        </section>
      )}
    </div>
  );
}
