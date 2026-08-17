// Phase 1 feature types: Tags, Recent Items, Workspace Sessions, Git, Command Palette

// ============ Tags ============
export interface Tag {
  id: string;
  name: string;
  color: string | null;
  created_at: number;
  resource_count?: number;
}

export interface ResourceTag {
  id: string;
  resource_id: string;
  tag_id: string;
  created_at: number;
}

// ============ Recent Items ============
export interface RecentItem {
  id: string;
  resource_id: string;
  resource_type: string; // 'file' | 'folder' | 'page' | 'project'
  access_count: number;
  last_accessed_at: number;
  created_at: number;
}

// ============ Workspace Sessions ============
export interface WorkspaceSession {
  id: string;
  project_id: string;
  open_files_json: string;
  active_file_id: string | null;
  terminal_tabs_json: string;
  active_terminal_index: number | null;
  running_tasks_json: string;
  panel_layout_json: string | null;
  scroll_positions_json: string | null;
  created_at: number;
  updated_at: number;
  last_restored_at: number | null;
}

export interface OpenFileInfo {
  file_id: string;
  file_path: string;
  scroll_position?: number;
  cursor_position?: { line: number; column: number };
}

export interface TerminalTabInfo {
  terminal_id: string;
  cwd: string;
  shell: string;
}

export interface RunningTaskInfo {
  task_id: string;
  task_name: string;
  command: string;
}

export interface PanelLayout {
  sidebar_width?: number;
  terminal_height?: number;
  editor_split?: 'horizontal' | 'vertical' | 'none';
}

// ============ Git ============
export interface GitRepository {
  id: string;
  project_id: string;
  repo_path: string;
  current_branch: string | null;
  remote_url: string | null;
  last_fetch_at: number | null;
  has_uncommitted: boolean;
  ahead_count: number;
  behind_count: number;
  created_at: number;
  updated_at: number;
}

export interface GitFileStatus {
  id: string;
  repo_id: string;
  file_path: string;
  status: 'untracked' | 'modified' | 'added' | 'deleted' | 'renamed' | 'conflicted';
  staged: boolean;
  created_at: number;
  updated_at: number;
}

// ============ Command Palette ============
export interface CommandHistory {
  id: string;
  command_id: string;
  command_label: string;
  command_category: string;
  execution_count: number;
  last_executed_at: number;
  created_at: number;
}

export interface CommandStatistics {
  total_commands: number;
  total_executions: number;
  most_used_category: string | null;
}

export interface Command {
  id: string;
  label: string;
  category: string;
  keywords?: string[];
  icon?: string;
  shortcut?: string;
  action: () => void | Promise<void>;
}
