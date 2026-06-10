// Single source of frontend DTOs mirroring the backend (T-501 seed; T-502
// completes parity). Re-exports the legacy `types/api.ts` shapes and adds the
// DTOs that previously lived inline in page components.

// 1. Import the legacy types so they can be referenced inside this file
import type {
  Project,
  ProjectType,
  WorkflowConfig,
  WorkflowStatus,
  UpdateProject,
  CreateProject,
  Item,
  ItemType,
  Priority,
  EstimateUnit,
  CreateItem,
  UpdateItem,
  Sprint,
  SprintStatus,
  BoardState,
  BoardColumn,
} from '../../types/api';

// 2. Re-export them to maintain the single source of truth for the frontend
export type {
  Project,
  ProjectType,
  WorkflowConfig,
  WorkflowStatus,
  UpdateProject,
  CreateProject,
  Item,
  ItemType,
  Priority,
  EstimateUnit,
  CreateItem,
  UpdateItem,
  Sprint,
  SprintStatus,
  BoardState,
  BoardColumn,
};

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
// Mirrors `BoardEvent` in crates/flexpm-api/src/handlers/websocket.rs, which is
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
