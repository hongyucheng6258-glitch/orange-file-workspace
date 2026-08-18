import { useEffect, useRef, useState } from "react";
import { BellRing, Cpu, X } from "lucide-react";
import {
  Gauge,
  HardDrive,
  ListTree,
  FileText,
  MonitorSmartphone,
  MonitorCog,
  Globe,
  Settings2,
  Cog,
  Rocket,
  BatteryCharging,
  ShieldCheck,
  HeartPulse,
  Activity,
  Wrench,
  Clock,
} from "lucide-react";
import type { StorageInfo, SystemSnapshot, TemperatureInfo } from "../../../lib/types";
import { call } from "../../../lib/tauri";
import { ALERT_COOLDOWN_MS, loadAlertRules } from "../lib/alerts";
import { OverviewTab } from "../components/OverviewTab";
import { PerformanceTab } from "../components/PerformanceTab";
import { StorageTab } from "../components/StorageTab";
import { ProcessesTab } from "../components/ProcessesTab";
import { ReportTab } from "../components/ReportTab";
import { HardwareTab } from "../components/HardwareTab";
import { NetworkTab } from "../components/NetworkTab";
import { ServicesTab } from "../components/ServicesTab";
import { DriversTab } from "../components/DriversTab";
import { StartupTab } from "../components/StartupTab";
import { PowerTab } from "../components/PowerTab";
import { SecurityTab } from "../components/SecurityTab";
import { HealthTab } from "../components/HealthTab";
import { SensorsTab } from "../components/SensorsTab";
import { ToolsTab } from "../components/ToolsTab";
import { AppUsageTab } from "../components/AppUsageTab";

const TABS = [
  { key: "overview", label: "总览", icon: MonitorSmartphone },
  { key: "performance", label: "性能", icon: Gauge },
  { key: "hardware", label: "硬件", icon: MonitorCog },
  { key: "storage", label: "存储", icon: HardDrive },
  { key: "sensors", label: "传感器", icon: Activity },
  { key: "network", label: "网络", icon: Globe },
  { key: "processes", label: "进程", icon: ListTree },
  { key: "services", label: "服务", icon: Settings2 },
  { key: "drivers", label: "驱动", icon: Cog },
  { key: "startup", label: "启动项", icon: Rocket },
  { key: "power", label: "电源", icon: BatteryCharging },
  { key: "security", label: "安全", icon: ShieldCheck },
  { key: "health", label: "健康", icon: HeartPulse },
  { key: "tools", label: "工具", icon: Wrench },
  { key: "app_usage", label: "软件使用", icon: Clock },
  { key: "report", label: "报告", icon: FileText },
] as const;

type TabKey = (typeof TABS)[number]["key"];

interface AlertToast {
  title: string;
  detail: string;
}

