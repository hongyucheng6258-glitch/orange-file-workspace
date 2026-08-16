/// 前端通过 `listen` 订阅的统一事件名。
pub const EVENT_RESOURCE_CHANGED: &str = "resource-changed";
#[allow(dead_code)] // 预留事件，供外部位置失效通知使用
pub const EVENT_LOCATION_INVALIDATED: &str = "location-invalidated";
pub const EVENT_TRASH_UPDATED: &str = "trash-updated";
pub const EVENT_TASK_PROGRESS: &str = "task-progress";
pub const EVENT_PROJECT_PROCESS_STATUS: &str = "project-process://status";
pub const EVENT_PROJECT_PROCESS_OUTPUT: &str = "project-process://output";
pub const EVENT_PROJECT_PROCESS_EXITED: &str = "project-process://exited";
pub const EVENT_PROJECT_PROCESS_ERROR: &str = "project-process://error";
pub const EVENT_PROJECT_PREVIEW_READY: &str = "project-preview://ready";
