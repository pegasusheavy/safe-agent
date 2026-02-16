export interface AgentStatus {
    paused: boolean;
    tools_count: number;
}

export interface PendingAction {
    id: string;
    proposed_at: string;
    reasoning?: string;
    action?: {
        tool?: string;
        params?: Record<string, unknown>;
        reasoning?: string;
    };
}

export interface ActivityEntry {
    action_type: string;
    summary: string;
    detail?: string;
    status: 'ok' | 'error' | string;
    created_at: string;
}

export interface CoreMemoryData {
    personality: string;
}

export interface ConversationMessage {
    role: string;
    content: string;
    created_at: string;
}

export interface ArchivalEntry {
    category: string;
    content: string;
    created_at: string;
}

export interface AgentStats {
    total_ticks: number;
    total_actions: number;
    total_approved: number;
    total_rejected: number;
    last_tick_at?: string;
    started_at: string;
}

export interface KnowledgeStats {
    nodes: number;
    edges: number;
}

export interface KnowledgeNode {
    id: number;
    node_type?: string;
    label: string;
    content: string;
    confidence?: number;
    updated_at: string;
}

export interface KnowledgeNeighbor {
    edge: { relation: string };
    node: { label: string; node_type: string };
}

export interface ToolInfo {
    name: string;
    description: string;
}

export interface CredentialStatus {
    name: string;
    label?: string;
    description: string;
    configured: boolean;
    required: boolean;
}

export interface SkillStatus {
    name: string;
    description?: string;
    skill_type: 'daemon' | 'oneshot';
    running: boolean;
    pid?: number;
    credentials: CredentialStatus[];
}

export interface ActionResponse {
    ok: boolean;
    message?: string;
    count?: number;
}

export interface ChatResponse {
    reply: string;
    timestamp: string;
}

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'DELETE';
