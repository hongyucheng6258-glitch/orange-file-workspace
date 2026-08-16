import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { SearchAddon } from "@xterm/addon-search";
import "@xterm/xterm/css/xterm.css";
import { createCommandTracker } from "../lib/commandTracker";
import { useSettingsStore } from "../../settings/stores/settingsStore";
import { TERMINAL_THEMES, type TerminalThemeId } from "../lib/terminalThemes";

/** 搜索结果状态（xterm onDidChangeResults 载荷）。 */
export interface SearchResultsInfo {
  resultIndex: number;
  resultCount: number;
}

/** 暴露给父组件的 xterm 实例句柄。 */
export interface TerminalViewHandle {
  term: Terminal;
  fit: () => void;
  focus: () => void;
  search: SearchAddon;
}

interface TerminalViewProps {
  /** 实例就绪（fit 完成、cols/rows 可用）后回调，供父组件创建会话。 */
  onReady: (handle: TerminalViewHandle) => void;
  /** 用户输入（UTF-8 字符串）。 */
  onData: (data: string) => void;
  /** 窗口尺寸变化。 */
  onResize: (cols: number, rows: number) => void;
  /** 搜索匹配结果变化（供父组件显示 当前/总数）。 */
  onSearchResults?: (r: SearchResultsInfo) => void;
  /** 用户按下回车执行完整命令时回调（用于命令历史）。 */
  onCommand?: (command: string) => void;
  className?: string;
}

/** xterm cursorStyle 合法值。 */
type CursorStyle = "block" | "bar" | "underline";

export const TerminalView = forwardRef<TerminalViewHandle, TerminalViewProps>(
  function TerminalView({ onReady, onData, onResize, onSearchResults, onCommand, className }, ref) {
    const containerRef = useRef<HTMLDivElement>(null);
    const termRef = useRef<Terminal | null>(null);
    const fitRef = useRef<FitAddon | null>(null);
    const searchRef = useRef<SearchAddon | null>(null);
    const trackerRef = useRef<ReturnType<typeof createCommandTracker> | null>(null);
    // 终端外观设置（缺失时使用默认值）。
    const terminalSettings = useSettingsStore((s) => s.settings?.terminal);
    const fontSize = terminalSettings?.font_size ?? 14;
    const cursorStyle: CursorStyle = terminalSettings?.cursor_style ?? "bar";
    const themeId: TerminalThemeId = terminalSettings?.theme ?? "campbell";
    const palette = TERMINAL_THEMES[themeId];
    const onDataRef = useRef(onData);
    onDataRef.current = onData;
    const onResizeRef = useRef(onResize);
    onResizeRef.current = onResize;
    const onReadyRef = useRef(onReady);
    onReadyRef.current = onReady;
    const onSearchResultsRef = useRef(onSearchResults);
    onSearchResultsRef.current = onSearchResults;
    const onCommandRef = useRef(onCommand);
    onCommandRef.current = onCommand;

    useImperativeHandle(ref, () => ({
      // getter：每次访问实时取值，避免句柄固定为 mount 时的 null
      get term() {
        return termRef.current!;
      },
      fit: () => fitRef.current?.fit(),
      focus: () => termRef.current?.focus(),
      get search() {
        return searchRef.current!;
      },
    }));

    useEffect(() => {
      const container = containerRef.current;
      if (!container) return;

      const term = new Terminal({
        cursorBlink: true,
        cursorStyle,
        cursorWidth: 1,
        fontSize,
        lineHeight: 1.15,
        letterSpacing: 0,
        fontFamily: '"Cascadia Mono", Consolas, "Courier New", monospace',
        scrollback: 10_000,
        theme: palette,
      });
      termRef.current = term;

      const fit = new FitAddon();
      fitRef.current = fit;
      term.loadAddon(fit);
      term.loadAddon(new WebLinksAddon());
      const search = new SearchAddon();
      searchRef.current = search;
      term.loadAddon(search);
      search.onDidChangeResults((r) => onSearchResultsRef.current?.(r));

      term.open(container);
      fit.fit();
      term.focus();

      const refocus = () => term.focus();
      container.addEventListener("mousedown", refocus);
      container.addEventListener("click", refocus);

      const dataDisposable = term.onData((d) => {
        onDataRef.current(d);
        trackerRef.current?.push(d);
      });
      const tracker = createCommandTracker((cmd) => onCommandRef.current?.(cmd));
      trackerRef.current = tracker;
      const resizeDisposable = term.onResize(({ cols, rows }) => {
        onResizeRef.current(cols, rows);
      });

      onReadyRef.current({ term, fit: () => fit.fit(), focus: () => term.focus(), search });

      // 容器尺寸变化时重新 fit；fit 会触发 onResize 事件同步后端。
      const ro = new ResizeObserver(() => {
        try {
          fit.fit();
        } catch {
          // 容器不可见时忽略
        }
      });
      ro.observe(container);

      return () => {
        ro.disconnect();
        container.removeEventListener("mousedown", refocus);
        container.removeEventListener("click", refocus);
        dataDisposable.dispose();
        resizeDisposable.dispose();
        trackerRef.current?.reset();
        trackerRef.current = null;
        term.dispose();
        termRef.current = null;
        fitRef.current = null;
        searchRef.current = null;
      };
    }, []);

    // 设置变化时实时应用到已存在的实例（主题/字号/光标）。
    useEffect(() => {
      const term = termRef.current;
      if (!term) return;
      term.options.fontSize = fontSize;
      term.options.cursorStyle = cursorStyle;
      term.options.theme = palette;
      // 字号变化会改变行列数，重新 fit 触发 onResize 同步后端。
      try {
        fitRef.current?.fit();
      } catch {
        // 容器不可见时忽略
      }
    }, [fontSize, cursorStyle, themeId, palette]);

    return (
      <div
        ref={containerRef}
        className={className ?? "terminal-view"}
        style={{ background: palette.background }}
      />
    );
  },
);
