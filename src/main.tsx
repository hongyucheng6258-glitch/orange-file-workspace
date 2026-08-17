import React from "react";
import ReactDOM from "react-dom/client";
import App from "./app/App";
import "./styles/tokens.css";
import "./styles/app.css";
import "./styles/workbench.css";
import { applyTheme, listenSystemTheme } from "./lib/theme";

async function bootstrap() {
  // 尽力读取主题设置；失败时保持 system 默认。
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const settings = await invoke<{
      appearance: { theme_mode: "system" | "light" | "dark" };
    }>("get_settings");
    const mode = settings.appearance.theme_mode;
    applyTheme(mode);
    if (mode === "system") {
      listenSystemTheme(() => {
        applyTheme("system");
      });
    }
  } catch {
    applyTheme("system");
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

void bootstrap();
