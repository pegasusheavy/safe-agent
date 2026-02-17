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
    enabled: boolean;
    running: boolean;
    pid?: number;
    credentials: CredentialStatus[];
}

export interface SkillDetail extends SkillStatus {
    manifest_raw: string;
    env: Record<string, string>;
    log_tail: string;
    dir: string;
    entrypoint: string;
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

// ---------------------------------------------------------------------------
// Streaming tool progress SSE events
// ---------------------------------------------------------------------------

export type ToolEventType =
    | 'thinking'
    | 'tool_start'
    | 'tool_result'
    | 'approval_needed'
    | 'turn_complete'
    | 'error';

export interface BaseToolEvent {
    type: ToolEventType;
    timestamp: string;
}

export interface ThinkingEvent extends BaseToolEvent {
    type: 'thinking';
    turn: number;
    max_turns: number;
    context?: string;
}

export interface ToolStartEvent extends BaseToolEvent {
    type: 'tool_start';
    tool: string;
    reasoning: string;
    auto_approved: boolean;
    approved?: boolean;
    approval_id?: string;
    turn?: number;
}

export interface ToolResultEvent extends BaseToolEvent {
    type: 'tool_result';
    tool: string;
    success: boolean;
    output_preview: string;
    approved?: boolean;
    approval_id?: string;
    turn?: number;
}

export interface ApprovalNeededEvent extends BaseToolEvent {
    type: 'approval_needed';
    tool: string;
    id: string;
    reasoning: string;
    turn: number;
}

export interface TurnCompleteEvent extends BaseToolEvent {
    type: 'turn_complete';
    turns_used: number;
    has_reply: boolean;
    tool_calls_total: number;
    pending_approvals?: number;
    exhausted?: boolean;
    context?: string;
    turn?: number;
}

export interface ErrorEvent extends BaseToolEvent {
    type: 'error';
    message: string;
    context?: string;
}

export type ToolEvent =
    | ThinkingEvent
    | ToolStartEvent
    | ToolResultEvent
    | ApprovalNeededEvent
    | TurnCompleteEvent
    | ErrorEvent;
