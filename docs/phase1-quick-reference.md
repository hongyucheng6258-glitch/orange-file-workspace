# Orange Phase 1 快速参考

## API 速查表

### 命令面板 API (commandPaletteApi)

```typescript
// 记录命令执行
await commandPaletteApi.recordExecution(commandId, commandLabel, commandCategory);

// 获取常用命令（按执行次数排序）
const frequent = await commandPaletteApi.getFrequentCommands(limit);

// 获取最近命令（按时间排序）
const recent = await commandPaletteApi.getRecentCommands(limit);

// 按分类获取命令历史
const byCategory = await commandPaletteApi.getCommandsByCategory(category, limit);

// 搜索命令历史
const results = await commandPaletteApi.searchCommands(query, limit);

// 获取命令统计信息
const stats = await commandPaletteApi.getCommandStats(commandId);

// 清空所有历史
await commandPaletteApi.clearHistory();

// 删除特定命令历史
await commandPaletteApi.deleteCommand(commandId);
```

### 标签 API (tagsApi)

```typescript
// 创建标签
const tag = await tagsApi.createTag(name, color);

// 列出所有标签
const tags = await tagsApi.listTags();

// 更新标签
const updated = await tagsApi.updateTag(tagId, newName, newColor);

// 删除标签
await tagsApi.deleteTag(tagId);

// 关联标签到资源
const resourceTag = await tagsApi.attachTag(tagId, resourceId, resourceType);

// 解除关联
await tagsApi.detachTag(tagId, resourceId, resourceType);

// 获取资源的所有标签
const resourceTags = await tagsApi.getResourceTags(resourceId, resourceType);

// 获取标签关联的所有资源
const resources = await tagsApi.getTagResources(tagId);

// 批量关联标签
await tagsApi.batchAttachTags(tagIds, resourceId, resourceType);
```

### 最近访问 API (recentApi)

```typescript
// 记录资源访问
await recentApi.recordAccess(resourceId, resourceType, resourcePath, resourceName);

// 获取最近访问列表
const items = await recentApi.getRecentItems(limit, resourceType);

// 删除单个记录
await recentApi.removeItem(itemId);

// 清空所有记录
await recentApi.clearAll();

// 清理旧记录（保留最近 N 天）
const deletedCount = await recentApi.cleanupOld(daysToKeep);
```

### 工作区会话 API (workspaceSessionsApi)

```typescript
// 保存会话
const session = await workspaceSessionsApi.saveSession(
  projectId,
  openFilesJson,
  activeFileId,
  terminalTabsJson,
  taskListJson,
  sidebarState,
  layoutConfig,
  scrollPositions
);

// 获取会话
const session = await workspaceSessionsApi.getSession(projectId);

// 更新最后访问时间
const updated = await workspaceSessionsApi.updateLastAccess(projectId);

// 删除会话
await workspaceSessionsApi.deleteSession(projectId);

// 列出所有会话
const sessions = await workspaceSessionsApi.listSessions(limit);
```

### Git API (gitApi)

```typescript
// 注册 Git 仓库
const repo = await gitApi.registerRepository(projectId, repoPath);

// 获取仓库信息
const repo = await gitApi.getRepository(projectId);

// 更新仓库状态
await gitApi.updateRepositoryStatus(
  repoId,
  currentBranch,
  hasUncommitted,
  aheadCount,
  behindCount
);

// 删除仓库
await gitApi.deleteRepository(repoId);

// 保存文件状态
const fileStatus = await gitApi.saveFileStatus(repoId, filePath, status, staged);

// 获取所有文件状态
const statuses = await gitApi.listFileStatuses(repoId);

// 获取暂存文件
const stagedFiles = await gitApi.getStagedFiles(repoId);

// 获取未暂存文件
const unstagedFiles = await gitApi.getUnstagedFiles(repoId);

// 按状态获取文件
const files = await gitApi.getFilesByStatus(repoId, status);
```

## Hooks 使用指南

### useCommandPalette

```typescript
const {
  isOpen,
  open,
  close,
  toggle,
  executeCommand,
  searchCommands,
  recentCommands,
  frequentCommands,
  clearHistory,
  loading
} = useCommandPalette();

// Hook 自动监听 Ctrl+K 快捷键
// 在组件中渲染 CommandPalette 即可
```

### useTags

```typescript
const {
  tags,
  resourceTags,
  loading,
  error,
  createTag,
  updateTag,
  deleteTag,
  attachTag,
  detachTag,
  toggleTag,
  isTagAttached,
  reload
} = useTags(resourceId, resourceType);

// 自动加载所有标签和指定资源的关联标签
```

### useRecentItems

