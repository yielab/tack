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

export interface BoardState {
  columns: BoardColumn[];
}

export interface BoardColumn {
  status: string;
  items: Item[];
  wip_limit?: number;
  wip_exceeded: boolean;
}

export interface CreateProject {
  name: string;
  description?: string;
  template: string;
}

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
  sprint_id?: string;
  due_date?: string;
}
