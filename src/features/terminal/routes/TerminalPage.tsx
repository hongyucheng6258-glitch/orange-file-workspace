import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { Channel } from "@tauri-apps/api/core";
import {
  ChevronDown,
  ChevronUp,
  Eraser,
  History,
  PanelLeft,
  PanelTop,
  Plus,
  RotateCcw,
  Search,
  Square,
  X,
} from "lucide-react";
import {
  TerminalView,
  TerminalViewHandle,
  type SearchResultsInfo,
} from "../components/TerminalView";
import { createTerminalInputBuffer, type TerminalInputBuffer } from "../lib/terminalInputBuffer";
import {
  addPane,
  createSplitTab,
  findPane,
  firstPane,
  removePane,
  setDirection,
  updatePane,
  type PaneDirection,
  type PaneMeta,
  type SplitTab,
} from "../lib/paneModel";
import {
  spawnTerminal,
  terminalClose,
  terminalResize,
  terminalWrite,
  listTerminalShells,
  recordTerminalHistory,
  listTerminalHistory,
  clearTerminalHistory,
  type TerminalEvent,
  type TerminalHistoryEntry,
  type TerminalShell,
  type TerminalShellInfo,
} from "../lib/terminal";

/** 单个标签页的运行时状态（内含分屏窗格）。 */
type TabMeta = SplitTab<TerminalShell>;

function shellTitle(shell: TerminalShell) {
  switch (shell) {
    case "powershell":
      return "PowerShell";
    case "cmd":
      return "CMD";
    case "gitbash":
      return "Git Bash";
    case "wsl":
      return "WSL";
  }
}

/** 探测失败时的兜底选项（内置两个恒可用）。 */
const FALLBACK_SHELLS: TerminalShellInfo[] = [
  { id: "powershell", label: "PowerShell", available: true },
  { id: "cmd", label: "CMD", available: true },
];

/**
 * 搜索高亮与结果事件选项。
 * 注意：addon-search 的 onDidChangeResults 仅在传入 decorations 时触发
 * （fireResultsChanged 会因参数为 false 直接返回），不传则计数无法更新。
 */
const SEARCH_DECORATIONS = {
  matchBackground: "#1f4d33",
  activeMatchBackground: "#005f43",
  matchOverviewRuler: "#1f4d33",
  activeMatchColorOverviewRuler: "#005f43",
};

