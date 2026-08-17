import { invoke } from '@tauri-apps/api/core';
import type {
  Tag,
  RecentItem,
  WorkspaceSession,
  GitRepository,
  GitFileStatus,
  CommandHistory,
  CommandStatistics,
} from '../types/phase1';

// ============ Tags API ============
export const tagsApi = {
  createTag: (name: string, color?: string) =>
    invoke<Tag>('create_tag', { name, color }),

  listTags: () => invoke<Tag[]>('list_tags'),

  getTag: (tagId: string) => invoke<Tag>('get_tag', { tagId }),

  updateTag: (tagId: string, name?: string, color?: string) =>
    invoke<void>('update_tag', { tagId, name, color }),

  deleteTag: (tagId: string) => invoke<void>('delete_tag', { tagId }),

  addTagToResource: (tagId: string, resourceId: string) =>
    invoke<void>('add_tag_to_resource', { tagId, resourceId }),

  removeTagFromResource: (tagId: string, resourceId: string) =>
    invoke<void>('remove_tag_from_resource', { tagId, resourceId }),

  getResourceTags: (resourceId: string) =>
    invoke<Tag[]>('get_resource_tags', { resourceId }),

  getResourcesByTag: (tagId: string) =>
    invoke<string[]>('get_resources_by_tag', { tagId }),
};

// ============ Recent Items API ============
export const recentApi = {
  recordAccess: (resourceId: string, resourceType: string) =>
    invoke<void>('record_resource_access', { resourceId, resourceType }),

  getRecentItems: (resourceTypeFilter?: string, limit: number = 20) =>
    invoke<RecentItem[]>('get_recent_items', { resourceTypeFilter, limit }),

  removeItem: (itemId: string) =>
    invoke<void>('remove_recent_item', { itemId }),

  clearAll: () => invoke<void>('clear_recent_items'),

  cleanupOld: (olderThanDays: number) =>
    invoke<void>('cleanup_old_recent_items', { olderThanDays }),
};

// ============ Workspace Sessions API ============
export const workspaceSessionsApi = {
  save: (
    projectId: string,
    openFilesJson?: string,
    activeFileId?: string,
    terminalTabsJson?: string,
    activeTerminalIndex?: number,
    runningTasksJson?: string,
    panelLayoutJson?: string,
    scrollPositionsJson?: string
  ) =>
    invoke<WorkspaceSession>('save_workspace_session', {
      projectId,
      openFilesJson,
      activeFileId,
      terminalTabsJson,
      activeTerminalIndex,
      runningTasksJson,
      panelLayoutJson,
      scrollPositionsJson,
    }),

  get: (projectId: string) =>
    invoke<WorkspaceSession | null>('get_workspace_session', { projectId }),

  delete: (projectId: string) =>
    invoke<void>('delete_workspace_session', { projectId }),

  list: () => invoke<WorkspaceSession[]>('list_workspace_sessions'),

  markRestored: (sessionId: string) =>
    invoke<void>('mark_session_restored', { sessionId }),
};

// ============ Git API ============
export const gitApi = {
  registerRepository: (
    projectId: string,
    repoPath: string,
    currentBranch?: string,
    remoteUrl?: string
  ) =>
    invoke<GitRepository>('register_git_repository', {
      projectId,
      repoPath,
      currentBranch,
      remoteUrl,
    }),

  getRepository: (projectId: string) =>
    invoke<GitRepository | null>('get_git_repository', { projectId }),

  updateStatus: (
    repoId: string,
    currentBranch?: string,
    hasUncommitted: boolean = false,
    aheadCount: number = 0,
    behindCount: number = 0
  ) =>
    invoke<void>('update_git_repository_status', {
      repoId,
      currentBranch,
      hasUncommitted,
      aheadCount,
      behindCount,
    }),

  markFetched: (repoId: string) =>
    invoke<void>('mark_git_repository_fetched', { repoId }),

  saveFileStatus: (
    repoId: string,
    filePath: string,
    status: string,
    staged: boolean
  ) =>
    invoke<GitFileStatus>('save_git_file_status', {
      repoId,
      filePath,
      status,
      staged,
    }),

  getFileStatus: (repoId: string, filePath: string) =>
    invoke<GitFileStatus | null>('get_git_file_status', { repoId, filePath }),

  listFileStatuses: (
    repoId: string,
    statusFilter?: string,
    stagedFilter?: boolean
  ) =>
    invoke<GitFileStatus[]>('list_git_file_statuses', {
      repoId,
      statusFilter,
      stagedFilter,
    }),

  clearFileStatuses: (repoId: string) =>
    invoke<void>('clear_git_file_statuses', { repoId }),

  deleteRepository: (repoId: string) =>
    invoke<void>('delete_git_repository', { repoId }),
};

// ============ Command Palette API ============
export const commandPaletteApi = {
  recordExecution: (commandId: string, commandLabel: string, commandCategory: string) =>
    invoke<void>('record_command_execution', {
      commandId,
      commandLabel,
      commandCategory,
    }),

  getFrequentCommands: (limit: number = 10) =>
    invoke<CommandHistory[]>('get_frequent_commands', { limit }),

  getRecentCommands: (limit: number = 10) =>
    invoke<CommandHistory[]>('get_recent_commands', { limit }),

  getCommandsByCategory: (category: string, limit: number = 20) =>
    invoke<CommandHistory[]>('get_commands_by_category', { category, limit }),

  searchCommands: (query: string, limit: number = 20) =>
    invoke<CommandHistory[]>('search_commands', { query, limit }),

  clearHistory: () => invoke<void>('clear_command_history'),

  deleteCommand: (commandId: string) =>
    invoke<void>('delete_command_history', { commandId }),

  getStatistics: () => invoke<CommandStatistics>('get_command_statistics'),
};
