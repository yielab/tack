import type {
  Project,
  CreateProject,
  Item,
  CreateItem,
  UpdateItem,
  BoardState,
  Sprint,
} from '../types/api';

const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:3210/api';

class ApiClient {
  private baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  private async request<T>(
    endpoint: string,
    options?: RequestInit
  ): Promise<T> {
    const response = await fetch(`${this.baseUrl}${endpoint}`, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options?.headers,
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(error || `HTTP ${response.status}: ${response.statusText}`);
    }

    return response.json();
  }

  // Projects
  async listProjects(): Promise<Project[]> {
    return this.request<Project[]>('/projects');
  }

  async getProject(id: string): Promise<Project> {
    return this.request<Project>(`/projects/${id}`);
  }

  async createProject(data: CreateProject): Promise<{ id: string }> {
    return this.request(`/projects`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async deleteProject(id: string): Promise<void> {
    await this.request(`/projects/${id}`, { method: 'DELETE' });
  }

  // Items
  async listItems(projectId: string): Promise<Item[]> {
    return this.request<Item[]>(`/projects/${projectId}/items`);
  }

  async getItem(id: string): Promise<Item> {
    return this.request<Item>(`/items/${id}`);
  }

  async createItem(projectId: string, data: CreateItem): Promise<{ id: string }> {
    return this.request(`/projects/${projectId}/items`, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  }

  async updateItem(id: string, data: UpdateItem): Promise<Item> {
    return this.request<Item>(`/items/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(data),
    });
  }

  async deleteItem(id: string): Promise<void> {
    await this.request(`/items/${id}`, { method: 'DELETE' });
  }

  // Board
  async getBoard(projectId: string): Promise<BoardState> {
    return this.request<BoardState>(`/projects/${projectId}/board`);
  }

  // Sprints
  async listSprints(projectId: string): Promise<Sprint[]> {
    return this.request<Sprint[]>(`/projects/${projectId}/sprints`);
  }

  // Search
  async searchGlobal(query: string, workspaceId?: string): Promise<Item[]> {
    const params = new URLSearchParams({ q: query });
    if (workspaceId) {
      params.append('workspace_id', workspaceId);
    }
    return this.request<Item[]>(`/search?${params}`);
  }

  async searchProject(projectId: string, query: string): Promise<Item[]> {
    const params = new URLSearchParams({ q: query });
    return this.request<Item[]>(`/projects/${projectId}/search?${params}`);
  }

  // WebSocket
  createBoardSocket(projectId: string): WebSocket {
    const wsUrl = this.baseUrl.replace(/^http/, 'ws');
    return new WebSocket(`${wsUrl}/projects/${projectId}/board/live`);
  }
}

export const api = new ApiClient(API_URL);
