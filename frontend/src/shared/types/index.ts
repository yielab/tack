// Single source of frontend DTOs mirroring the backend.

// ─── Projects ──────────────────────────────────────────────────────────────

export interface Project {
  id: string;
  workspace_id: string;
  name: string;
  description?: string;
  project_type: ProjectType;
  vocabulary: Record<string, string>;
  workflow: WorkflowConfig;
  created_at: string;
  updated_at: string;
  archived: boolean;
}

export type ProjectType =
  | 'software'
  | 'web'
  | 'mobile'
  | 'construction'
  | 'personal'
  | 'homework'
  | 'maintenance'
  | 'custom';

export interface WorkflowConfig {
  workflow_type: string;
  statuses: WorkflowStatus[];
  transitions?: Array<{ from: string; to: string }>;
}

export interface WorkflowStatus {
  name: string;
  category: 'todo' | 'in_progress' | 'done';
  wip_limit?: number;
  order: number;
}

export interface UpdateProject {
  name?: string;
  description?: string;
  vocabulary?: Record<string, string>;
  workflow?: WorkflowConfig;
  archived?: boolean;
}

export interface CreateProject {
  name: string;
  description?: string;
  /** Selects the default workflow and vocabulary. Required by the API. */
  project_type: ProjectType;
  /** Optional named template to seed from. */
  template?: string;
}

// ─── Items ─────────────────────────────────────────────────────────────────

export interface Item {
  id: string;
  project_id: string;
  parent_id?: string;
  title: string;
  description?: string;
  item_type: ItemType;
  status: string;
  priority: Priority;
  estimate?: number;
  estimate_unit: EstimateUnit;
  tags: string[];
  sort_order: number;
  sprint_id?: string;
  due_date?: string;
  started_at?: string;
  completed_at?: string;
  created_at: string;
  updated_at: string;
}

export type ItemType =
  | 'epic'
  | 'feature'
  | 'task'
  | 'subtask'
  | 'bug'
  | 'requirement'
  | { custom: string };

export type Priority = 'critical' | 'high' | 'medium' | 'low' | 'none';

export type EstimateUnit = 'story_points' | 'hours' | 'days' | 'custom';

export interface CreateItem {
  title: string;
  description?: string;
  item_type: ItemType;
  status?: string;
  priority?: Priority;
  estimate?: number;
  estimate_unit?: EstimateUnit;
  tags?: string[];
  parent_id?: string;
  sprint_id?: string;
  due_date?: string;
}

export interface UpdateItem {
  title?: string;
  description?: string;
  status?: string;
  priority?: Priority;
  estimate?: number;
  tags?: string[];
  parent_id?: string;
  sprint_id?: string | null;
  due_date?: string | null;
}

// ─── Sprints ───────────────────────────────────────────────────────────────

export interface Sprint {
  id: string;
  project_id: string;
  name: string;
  goal?: string;
  start_date?: string;
  end_date?: string;
  status: SprintStatus;
  created_at: string;
  updated_at: string;
}

export type SprintStatus = 'planning' | 'active' | 'review' | 'closed';

// ─── Board ─────────────────────────────────────────────────────────────────

export interface BoardState {
  columns: BoardColumn[];
}

export interface BoardColumn {
  status: string;
  items: Item[];
  wip_limit?: number;
  wip_exceeded: boolean;
}

// ─── Custom fields (definitions) ───────────────────────────────────────────

export type CustomFieldType =
  | 'text'
  | 'long_text'
  | 'number'
  | 'date'
  | 'boolean'
  | 'select'
  | 'multi_select'
  | 'url'
  | 'email';

export interface CustomField {
  id: string;
  project_id: string;
  name: string;
  field_type: CustomFieldType; // Fixed: strict union instead of string
  description: string | null;
  required: boolean;
  default_value: unknown;
  options: string[] | null;
  validation: unknown;
  created_at: string;
  updated_at: string;
}