```typescript
const {
  items,
  loading,
  error,
  recordAccess,
  removeItem,
  clearAll,
  cleanupOld,
  reload
} = useRecentItems(resourceType, limit);

// 自动加载最近访问列表
```

### useWorkspaceSession

```typescript
const {
  session,
  loading,
  error,
  saveSession,
  updateLastAccess,
  deleteSession,
  autoSave,
  reload
} = useWorkspaceSession(projectId);

// 自动加载当前项目的会话
// autoSave 可在状态变化时调用（建议加防抖）
```

### useGitStatus

```typescript
const {
  repository,
  fileStatuses,
  loading,
  error,
  updateRepositoryStatus,
  saveFileStatus,
  getStagedFiles,
  getUnstagedFiles,
  getFilesByStatus,
  reload
} = useGitStatus(projectId, repoPath);

// 自动注册并加载 Git 仓库状态
```

## 组件快速集成

### CommandPalette

```tsx
import { CommandPalette } from './components/CommandPalette';
import { useCommandPalette } from './hooks';

function App() {
  const palette = useCommandPalette();
  
  const commands = [
    {
      id: 'open-file',
      label: 'Open File',
      category: 'File',
      keywords: ['open', 'file'],
      action: async () => { /* ... */ }
    },
    // ... 更多命令
  ];

  return (
    <>
      <CommandPalette
        isOpen={palette.isOpen}
        onClose={palette.close}
        commands={commands}
        recentCommands={palette.recentCommands}
        frequentCommands={palette.frequentCommands}
      />
      {/* 其他内容 */}
    </>
  );
}
```

### TagManager

```tsx
import { TagManager } from './components/TagManager';

function FileContextMenu({ fileId }) {
  return (
    <TagManager
      resourceId={fileId}
      resourceType="file"
      onTagsChange={(tags) => {
        console.log('Tags updated:', tags);
      }}
    />
  );
}
```

### RecentItemsList

```tsx
import { RecentItemsList } from './components/RecentItemsList';

function Sidebar() {
  return (
    <RecentItemsList
      resourceTypeFilter="file"
      limit={20}
      onItemClick={(item) => {
        // 打开文件
        openFile(item.resource_id);
      }}
    />
  );
}
```

### GitStatus

```tsx
import { GitStatus } from './components/GitStatus';

function ProjectView({ projectId, repoPath }) {
  return (
    <GitStatus
      projectId={projectId}
      repoPath={repoPath}
      onRefresh={() => {
        console.log('Git status refreshed');
      }}
    />
  );
}
```

## 数据结构参考

### Command

```typescript
interface Command {
  id: string;
  label: string;
  category: string;
  keywords: string[];
  action: () => Promise<void>;
  shortcut?: string;
  icon?: string;
}
```

### Tag

```typescript
interface Tag {
  id: string;
  name: string;
  color?: string;
  created_at: string;
  updated_at: string;
}
```

### RecentItem

```typescript
interface RecentItem {
  id: string;
  resource_id: string;
  resource_type: 'file' | 'folder' | 'page' | 'project';
  resource_path: string;
  resource_name?: string;
  access_count: number;
  last_accessed: string;
  created_at: string;
}
```

### WorkspaceSession

```typescript
interface WorkspaceSession {
  id: string;
  project_id: string;
  open_files_json: string;
  active_file_id: string | null;
  terminal_tabs_json: string;
  task_list_json: string;
  sidebar_state: string;
  layout_config: string;
  scroll_positions: string;
  last_restored: string | null;
  created_at: string;
  updated_at: string;
}
```

### GitRepository

```typescript
interface GitRepository {
  id: string;
  project_id: string;
  repo_path: string;
  current_branch: string | null;
  has_uncommitted: boolean;
  ahead_count: number;
  behind_count: number;
  last_fetched: string | null;
  created_at: string;
  updated_at: string;
}
```

### GitFileStatus

```typescript
interface GitFileStatus {
  id: string;
  repo_id: string;
  file_path: string;
  status: string; // 'untracked' | 'modified' | 'added' | 'deleted' | 'renamed' | 'conflicted'
  staged: boolean;
  created_at: string;
  updated_at: string;
}
```

## 常见场景示例

### 场景 1：实现文件打开时记录访问历史

```typescript
import { recentApi } from './api/phase1';

async function openFile(fileId: string, filePath: string, fileName: string) {
  // 打开文件的实际逻辑
  await actualOpenFile(fileId);
  
  // 记录访问历史
  await recentApi.recordAccess(fileId, 'file', filePath, fileName);
}
```

### 场景 2：项目切换时保存和恢复会话