export function SystemPage() {
  const [tab, setTab] = useState<TabKey>("overview");
  const [toasts, setToasts] = useState<AlertToast[]>([]);
  const lastAlertRef = useRef<Record<string, number>>({});

  // 告警轮询：每 5 秒按规则检测。
  useEffect(() => {
    let stopped = false;
    const check = async () => {
      const cfg = loadAlertRules();
      const now = Date.now();
      const fired: AlertToast[] = [];
      const withinCooldown = (key: string) =>
        now - (lastAlertRef.current[key] ?? 0) < ALERT_COOLDOWN_MS;

      try {
        const snap = await call<SystemSnapshot>("get_system_snapshot", {});
        if (
          cfg.cpu.enabled &&
          snap.cpu_usage >= cfg.cpu.threshold &&
          !withinCooldown("cpu")
        ) {
          fired.push({
            title: "CPU 使用率告警",
            detail: `当前 ${snap.cpu_usage.toFixed(0)}%，超过阈值 ${cfg.cpu.threshold}%`,
          });
          lastAlertRef.current.cpu = now;
        }
        if (
          cfg.mem.enabled &&
          snap.mem_percent >= cfg.mem.threshold &&
          !withinCooldown("mem")
        ) {
          fired.push({
            title: "内存使用率告警",
            detail: `当前 ${snap.mem_percent.toFixed(0)}%，超过阈值 ${cfg.mem.threshold}%`,
          });
          lastAlertRef.current.mem = now;
        }
      } catch {
        // 忽略单次检测失败
      }

      if (cfg.disk.enabled) {
        try {
          const storage = await call<StorageInfo[]>("get_storage_info", {});
          const maxUsed = storage.reduce((acc, d) => {
            const p = d.total_space > 0 ? ((d.total_space - d.available_space) / d.total_space) * 100 : 0;
            return Math.max(acc, p);
          }, 0);
          if (maxUsed >= cfg.disk.threshold && !withinCooldown("disk")) {
            fired.push({
              title: "磁盘空间告警",
              detail: `使用率最高 ${maxUsed.toFixed(0)}%，超过阈值 ${cfg.disk.threshold}%`,
            });
            lastAlertRef.current.disk = now;
          }
        } catch {
          // ignore
        }
      }

      if (cfg.temp.enabled) {
        try {
          const temps = await call<TemperatureInfo[]>("get_temperature_info", {});
          const maxTemp = temps.reduce((acc, t) => Math.max(acc, t.temperature_c ?? 0), 0);
          if (maxTemp >= cfg.temp.threshold && !withinCooldown("temp")) {
            fired.push({
              title: "温度告警",
              detail: `当前最高 ${maxTemp.toFixed(0)}°C，超过阈值 ${cfg.temp.threshold}°C`,
            });
            lastAlertRef.current.temp = now;
          }
        } catch {
          // ignore
        }
      }

      if (!stopped && fired.length > 0) {
        setToasts(fired);
      }
    };

    check();
    const timer = window.setInterval(check, 5000);
    return () => {
      stopped = true;
      window.clearInterval(timer);
    };
  }, []);

  // 告警横幅 8 秒后自动消失。
  useEffect(() => {
    if (toasts.length === 0) return;
    const timer = window.setTimeout(() => setToasts([]), 8000);
    return () => window.clearTimeout(timer);
  }, [toasts]);

  return (
    <div className="system-page">
      <div className="system-page-head">
        <h2>
          <Cpu size={17} /> 电脑信息
        </h2>
        <p>查看本机硬件、系统、性能与进程信息</p>
      </div>

      {toasts.length > 0 && (
        <div className="alert-toasts">
          {toasts.map((t, i) => (
            <div className="alert-toast" key={i}>
              <BellRing size={15} />
              <div className="alert-toast-body">
                <div className="alert-toast-title">{t.title}</div>
                <div className="alert-toast-detail">{t.detail}</div>
              </div>
              <button className="btn btn-ghost" onClick={() => setToasts([])}>
                <X size={13} />
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="system-tabs scrollable">
        {TABS.map(({ key, label, icon: Icon }) => (
          <button
            key={key}
            className={`system-tab-btn ${tab === key ? "active" : ""}`}
            onClick={() => setTab(key)}
          >
            <Icon size={14} /> {label}
          </button>
        ))}
      </div>

      {tab === "overview" && <OverviewTab />}
      {tab === "performance" && <PerformanceTab />}
      {tab === "hardware" && <HardwareTab />}
      {tab === "storage" && <StorageTab />}
      {tab === "sensors" && <SensorsTab />}
      {tab === "network" && <NetworkTab />}
      {tab === "processes" && <ProcessesTab />}
      {tab === "services" && <ServicesTab />}
      {tab === "drivers" && <DriversTab />}
      {tab === "startup" && <StartupTab />}
      {tab === "power" && <PowerTab />}
      {tab === "security" && <SecurityTab />}
      {tab === "health" && <HealthTab />}
      {tab === "tools" && <ToolsTab />}
      {tab === "app_usage" && <AppUsageTab />}
      {tab === "report" && <ReportTab />}
    </div>
  );
}
