/** 与 Rust 端模型对应的类型定义。 */

export type ResourceKind = "file" | "folder" | "page" | "project";
export type SourceType = "managed" | "external";

export interface Resource {
  id: string;
  kind: ResourceKind;
  name: string;
  parent_id: string | null;
  is_favorite: boolean;
  is_deleted: boolean;
  created_at: number;
  updated_at: number;
  deleted_at: number | null;
}

export interface ResourceLocation {
  id: string;
  resource_id: string;
  source_type: SourceType;
  path: string;
  canonical_path: string | null;
  file_size: number | null;
  modified_at: number | null;
  created_at: number;
  last_verified_at: number | null;
  content_hash: string | null;
  hash_algorithm: string | null;
  is_available: boolean;
}

export interface FileMetadata {
  resource_id: string;
  extension: string | null;
  mime_type: string | null;
  size_bytes: number;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  encoding: string | null;
  line_count: number | null;
  is_binary: boolean;
  preview_kind: string | null;
  metadata_json: string | null;
}

export interface ResourceDetail {
  resource: Resource;
  locations: ResourceLocation[];
}

export interface RecentItem {
  id: string;
  kind: ResourceKind;
  name: string;
  parent_id: string | null;
  is_favorite: boolean;
  updated_at: number;
  path: string | null;
  source_type: SourceType | null;
  file_size: number | null;
}

export interface DashboardStats {
  totalFiles: number;
  totalFolders: number;
  totalPages: number;
  totalProjects: number;
  favorites: number;
  trash: number;
  totalSize: number;
  recent: RecentItem[];
}

export interface AppEnvironment {
  name: string;
  version: string;
  data_dir: string;
  db_path: string;
}

export interface SystemOverview {
  device_name: string | null;
  os_name: string | null;
  os_version: string | null;
  kernel_version: string | null;
  manufacturer: string | null;
  product_name: string | null;
  bios_version: string | null;
  cpu_brand: string | null;
  cpu_vendor: string | null;
  cpu_cores: number;
  cpu_threads: number;
  cpu_frequency: number;
  cpu_usage: number;
  mem_total: number;
  mem_used: number;
  mem_percent: number;
  swap_total: number;
  swap_used: number;
  uptime: number;
  boot_time: number;
  process_count: number;
  disk_count: number;
}

export interface StorageInfo {
  name: string;
  mount_point: string;
  kind: string;
  file_system: string;
  total_space: number;
  available_space: number;
  is_removable: boolean;
}

export interface ProcessInfo {
  pid: number;
  name: string;
  cpu_usage: number;
  memory: number;
  path: string | null;
  status: string;
  start_time: number;
  user: string | null;
}

export interface NetworkRate {
  name: string;
  down_rate: number;
  up_rate: number;
  total_down: number;
  total_up: number;
}

export interface SystemSnapshot {
  cpu_usage: number;
  cpu_per_core: number[];
  mem_total: number;
  mem_used: number;
  mem_percent: number;
  swap_total: number;
  swap_used: number;
  networks: NetworkRate[];
  timestamp: number;
}

export interface ReportOutput {
  json_path: string;
  html_path: string;
}

export interface GpuInfo {
  name: string;
  vendor: string | null;
  vram_bytes: number;
  driver_version: string | null;
  driver_date: string | null;
}

export interface NetworkAdapterInfo {
  name: string;
  friendly_name: string | null;
  mac: string | null;
  ipv4: string[];
  ipv6: string[];
  status: string;
  speed_bps: number;
}

export interface ServiceInfo {
  name: string;
  display_name: string;
  state: string;
  start_type: string;
}

export interface DriverInfo {
  name: string;
  display_name: string;
  state: string;
  start_type: string;
}

export interface StartupItemInfo {
  name: string;
  command: string;
  source: string;
}

export interface BatteryInfo {
  ac_status: string;
  charging: boolean;
  percent: number | null;
  life_time_secs: number | null;
}

export interface SecurityStatus {
  firewall_standard: boolean;
  firewall_domain: boolean;
  firewall_public: boolean;
  defender_running: boolean;
  windows_update_running: boolean;
  secure_boot: boolean;
  uac_enabled: boolean;
  running_as_admin: boolean;
}

export interface HealthItem {
  level: "ok" | "warning" | "danger";
  title: string;
  detail: string;
}

export interface TemperatureInfo {
  label: string;
  temperature_c: number | null;
  max_c: number | null;
}

export interface DiskHealthInfo {
  name: string;
  mount_point: string;
  health_status: string;
  health_code: number;
}

export interface GpuMetric {
  name: string;
  vram_total: number;
  vram_used: number;
  vram_percent: number;
}

export interface AlertRule {
  enabled: boolean;
  threshold: number;
}

export interface AlertRules {
  cpu: AlertRule;
  mem: AlertRule;
  disk: AlertRule;
  temp: AlertRule;
}

export interface ToolCleanResult {
  deleted_files: number;
  freed_bytes: number;
  skipped_files: number;
}

export type CleanupMode = "safe" | "deep";
export type CleanupRisk = "low" | "medium" | "high";
export type CleanupItemStatus = "ready" | "requires_admin" | "completed" | "failed" | "partial";

export interface CleanupScanItem {
  id: string;
  name: string;
  description: string;
  risk: CleanupRisk;
  default_selected: boolean;
  requires_admin: boolean;
  files: number;
  bytes: number;
  status: "ready" | "requires_admin" | "partial";
  message: string | null;
}

export interface CleanupRunItem {
  id: string;
  name: string;
  status: CleanupItemStatus;
  deleted_files: number;
  freed_bytes: number;
  skipped_files: number;
  message: string | null;
}

export interface CleanupRunResult {
  status: "completed" | "completed_with_errors" | "partial";
  deleted_files: number;
  freed_bytes: number;
  skipped_files: number;
  items: CleanupRunItem[];
}

export interface AppUsageStat {
  app_id: number;
  display_name: string;
  process_name: string;
  canonical_path: string;
  active_seconds: number;
  last_active_at: number;
  percentage: number;
}

export interface DailyTotal {
  date_ymd: number;
  active_seconds: number;
}

export interface AppUsageSummary {
  range_label: string;
  total_active_seconds: number;
  apps: AppUsageStat[];
  daily_totals: DailyTotal[];
}

export interface AppUsageStatus {
  paused: boolean;
  idle_threshold_secs: number;
}

export interface AppErrorPayload {
  code: string;
  message: string;
}

export type CommandResult<T> = T;