```typescript
import { useWorkspaceSession } from './hooks';

function ProjectSwitcher() {
  const currentSession = useWorkspaceSession(currentProjectId);
  
  async function switchProject(newProjectId: string) {
    // 保存当前项目状态
    await currentSession.saveSession({
      openFilesJson: JSON.stringify(openFiles),
      activeFileId: activeFile?.id,
      terminalTabsJson: JSON.stringify(terminals),
      layoutConfig: JSON.stringify(layout),
      scrollPositions: JSON.stringify(scrolls)
    });
    
    // 切换项目
    setCurrentProjectId(newProjectId);
    
    // 加载新项目的会话（hook 会自动触发）
  }
}
```

### 场景 3：右键菜单添加标签管理

```typescript
import { useTags } from './hooks';

function FileContextMenu({ fileId }: { fileId: string }) {
  const { tags, toggleTag, isTagAttached } = useTags(fileId, 'file');
  const [showTagManager, setShowTagManager] = useState(false);

  return (
    <ContextMenu>
      <MenuItem onClick={() => setShowTagManager(true)}>
        Manage Tags
      </MenuItem>
      
      {showTagManager && (
        <TagManager
          resourceId={fileId}
          resourceType="file"
          onTagsChange={() => setShowTagManager(false)}
        />
      )}
    </ContextMenu>
  );
}
```

### 场景 4：命令面板注册自定义命令

```typescript
import { useCommandPalette } from './hooks';

function App() {
  const palette = useCommandPalette();
  
  const customCommands: Command[] = [
    {
      id: 'custom-export',
      label: 'Export Project',
      category: 'Custom',
      keywords: ['export', 'save', 'backup'],
      action: async () => {
        await exportProject();
      }
    },
    {
      id: 'custom-import',
      label: 'Import Project',
      category: 'Custom',
      keywords: ['import', 'load', 'restore'],
      shortcut: 'Ctrl+Shift+I',
      action: async () => {
        await importProject();
      }
    }
  ];

  return <CommandPalette {...palette} commands={customCommands} />;
}
```

## 测试验证

### 运行 Rust 单元测试

```bash
cd src-tauri
cargo test
```

预期输出：34 个测试全部通过

### 前端组件测试

```bash
npm run test
```

### 手动验证清单

- [ ] Ctrl+K 能打开命令面板
- [ ] 命令面板支持模糊搜索
- [ ] 标签可以创建、编辑、删除
- [ ] 标签可以关联到文件/文件夹
- [ ] 最近访问列表显示正确
- [ ] 点击最近访问条目能打开对应资源
- [ ] 项目切换时状态能正确保存和恢复
- [ ] Git 状态能正确显示分支和文件变更
- [ ] 暂存区和非暂存区文件分类正确

## 性能优化建议

1. **防抖 autoSave**：在 useWorkspaceSession 中添加 debounce，避免频繁写入数据库
2. **虚拟滚动**：RecentItemsList 数据量大时使用虚拟列表
3. **命令缓存**：CommandPalette 的命令列表可以缓存，避免每次打开重新计算
4. **增量更新**：Git 文件状态变化时只更新变化的文件，而不是全量刷新
5. **索引优化**：为高频查询字段（如 last_accessed, access_count）添加数据库索引

## 故障排查

### 问题：命令面板打不开
- 检查是否正确注册了 Ctrl+K 监听器
- 确认 useCommandPalette hook 已在根组件初始化

### 问题：标签关联失败
- 检查 resource_id 和 resource_type 是否正确
- 确认 tags 和 resource_tags 表已创建

### 问题：最近访问列表为空
- 检查是否调用了 recordAccess
- 确认 recent_items 表已创建
- 查看数据库中是否有记录：`SELECT * FROM recent_items;`

### 问题：会话恢复失败
- 检查 JSON 字段格式是否正确
- 确认 project_id 匹配
- 查看 workspace_sessions 表：`SELECT * FROM workspace_sessions WHERE project_id = ?;`

### 问题：Git 状态不更新
- 检查 repo_path 是否是有效的 Git 仓库
- 确认调用了 updateRepositoryStatus
- 手动刷新：调用 reload() 方法

## 文件位置速查

| 类型 | 路径 |
|------|------|
| 数据库迁移 | `src-tauri/migrations/0010_unified_workspace.sql` |
| 服务层 | `src-tauri/src/services/` |
| 命令层 | `src-tauri/src/commands/` |
| 类型定义 | `src/types/phase1.ts` |
| API 封装 | `src/api/phase1.ts` |
| React 组件 | `src/components/` |
| Custom Hooks | `src/hooks/` |
| 集成示例 | `src/examples/Phase1Integration.tsx` |
| 文档 | `docs/` |
