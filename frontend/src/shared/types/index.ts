// Single source of frontend DTOs mirroring the backend (T-501 seed; T-502
// completes parity). Re-exports the legacy `types/api.ts` shapes and adds the
// DTOs that previously lived inline in page components.

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
} from '../../types/api';

// ─── Boards (multi-board) ──────────────────────────────────────────────────

export interface Board {
  id: string;
  project_id: string;
  name: string;
  description: string | null;
  filters: unknown;
  grouping: string | null;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateBoard {
  name: string;
  description?: string | null;
  grouping?: string | null;
  is_default?: boolean;
}

export interface UpdateBoard {
  name?: string;
  description?: string | null;
  grouping?: string | null;
  is_default?: boolean;
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
  field_type: string;
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
  field_type: string;
  description?: string | null;
  required?: boolean;
  options?: string[];
}

export interface UpdateCustomField {
  name?: string;
  field_type?: string;
  description?: string | null;
  required?: boolean;
  options?: string[];
}

// ─── Project templates ─────────────────────────────────────────────────────

export interface ProjectTemplate {
  id: string;
  name: string;
  description: string | null;
  project_type: string;
  is_builtin: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateTemplate {
  name: string;
  description?: string | null;
  project_type: string;
  vocabulary?: unknown;
  workflow?: unknown;
  custom_fields?: unknown;
  default_boards?: unknown;
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
  icon?: string;
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

// ─── Board view (GET /boards/{id}/view) ────────────────────────────────────

/** A column as returned by the backend board-view endpoint. Note the field is
 * `name`; the frontend `BoardColumn` calls the same concept `status`. */
export interface BoardViewColumn {
  name: string;
  items: import('../../types/api').Item[];
  wip_limit?: number;
  wip_exceeded: boolean;
}

export interface BoardViewResponse {
  board: Board;
  columns: BoardViewColumn[];
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