export interface CreateCustomField {
  name: string;
  field_type: CustomFieldType; // Fixed: strict union instead of string
  description?: string | null;
  required?: boolean;
  options?: string[] | null; // Fixed: added null
}

export interface UpdateCustomField {
  name?: string;
  field_type?: CustomFieldType; // Fixed: strict union instead of string
  description?: string | null;
  required?: boolean;
  options?: string[] | null; // Fixed: added null
}

// ─── Project templates ─────────────────────────────────────────────────────

export interface TemplateCustomField {
  name: string;
  field_type: CustomFieldType; // Fixed: strict union instead of string
  description?: string | null;
  required?: boolean;
  options?: string[] | null;
}

export interface TemplateBoardColumn {
  status: string;
  wip_limit?: number | null;
  collapsed?: boolean;
}

export interface TemplateBoardConfig {
  name: string;
  description?: string | null;
  columns: TemplateBoardColumn[];
  is_default?: boolean;
}

export interface ProjectTemplate {
  id: string;
  name: string;
  description: string | null;
  project_type: ProjectType; // Fixed: explicit type parity
  vocabulary: Record<string, string> | null;
  workflow: WorkflowConfig | null;
  custom_fields: TemplateCustomField[] | null;
  default_boards: TemplateBoardConfig[] | null;
  is_builtin: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateTemplate {
  name: string;
  description?: string | null;
  project_type: ProjectType; // Fixed: explicit type parity
  vocabulary?: Record<string, string> | null;
  workflow?: WorkflowConfig | null;
  custom_fields?: TemplateCustomField[] | null;
  default_boards?: TemplateBoardConfig[] | null;
}

export interface CreateProjectFromTemplate {
  name: string;
  description?: string | null;
}

// ─── Comments ──────────────────────────────────────────────────────────────

export type CommentType = 'comment' | 'status_change' | 'edit' | 'system';

export interface Comment {
  id: string;
  item_id: string;
  author: string | null;
  content: string;
  comment_type: CommentType;
  created_at: string;
  updated_at: string;
}

export interface CreateComment {
  content: string;
  author?: string;
}

// ─── Dependencies ──────────────────────────────────────────────────────────

export type DependencyType =
  | 'blocks'
  | 'is_blocked_by'
  | 'relates_to'
  | 'duplicates';

export interface Dependency {
  id: string;
  source_item_id: string;
  target_item_id: string;
  dependency_type: DependencyType;
  created_at: string;
}

export interface CreateDependency {
  target_item_id: string;
  dependency_type: DependencyType;
}

// ─── Roles / specialties ───────────────────────────────────────────────────

export interface Role {
  id: string;
  project_id: string;
  name: string;
  color: string;
  icon: string | null;
  created_at: string;
}

export interface CreateRole {
  name: string;
  color?: string;
  icon?: string | null; // Fixed: added null
}

// ─── Attachments ───────────────────────────────────────────────────────────

export interface Attachment {
  id: string;
  item_id: string;
  filename: string;
  mime_type: string;
  storage_path: string;
  size_bytes: number;
  uploaded_at: string;
}

// ─── Custom field values (per item) ────────────────────────────────────────

export interface CustomFieldValue {
  id: string;
  item_id: string;
  field_id: string;
  value: unknown;
  created_at: string;
  updated_at: string;
}

// ─── Realtime board events (WebSocket) ─────────────────────────────────────
//
// Mirrors `BoardEvent` in crates/tack-api/src/handlers/websocket.rs, which is
// serialized with `#[serde(tag = "type", rename_all = "snake_case")]`.

export type BoardEvent =
  | { type: 'item_created'; project_id: string; item_id: string; status: string }
  | {
      type: 'item_updated';
      project_id: string;
      item_id: string;
      old_status: string | null;
      new_status: string;
    }
  | { type: 'item_deleted'; project_id: string; item_id: string }
  | { type: 'board_config_updated'; project_id: string }
  | { type: 'sprint_updated'; project_id: string; sprint_id: string }
  | { type: 'ping' };

export type BoardEventType = BoardEvent['type'];