export function TerminalPage() {
  const [tabs, setTabs] = useState<TabMeta[]>([]);
  const [activeKey, setActiveKey] = useState<number | null>(null);
  // 激活标签内当前聚焦的窗格（会话级；无标签时为 null）。
  const [activePaneKey, setActivePaneKey] = useState<number | null>(null);
  // 新标签使用的 Shell（已有标签的 Shell 固定）。
  const [shell, setShell] = useState<TerminalShell>("powershell");
  // 后端探测的可用 Shell 列表。
  const [shellOptions, setShellOptions] = useState<TerminalShellInfo[]>(FALLBACK_SHELLS);
  // 输出搜索（Ctrl+F）：作用于当前激活窗格。
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResult, setSearchResult] = useState<SearchResultsInfo | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  // 命令历史面板：作用于激活标签的 Shell。
  const [historyOpen, setHistoryOpen] = useState(false);
  const [historyEntries, setHistoryEntries] = useState<TerminalHistoryEntry[]>([]);

  const nextKeyRef = useRef(1);
  const nextPaneKeyRef = useRef(1);
  const mountedRef = useRef(true);
  const initializedRef = useRef(false);
  const viewHandlesRef = useRef<Map<number, TerminalViewHandle>>(new Map());
  const inputBuffersRef = useRef<Map<number, TerminalInputBuffer>>(new Map());
  // 同步防重入：spawn 是异步的，busy 状态更新不会在同一 React 渲染周期内生效，
  // StrictMode 下 TerminalView effect 同步双跑会导致同一窗格重复 spawn，这里用 ref 集合拦截。
  const spawningRef = useRef<Set<number>>(new Set());
  const shellRef = useRef(shell);
  shellRef.current = shell;
  // tabs 镜像：事件回调（onmessage/onReady）中读取最新状态。
  const tabsRef = useRef<TabMeta[]>([]);
  tabsRef.current = tabs;

  // “在终端打开”入口：URL query 携带初始工作目录（路径已编码），仅作用于首标签。
  const [searchParams] = useSearchParams();
  const initialCwd = useMemo(() => {
    const c = searchParams.get("cwd");
    return c && c.trim() ? c : undefined;
  }, [searchParams]);

  /** 更新标签内指定窗格的状态（不可变）。 */
  const updatePaneState = useCallback((paneKey: number, patch: Partial<PaneMeta>) => {
    setTabs((prev) => prev.map((t) => updatePane(t, paneKey, patch)));
  }, []);

  /** 创建标签（含首个窗格；不自动 spawn，等待 TerminalView onReady 后由 spawnPane 启动）。 */
  const createTab = useCallback((s: TerminalShell, cwd?: string) => {
    const key = nextKeyRef.current++;
    const paneKey = nextPaneKeyRef.current++;
    const n = tabsRef.current.length + 1;
    const tab: TabMeta = createSplitTab({
      key,
      title: `${shellTitle(s)} ${n}`,
      shell: s,
      initialCwd: cwd,
      paneKey,
    });
    inputBuffersRef.current.set(paneKey, createTerminalInputBuffer(terminalWrite));
    setTabs((prev) => [...prev, { ...tab }]);
    setActiveKey(key);
    setActivePaneKey(paneKey);
  }, []);

  /** 启动指定窗格的会话（幂等：busy、已有会话或正在 spawn 时跳过）。 */
  const spawnPane = useCallback(
    async (paneKey: number) => {
      if (spawningRef.current.has(paneKey)) return;
      const tab = tabsRef.current.find((t) => findPane(t, paneKey));
      const pane = tab ? findPane(tab, paneKey) : undefined;
      if (!tab || !pane || pane.busy || pane.sessionId != null) return;
      const handle = viewHandlesRef.current.get(paneKey);
      if (!handle) return;
      const term = handle.term;
      const seq = pane.seq + 1;
      spawningRef.current.add(paneKey);
      updatePaneState(paneKey, { seq, busy: true, error: null, exited: null });

      const channel = new Channel<TerminalEvent>();
      channel.onmessage = (ev) => {
        if (ev.type === "output" && Array.isArray(ev.data)) {
          viewHandlesRef.current.get(paneKey)?.term.write(new Uint8Array(ev.data));
        } else if (ev.type === "exit") {
          const code = typeof ev.data === "number" ? ev.data : null;
          inputBuffersRef.current.get(paneKey)?.reset();
          updatePaneState(paneKey, { sessionId: null, busy: false, exited: code });
        }
      };

      try {
        const info = await spawnTerminal({
          shell: tab.shell,
          cwd: tab.initialCwd,
          cols: term.cols,
          rows: term.rows,
          channel,
        });
        const curTab = tabsRef.current.find((t) => findPane(t, paneKey));
        const curPane = curTab ? findPane(curTab, paneKey) : undefined;
        if (!curPane || curPane.seq !== seq || !mountedRef.current) {
          // 窗格已被关闭/重启或页面已卸载：立即回收该会话。
          void terminalClose(info.session_id).catch(() => undefined);
          return;
        }
        updatePaneState(paneKey, { sessionId: info.session_id, cwd: info.cwd, busy: false });
        await inputBuffersRef.current.get(paneKey)?.attach(info.session_id);
        requestAnimationFrame(() => viewHandlesRef.current.get(paneKey)?.focus());
      } catch (e) {
        const curTab = tabsRef.current.find((t) => findPane(t, paneKey));
        const curPane = curTab ? findPane(curTab, paneKey) : undefined;
        if (curPane && curPane.seq === seq && mountedRef.current) {
          updatePaneState(paneKey, { error: (e as Error).message, busy: false });
        }
      } finally {
        spawningRef.current.delete(paneKey);
      }
    },
    [updatePaneState],
  );

  /** 结束指定窗格的会话（保留窗格，可重启）。 */
  const stopPane = useCallback(
    (paneKey: number) => {
      const tab = tabsRef.current.find((t) => findPane(t, paneKey));
      const pane = tab ? findPane(tab, paneKey) : undefined;
      if (!pane) return;
      const id = pane.sessionId;
      inputBuffersRef.current.get(paneKey)?.reset();
      updatePaneState(paneKey, { seq: pane.seq + 1, sessionId: null, busy: false });
      if (id != null) {
        void terminalClose(id).catch(() => undefined);
      }
    },
    [updatePaneState],
  );

  /** 重启指定窗格（先回收旧会话，再重新 spawn，沿用原 shell 与初始 cwd）。 */
  const restartPane = useCallback(
    (paneKey: number) => {
      stopPane(paneKey);
      void spawnPane(paneKey);
    },
    [stopPane, spawnPane],
  );

  /** 关闭标签：回收其下所有窗格会话并移除。 */
  const closeTab = useCallback((key: number) => {
    const tab = tabsRef.current.find((t) => t.key === key);
    tab?.panes.forEach((p) => {
      if (p.sessionId != null) {
        void terminalClose(p.sessionId).catch(() => undefined);
      }
      viewHandlesRef.current.delete(p.key);
      inputBuffersRef.current.get(p.key)?.reset();
      inputBuffersRef.current.delete(p.key);
    });
    const idx = tabsRef.current.findIndex((t) => t.key === key);
    setTabs((prev) => prev.filter((t) => t.key !== key));
    setActiveKey((prev) => {
      if (prev !== key) return prev;
      const remaining = tabsRef.current.filter((t) => t.key !== key);
      return remaining[Math.min(idx, remaining.length - 1)]?.key ?? null;
    });
    setActivePaneKey((prev) => {
      if (prev == null) return prev;
      const closedPaneKeys = new Set(tab?.panes.map((p) => p.key) ?? []);
      if (!closedPaneKeys.has(prev)) return prev;
      const nextTab = tabsRef.current.find((t) => t.key !== key);
      return nextTab?.panes[0]?.key ?? null;
    });
  }, []);

  /** 关闭单个窗格；若标签内无剩余窗格则关闭整个标签。 */
  const closePane = useCallback(
    (paneKey: number) => {
      const tab = tabsRef.current.find((t) => findPane(t, paneKey));
      if (!tab) return;
      const pane = findPane(tab, paneKey);
      if (pane?.sessionId != null) {
        void terminalClose(pane.sessionId).catch(() => undefined);
      }
      viewHandlesRef.current.delete(paneKey);
      inputBuffersRef.current.get(paneKey)?.reset();
      inputBuffersRef.current.delete(paneKey);
      const next = removePane(tab, paneKey);
      if (!next) {
        closeTab(tab.key);
        return;
      }
      setTabs((prev) => prev.map((t) => (t.key === tab.key ? next : t)));
      setActivePaneKey((prev) => (prev === paneKey ? next.panes[0]?.key ?? null : prev));
    },
    [closeTab],
  );

  /** 在当前激活标签内分屏：追加一个窗格并设为激活。 */
  const splitActiveTab = useCallback(
    (direction: PaneDirection) => {
      if (activeKey == null) return;
      const tab = tabsRef.current.find((t) => t.key === activeKey);
      if (!tab) return;
      const paneKey = nextPaneKeyRef.current++;
      const next = addPane(setDirection(tab, direction), paneKey);
      inputBuffersRef.current.set(paneKey, createTerminalInputBuffer(terminalWrite));
      setTabs((prev) => prev.map((t) => (t.key === tab.key ? next : t)));
      setActivePaneKey(paneKey);
      // 新窗格的 TerminalView 挂载后通过 onReady 自动 spawn。
    },
    [activeKey],
  );

  const addTab = useCallback(() => {
    createTab(shellRef.current);
  }, [createTab]);

  // 挂载：创建首标签（URL cwd 优先）；卸载：回收所有会话。
  useEffect(() => {
    mountedRef.current = true;
    if (!initializedRef.current) {
      initializedRef.current = true;
      createTab(shellRef.current, initialCwd);
    }
    return () => {
      mountedRef.current = false;
      initializedRef.current = false;
      const ids: number[] = [];
      tabsRef.current.forEach((t) =>
        t.panes.forEach((p) => {
          if (p.sessionId != null) ids.push(p.sessionId);
        }),
      );
      viewHandlesRef.current.clear();
      inputBuffersRef.current.forEach((input) => input.reset());
      inputBuffersRef.current.clear();
      setTabs([]);
      setActiveKey(null);
      setActivePaneKey(null);
      ids.forEach((id) => void terminalClose(id).catch(() => undefined));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // 探测可用 Shell（Git Bash / WSL 是否安装由后端判定）。
  useEffect(() => {
    let cancelled = false;
    listTerminalShells()
      .then((list) => {
        if (!cancelled && list.length > 0) setShellOptions(list);
      })
      .catch(() => {
        // 保留 FALLBACK_SHELLS
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const handleData = useCallback((paneKey: number, data: string) => {
    let input = inputBuffersRef.current.get(paneKey);
    if (!input) {
      input = createTerminalInputBuffer(terminalWrite);
      inputBuffersRef.current.set(paneKey, input);
      const pane = tabsRef.current
        .flatMap((t) => t.panes)
        .find((p) => p.key === paneKey);
      if (pane?.sessionId != null) void input.attach(pane.sessionId);
    }
    input.push(data);
  }, []);

  const handleResize = useCallback((paneKey: number, cols: number, rows: number) => {
    const pane = tabsRef.current
      .flatMap((t) => t.panes)
      .find((p) => p.key === paneKey);
    if (pane?.sessionId != null) {
      void terminalResize(pane.sessionId, cols, rows).catch(() => undefined);
    }
  }, []);

  /** 命令历史：记录一条命令（空命令后端忽略）。 */
  const handleCommand = useCallback((paneKey: number, command: string) => {
    const tab = tabsRef.current.find((t) => findPane(t, paneKey));
    if (!tab) return;
    const pane = findPane(tab, paneKey);
    void recordTerminalHistory({
      shell: tab.shell,
      command,
      cwd: pane?.cwd || undefined,
    }).catch(() => undefined);
  }, []);

  const setViewHandle = useCallback(
    (paneKey: number, h: TerminalViewHandle | null) => {
      if (h) {
        viewHandlesRef.current.set(paneKey, h);
      } else {
        viewHandlesRef.current.delete(paneKey);
      }
    },
    [],
  );

  const focusActiveTerminal = useCallback(() => {
    if (activePaneKey == null) return;
    requestAnimationFrame(() => viewHandlesRef.current.get(activePaneKey)?.focus());
  }, [activePaneKey]);

  /** 打开历史面板并加载当前标签 Shell 的历史。 */
  const openHistory = useCallback(() => {
    setHistoryOpen(true);
    const tab = tabsRef.current.find((t) => t.key === activeKey);
    if (!tab) return;
    listTerminalHistory(tab.shell, 200)
      .then((entries) => setHistoryEntries(entries))
      .catch(() => setHistoryEntries([]));
  }, [activeKey]);

  const closeHistory = useCallback(() => {
    setHistoryOpen(false);
    setHistoryEntries([]);
    focusActiveTerminal();
  }, [focusActiveTerminal]);

  /** 点击历史条目：把命令作为输入发送到当前激活窗格（复用输入缓冲，会话未就绪也能排队）。 */
  const applyHistory = useCallback(
    (command: string) => {
      if (activePaneKey == null) return;
      handleData(activePaneKey, command + "\r");
      closeHistory();
    },
    [handleData, activePaneKey, closeHistory],
  );

  /** 清空当前 Shell 的历史。 */
  const clearHistory = useCallback(() => {
    const tab = tabsRef.current.find((t) => t.key === activeKey);
    if (!tab) return;
    void clearTerminalHistory(tab.shell)
      .then(() => listTerminalHistory(tab.shell, 200))
      .then(setHistoryEntries)
      .catch(() => undefined);
  }, [activeKey]);

  /** 当前激活窗格的搜索 addon（无窗格时为 undefined）。 */
  const activeSearch = useCallback(
    () =>
      activePaneKey != null
        ? viewHandlesRef.current.get(activePaneKey)?.search
        : undefined,
    [activePaneKey],
  );

  /** 执行搜索：dir=1 下一个，dir=-1 上一个；incremental 用于输入过程。 */
  const runFind = useCallback(
    (dir: 1 | -1, incremental: boolean) => {
      const s = activeSearch();
      if (!s || !searchQuery.trim()) return;
      if (dir === 1) {
        s.findNext(
          searchQuery,
          incremental
            ? { incremental: true, decorations: SEARCH_DECORATIONS }
            : { decorations: SEARCH_DECORATIONS },
        );
      } else {
        s.findPrevious(searchQuery, { decorations: SEARCH_DECORATIONS });
      }
    },
    [activeSearch, searchQuery],
  );

  const openSearch = useCallback(() => {
    setSearchOpen(true);
    requestAnimationFrame(() => searchInputRef.current?.focus());
  }, []);

  const closeSearch = useCallback(() => {
    setSearchOpen(false);
    setSearchQuery("");
    setSearchResult(null);
    // 清理所有标签的搜索高亮，避免切换标签后残留。
    viewHandlesRef.current.forEach((h) => h.search.clearDecorations());
    focusActiveTerminal();
  }, [focusActiveTerminal]);

  // 快捷键：Ctrl+F 打开搜索（捕获阶段拦截，避免传给 shell）；Esc 关闭。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && (e.key === "f" || e.key === "F")) {
        e.preventDefault();
        e.stopPropagation();
        openSearch();
      } else if (e.key === "Escape" && searchOpen) {
        e.preventDefault();
        closeSearch();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [openSearch, closeSearch, searchOpen]);

  // 切换标签/窗格后恢复输入焦点；搜索打开时仍由搜索框保持焦点。
  useEffect(() => {
    if (!searchOpen) focusActiveTerminal();
  }, [activeKey, activePaneKey, searchOpen, focusActiveTerminal]);

  // 切换标签/窗格后对新的激活窗格重新执行搜索。
  useEffect(() => {
    if (!searchOpen) return;
    if (searchQuery.trim()) {
      activeSearch()?.findNext(searchQuery, {
        incremental: true,
        decorations: SEARCH_DECORATIONS,
      });
    } else {
      setSearchResult(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeKey, activePaneKey]);

  const activeTab = tabs.find((t) => t.key === activeKey) ?? null;
  const activePane =
    (activeTab != null && activePaneKey != null
      ? findPane(activeTab, activePaneKey)
      : undefined) ?? null;
  const activeRunning = activePane?.sessionId != null;
  // 激活标签内是否存在已退出/出错的窗格（用于"重启"可用性）。
  const activeTabHasStoppedPane =
    activeTab?.panes.some((p) => p.exited != null || p.error != null) ?? false;

  return (
    <div className="terminal-page">
      <div className="terminal-toolbar">
        <select
          className="terminal-shell-select"
          value={shell}
          title="新标签使用的 Shell"
          onChange={(e) => setShell(e.target.value as TerminalShell)}
        >
          {shellOptions
            .filter((s) => s.available)
            .map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
        </select>
        <span className="terminal-cwd" title={activePane?.cwd ?? ""}>
          {activePane?.cwd || (activeTab ? "正在启动…" : "未打开终端")}
        </span>
        <span className="toolbar-sep" />
        <button
          className="btn btn-ghost"
          title="左右分屏（当前标签）"
          disabled={activeTab == null}
          onClick={() => splitActiveTab("row")}
        >
          <PanelLeft size={14} />
        </button>
        <button
          className="btn btn-ghost"
          title="上下分屏（当前标签）"
          disabled={activeTab == null}
          onClick={() => splitActiveTab("column")}
        >
          <PanelTop size={14} />
        </button>
        <span className="toolbar-sep" />
        <button
          className="btn btn-ghost"
          title="清屏（当前窗格）"
          disabled={!activeRunning}
          onClick={() => {
            if (activePaneKey != null) {
              const h = viewHandlesRef.current.get(activePaneKey);
              h?.term.clear();
              h?.focus();
            }
          }}
        >
          <Eraser size={14} />
          清屏
        </button>
        <button
          className="btn btn-ghost"
          title="结束会话（当前窗格）"
          disabled={!activeRunning}
          onClick={() => {
            if (activePaneKey != null) stopPane(activePaneKey);
            focusActiveTerminal();
          }}
        >
          <Square size={14} />
          结束
        </button>
        <button
          className="btn btn-ghost"
          title="重启会话（当前窗格）"
          disabled={activeRunning || !activeTabHasStoppedPane}
          onClick={() => {
            if (activePaneKey != null) restartPane(activePaneKey);
            focusActiveTerminal();
          }}
        >
          <RotateCcw size={14} />
          重启
        </button>
        <span className="toolbar-sep" />
        <button
          className="btn btn-ghost"
          title="搜索输出（Ctrl+F）"
          onClick={openSearch}
        >
          <Search size={14} />
        </button>
        <button
          className="btn btn-ghost"
          title="命令历史"
          onClick={() => (historyOpen ? closeHistory() : openHistory())}
        >
          <History size={14} />
        </button>
        {historyOpen && (
          <div className="terminal-history">
            <div className="terminal-history-head">
              <span className="terminal-history-title">命令历史</span>
              <button
                className="btn btn-ghost"
                title="清空当前 Shell 的历史"
                disabled={historyEntries.length === 0}
                onClick={clearHistory}
              >
                <Eraser size={13} />
              </button>
              <button
                className="btn btn-ghost"
                title="关闭历史面板"
                onClick={closeHistory}
              >
                <X size={13} />
              </button>
            </div>
            <div className="terminal-history-list">
              {historyEntries.length === 0 ? (
                <div className="terminal-history-empty">暂无历史命令</div>
              ) : (
                historyEntries.map((entry) => (
                  <button
                    key={entry.id}
                    className="terminal-history-item"
                    title={entry.cwd ? `在 ${entry.cwd} 执行` : "执行该命令"}
                    onClick={() => applyHistory(entry.command)}
                  >
                    <span className="terminal-history-cmd">{entry.command}</span>
                    <span className="terminal-history-cwd">{entry.cwd}</span>
                  </button>
                ))
              )}
            </div>
          </div>
        )}
        {searchOpen && (
          <div className="terminal-search">
            <input
              ref={searchInputRef}
              className="terminal-search-input"
              placeholder="搜索输出…"
              value={searchQuery}
              onChange={(e) => {
                const v = e.target.value;
                setSearchQuery(v);
                if (v.trim()) {
                  runFind(1, true);
                } else {
                  setSearchResult(null);
                  activeSearch()?.clearDecorations();
                }
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  runFind(e.shiftKey ? -1 : 1, false);
                } else if (e.key === "Escape") {
                  e.preventDefault();
                  closeSearch();
                }
              }}
            />
            <button
              className="btn btn-ghost"
              title="上一个匹配（Shift+Enter）"
              disabled={!searchQuery.trim()}
              onClick={() => runFind(-1, false)}
            >
              <ChevronUp size={14} />
            </button>
            <button
              className="btn btn-ghost"
              title="下一个匹配（Enter）"
              disabled={!searchQuery.trim()}
              onClick={() => runFind(1, false)}
            >
              <ChevronDown size={14} />
            </button>
            <span className="terminal-search-count">
              {searchResult && searchResult.resultCount > 0
                ? `${searchResult.resultIndex + 1}/${searchResult.resultCount}`
                : searchQuery.trim()
                  ? "0/0"
                  : ""}
            </span>
            <button
              className="btn btn-ghost"
              title="关闭搜索（Esc）"
              onClick={closeSearch}
            >
              <X size={14} />
            </button>
          </div>
        )}
      </div>

      <div className="terminal-tabs">
        {tabs.map((tab) => (
          <div
            key={tab.key}
            className={`terminal-tab ${tab.key === activeKey ? "active" : ""}`}
            onClick={() => {
              setActiveKey(tab.key);
              // 切换到该标签时，默认聚焦其第一个窗格。
              const paneKey = tab.panes[0]?.key;
              if (paneKey != null) setActivePaneKey(paneKey);
            }}
            title={`${tab.panes.length > 1 ? `${tab.panes.length} 个窗格 · ` : ""}${
              firstPane(tab)?.cwd || tab.initialCwd || tab.title
            }`}
          >
            <span className="terminal-tab-dot" />
            <span className="terminal-tab-title">{tab.title}</span>
            {tab.panes.length > 1 && (
              <span className="terminal-tab-count">{tab.panes.length}</span>
            )}
            {tab.panes.some((p) => p.busy) && <span className="terminal-tab-spin" />}
            <button
              className="terminal-tab-close"
              title="关闭标签"
              onClick={(e) => {
                e.stopPropagation();
                closeTab(tab.key);
              }}
            >
              <X size={12} />
            </button>
          </div>
        ))}
        <button className="terminal-tab-add" title="新建终端标签" onClick={addTab}>
          <Plus size={14} />
        </button>
      </div>

      <div className="terminal-body">
        {tabs.length === 0 ? (
          <div className="terminal-empty">
            <span>所有终端已关闭</span>
            <button className="btn btn-primary" onClick={addTab}>
              <Plus size={14} /> 新建终端
            </button>
          </div>
        ) : (
          tabs.map((tab) => (
            <div
              key={tab.key}
              className={`terminal-pane ${tab.key === activeKey ? "active" : ""}`}
              style={{ display: tab.key === activeKey ? undefined : "none" }}
            >
              <div className={`terminal-split terminal-split-${tab.direction}`}>
                {tab.panes.map((pane) => (
                  <div
                    key={pane.key}
                    className={`terminal-split-cell ${
                      pane.key === activePaneKey ? "active" : ""
                    }`}
                    onMouseDown={() => {
                      setActivePaneKey(pane.key);
                      requestAnimationFrame(() =>
                        viewHandlesRef.current.get(pane.key)?.focus(),
                      );
                    }}
                  >
                    <TerminalView
                      ref={(h) => setViewHandle(pane.key, h)}
                      onReady={() => {
                        void spawnPane(pane.key);
                        if (pane.key === activePaneKey) focusActiveTerminal();
                      }}
                      onData={(d) => handleData(pane.key, d)}
                      onResize={(c, r) => handleResize(pane.key, c, r)}
                      onSearchResults={setSearchResult}
                      onCommand={(cmd) => handleCommand(pane.key, cmd)}
                    />
                    {pane.error && (
                      <div className="terminal-overlay terminal-error">{pane.error}</div>
                    )}
                    {pane.exited != null && (
                      <div className="terminal-overlay terminal-exited">
                        进程已退出（代码 {pane.exited}）。点击“重启”可重新打开会话。
                      </div>
                    )}
                    {tab.panes.length > 1 && (
                      <button
                        className="terminal-split-close"
                        title="关闭该窗格"
                        onClick={(e) => {
                          e.stopPropagation();
                          closePane(pane.key);
                        }}
                      >
                        <X size={11} />
                      </button>
                    )}
                  </div>
                ))}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
