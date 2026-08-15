export type ThemeMode = "system" | "light" | "dark";

/** 将主题模式应用到 <html data-theme>。system 时移除属性，回退到 CSS media query。 */
export function applyTheme(mode: ThemeMode): void {
  const el = document.documentElement;
  if (mode === "system") {
    delete el.dataset.theme;
  } else {
    el.dataset.theme = mode;
  }
}

/** 注册系统主题变化监听，返回取消函数。system 模式时确保 data-theme 属性干净。 */
export function listenSystemTheme(onChange: () => void): () => void {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const handler = () => onChange();
  mq.addEventListener("change", handler);
  return () => mq.removeEventListener("change", handler);
}
