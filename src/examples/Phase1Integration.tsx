import React from 'react';
import { CommandPalette } from '../components/CommandPalette';
import { TagManager } from '../components/TagManager';
import { RecentItemsList } from '../components/RecentItemsList';
import { GitStatus } from '../components/GitStatus';
import { useCommandPalette, useTags, useRecentItems, useGitStatus } from '../hooks';
import type { Command } from '../types/phase1';

/**
 * Phase 1 集成示例
 * 
 * 本文件展示如何在 Orange 主应用中集成 Phase 1 的所有功能：
 * 1. 命令面板 (Ctrl+K)
 * 2. 标签管理
 * 3. 最近访问
 * 4. Git 状态
 * 5. 工作区会话（见 useWorkspaceSession hook）
 */

export const Phase1Integration: React.FC = () => {
  // 命令面板 hook - 提供全局 Ctrl+K 快捷键
  const commandPalette = useCommandPalette();

  // 标签管理 hook - 可选传入 resourceId 和 resourceType 来管理特定资源的标签
  const tags = useTags();

  // 最近访问 hook - 可选传入 resourceType 过滤特定类型
  const recentItems = useRecentItems();

  // Git 状态 hook - 需要 projectId 和 repoPath
  const gitStatus = useGitStatus('example-project-id', 'C:\\projects\\orange');

  // 定义命令面板的命令列表
  const commands: Command[] = [
    {
      id: 'open-file',
      label: 'Open File',
      category: 'File',
      keywords: ['open', 'file', 'load'],
      action: async () => {
        console.log('Opening file picker...');
        // 实际实现：调用 Tauri 文件选择器
      }
    },
    {
      id: 'new-file',
      label: 'New File',
      category: 'File',
      keywords: ['new', 'create', 'file'],
      action: async () => {
        console.log('Creating new file...');
        // 实际实现：创建新文件逻辑
      }
    },
    {
      id: 'search-files',
      label: 'Search Files',
      category: 'Search',
      keywords: ['search', 'find', 'grep'],
      shortcut: 'Ctrl+Shift+F',
      action: async () => {
        console.log('Opening search panel...');
        // 实际实现：打开搜索面板
      }
    },
    {
      id: 'git-commit',
      label: 'Git: Commit',
      category: 'Git',
      keywords: ['git', 'commit', 'save'],
      action: async () => {
        console.log('Opening commit dialog...');
        // 实际实现：打开 Git commit 对话框
      }
    },
    {
      id: 'toggle-sidebar',
      label: 'Toggle Sidebar',
      category: 'View',
      keywords: ['sidebar', 'toggle', 'view'],
      shortcut: 'Ctrl+B',
      action: async () => {
        console.log('Toggling sidebar...');
        // 实际实现：切换侧边栏显示
      }
    }
  ];

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      gap: '2rem',
      padding: '2rem',
      backgroundColor: '#F7F4EF',
      minHeight: '100vh',
      fontFamily: 'Inter, sans-serif'
    }}>
      <header style={{
        marginBottom: '1rem'
      }}>
        <h1 style={{
          fontSize: '2rem',
          fontFamily: 'Fraunces, serif',
          fontWeight: 400,
          letterSpacing: '-0.02em',
          color: '#1F2421',
          margin: 0
        }}>
          Orange Phase 1 <em style={{ color: '#C4612F' }}>Integration</em>
        </h1>
        <p style={{
          fontSize: '0.875rem',
          color: '#5C635D',
          marginTop: '0.5rem'
        }}>
          Press <kbd style={{
            padding: '0.125rem 0.5rem',
            backgroundColor: '#FBF9F5',
            border: '1px solid #E7E1D7',
            borderRadius: '4px',
            fontSize: '0.75rem',
            fontFamily: 'monospace'
          }}>Ctrl+K</kbd> to open command palette
        </p>
      </header>

      {/* 命令面板 - 全局组件，通过 Ctrl+K 触发 */}
      <CommandPalette
        isOpen={commandPalette.isOpen}
        onClose={commandPalette.close}
        commands={commands}
      />

      <div style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(auto-fit, minmax(400px, 1fr))',
        gap: '1.5rem'
      }}>
        {/* 最近访问列表 */}
        <section>
          <h2 style={{
            fontSize: '1.25rem',
            fontWeight: 500,
            color: '#1F2421',
            marginBottom: '1rem'
          }}>
            Recent Items
          </h2>
          <RecentItemsList
            limit={10}
            onItemClick={(item) => {
              console.log('Opening recent item:', item);
              // 实际实现：根据 resource_type 打开相应资源
            }}
          />
        </section>

        {/* 标签管理 */}
        <section>
          <h2 style={{
            fontSize: '1.25rem',
            fontWeight: 500,
            color: '#1F2421',
            marginBottom: '1rem'
          }}>
            Tag Manager
          </h2>
          <TagManager
            resourceId="example-file-id"
            embedded
          />
        </section>

        {/* Git 状态 */}
        <section>
          <h2 style={{
            fontSize: '1.25rem',
            fontWeight: 500,
            color: '#1F2421',
            marginBottom: '1rem'
          }}>
            Git Status
          </h2>
          <GitStatus
            projectId="example-project-id"
            repoPath="C:\\projects\\orange"
            onRefresh={() => {
              console.log('Git status refreshed');
            }}
          />
        </section>
      </div>

      {/* 集成说明 */}
      <section style={{
        marginTop: '2rem',
        padding: '1.5rem',
        backgroundColor: '#FBF9F5',
        borderRadius: '8px',
        border: '1px solid #E7E1D7'
      }}>
        <h3 style={{
          fontSize: '1rem',
          fontWeight: 500,
          color: '#1F2421',
          marginBottom: '1rem'
        }}>
          集成指南
        </h3>
        <ul style={{
          fontSize: '0.875rem',
          color: '#5C635D',
          lineHeight: 1.6,
          paddingLeft: '1.5rem'
        }}>
          <li>命令面板：已自动监听 Ctrl+K 快捷键，可在任何页面触发</li>
          <li>标签管理：在文件/文件夹/页面的右键菜单中集成 TagManager 组件</li>
          <li>最近访问：在仪表板或侧边栏中显示 RecentItemsList 组件</li>
          <li>Git 状态：在项目详情页或底部状态栏中集成 GitStatus 组件</li>
          <li>工作区会话：使用 useWorkspaceSession hook 实现项目状态的自动保存和恢复</li>
        </ul>
      </section>

      {/* 数据统计 */}
      <section style={{
        display: 'flex',
        gap: '1rem',
        flexWrap: 'wrap'
      }}>
        <div style={{
          flex: 1,
          minWidth: '150px',
          padding: '1rem',
          backgroundColor: '#FFFFFF',
          borderRadius: '8px',
          border: '1px solid #E7E1D7'
        }}>
          <div style={{
            fontSize: '0.75rem',
            color: '#5C635D',
            marginBottom: '0.25rem'
          }}>
            Total Tags
          </div>
          <div style={{
            fontSize: '1.5rem',
            fontWeight: 500,
            color: '#1F2421'
          }}>
            {tags.tags.length}
          </div>
        </div>

        <div style={{
          flex: 1,
          minWidth: '150px',
          padding: '1rem',
          backgroundColor: '#FFFFFF',
          borderRadius: '8px',
          border: '1px solid #E7E1D7'
        }}>
          <div style={{
            fontSize: '0.75rem',
            color: '#5C635D',
            marginBottom: '0.25rem'
          }}>
            Recent Items
          </div>
          <div style={{
            fontSize: '1.5rem',
            fontWeight: 500,
            color: '#1F2421'
          }}>
            {recentItems.items.length}
          </div>
        </div>

        <div style={{
          flex: 1,
          minWidth: '150px',
          padding: '1rem',
          backgroundColor: '#FFFFFF',
          borderRadius: '8px',
          border: '1px solid #E7E1D7'
        }}>
          <div style={{
            fontSize: '0.75rem',
            color: '#5C635D',
            marginBottom: '0.25rem'
          }}>
            Git Files Changed
          </div>
          <div style={{
            fontSize: '1.5rem',
            fontWeight: 500,
            color: gitStatus.fileStatuses.length > 0 ? '#C4612F' : '#1F2421'
          }}>
            {gitStatus.fileStatuses.length}
          </div>
        </div>
      </section>
    </div>
  );
};
