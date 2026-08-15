import type { AlertRules } from "../../../lib/types";

const RULES_KEY = "nexus-alert-rules";

export const DEFAULT_RULES: AlertRules = {
  cpu: { enabled: true, threshold: 90 },
  mem: { enabled: true, threshold: 90 },
  disk: { enabled: true, threshold: 90 },
  temp: { enabled: false, threshold: 85 },
};

export function loadAlertRules(): AlertRules {
  try {
    const raw = localStorage.getItem(RULES_KEY);
    if (!raw) return DEFAULT_RULES;
    const parsed = JSON.parse(raw) as Partial<AlertRules>;
    return {
      cpu: { ...DEFAULT_RULES.cpu, ...parsed.cpu },
      mem: { ...DEFAULT_RULES.mem, ...parsed.mem },
      disk: { ...DEFAULT_RULES.disk, ...parsed.disk },
      temp: { ...DEFAULT_RULES.temp, ...parsed.temp },
    };
  } catch {
    return DEFAULT_RULES;
  }
}

export function saveAlertRules(rules: AlertRules): void {
  localStorage.setItem(RULES_KEY, JSON.stringify(rules));
}

/** 距上次提醒的最小间隔（毫秒），避免刷屏。 */
export const ALERT_COOLDOWN_MS = 5 * 60 * 1000;
