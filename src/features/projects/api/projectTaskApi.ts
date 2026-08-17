/**
 * 项目任务 API — 封装 Tauri 命令调用
 */

import { call } from "../../../lib/tauri";
import type { Resource } from "../../../lib/types";
import type {
  ProjectTask,
  ProjectTaskLink,
  TaskPriority,
  TaskLinkType,
  UpdateTaskParams,
} from "../types/projectTask";

export async function createProjectTask(
  projectId: string,
  title: string,
  description?: string,
  priority?: TaskPriority,
): Promise<ProjectTask> {
  return call<ProjectTask>("create_project_task", {
    projectId,
    title,
    description: description ?? null,
    priority: priority ?? null,
  });
}

export async function listProjectTasks(projectId: string): Promise<ProjectTask[]> {
  return call<ProjectTask[]>("list_project_tasks", { projectId });
}

export async function updateProjectTask(
  id: string,
  params: UpdateTaskParams,
): Promise<ProjectTask | null> {
  return call<ProjectTask | null>("update_project_task", {
    id,
    title: params.title ?? null,
    description: params.description === undefined ? null : params.description,
    status: params.status ?? null,
    priority: params.priority ?? null,
    dueDate: params.due_date === undefined ? null : params.due_date,
  });
}

export async function deleteProjectTask(id: string): Promise<void> {
  return call<void>("delete_project_task", { id });
}

export async function reorderProjectTasks(taskIds: string[]): Promise<void> {
  return call<void>("reorder_project_tasks", { taskIds });
}

export async function linkTaskResource(
  taskId: string,
  resourceId: string,
  linkType?: TaskLinkType,
): Promise<ProjectTaskLink> {
  return call<ProjectTaskLink>("link_task_resource", {
    taskId,
    resourceId,
    linkType: linkType ?? null,
  });
}

export async function unlinkTaskResource(
  taskId: string,
  resourceId: string,
): Promise<void> {
  return call<void>("unlink_task_resource", { taskId, resourceId });
}

export async function listTaskLinks(taskId: string): Promise<ProjectTaskLink[]> {
  return call<ProjectTaskLink[]>("list_task_links", { taskId });
}

export async function listLinksByResource(
  resourceId: string,
): Promise<ProjectTaskLink[]> {
  return call<ProjectTaskLink[]>("list_links_by_resource", { resourceId });
}

/** get_resource 返回的复合类型 */
interface ResourceWithLocations {
  resource: Resource;
  locations: unknown[];
}

/** 查询任务关联的资源详情 */
export async function listLinkedResources(
  taskId: string,
): Promise<Resource[]> {
  const links = await listTaskLinks(taskId);
  const resources: Resource[] = [];
  for (const link of links) {
    try {
      const resp = await call<ResourceWithLocations>("get_resource", { id: link.resource_id });
      resources.push(resp.resource);
    } catch {
      // resource may have been deleted
    }
  }
  return resources;
}
