import { useEffect, useState } from "react";
import { Database, HardDrive, Package } from "lucide-react";
import { call } from "../../../lib/tauri";

interface Env {
  name: string;
  version: string;
  data_dir: string;
  managed_dir: string;
  db_path: string;
}

export function AboutSettings() {
  const [env, setEnv] = useState<Env | null>(null);
  useEffect(() => {
    call<Env>("app_environment", {}).then(setEnv).catch(() => {});
  }, []);
  return (
    <div className="settings-section">
      <h3>关于</h3>
      {env && (
        <div className="settings-rows">
          <div className="settings-row">
            <span>
              <Package size={13} /> 名称
            </span>
            <span>{env.name}</span>
          </div>
          <div className="settings-row">
            <span>版本</span>
            <span>{env.version}</span>
          </div>
          <div className="settings-row">
            <span>
              <HardDrive size={13} /> 数据目录
            </span>
            <span className="mono">{env.data_dir}</span>
          </div>
          <div className="settings-row">
            <span>
              <HardDrive size={13} /> 托管文件目录
            </span>
            <span className="mono">{env.managed_dir}</span>
          </div>
          <div className="settings-row">
            <span>
              <Database size={13} /> 数据库
            </span>
            <span className="mono">{env.db_path}</span>
          </div>
        </div>
      )}
    </div>
  );
}
