import { describe, it, expect } from 'vitest';
import type {
    AgentStatus,
    PendingAction,
    ActivityEntry,
    CoreMemoryData,
    ConversationMessage,
    ArchivalEntry,
    AgentStats,
    KnowledgeStats,
    KnowledgeNode,
    KnowledgeNeighbor,
    ToolInfo,
    CredentialStatus,
    SkillStatus,
    SkillDetail,
    ActionResponse,
    ChatResponse,
    ToolEventType,
    ToolEvent,
    ThinkingEvent,
    ToolStartEvent,
    ToolResultEvent,
    ApprovalNeededEvent,
    TurnCompleteEvent,
    ErrorEvent,
} from '../types';

describe('types', () => {
    describe('interfaces exist and are structurally correct', () => {
        it('AgentStatus', () => {
            const s: AgentStatus = { paused: false, tools_count: 10 };
            expect(s.paused).toBe(false);
            expect(s.tools_count).toBe(10);
        });

        it('PendingAction', () => {
            const a: PendingAction = {
                id: '1',
                proposed_at: '2025-01-01T00:00:00Z',
                reasoning: 'test',
                action: { tool: 'exec', params: {} },
            };
            expect(a.id).toBe('1');
            expect(a.action?.tool).toBe('exec');
        });

        it('ActivityEntry', () => {
            const e: ActivityEntry = {
                action_type: 'exec',
                summary: 'ran command',
                status: 'ok',
                created_at: '2025-01-01',
            };
            expect(e.action_type).toBe('exec');
            expect(e.status).toBe('ok');
        });

        it('CoreMemoryData', () => {
            const c: CoreMemoryData = { personality: 'helpful' };
            expect(c.personality).toBe('helpful');
        });

        it('ConversationMessage', () => {
            const m: ConversationMessage = {
                role: 'user',
                content: 'hello',
                created_at: '2025-01-01',
            };
            expect(m.role).toBe('user');
        });

        it('ArchivalEntry', () => {
            const a: ArchivalEntry = {
                category: 'note',
                content: 'text',
                created_at: '2025-01-01',
            };
            expect(a.category).toBe('note');
        });

        it('AgentStats', () => {
            const s: AgentStats = {
                total_ticks: 5,
                total_actions: 10,
                total_approved: 8,
                total_rejected: 2,
                started_at: '2025-01-01',
            };
            expect(s.total_ticks).toBe(5);
        });

        it('KnowledgeStats', () => {
            const s: KnowledgeStats = { nodes: 10, edges: 15 };
            expect(s.nodes).toBe(10);
        });

        it('KnowledgeNode', () => {
            const n: KnowledgeNode = {
                id: 1,
                label: 'foo',
                content: 'bar',
                updated_at: '2025-01-01',
            };
            expect(n.id).toBe(1);
        });

        it('KnowledgeNeighbor', () => {
            const n: KnowledgeNeighbor = {
                edge: { relation: 'knows' },
                node: { label: 'x', node_type: 'concept' },
            };
            expect(n.edge.relation).toBe('knows');
        });

        it('ToolInfo', () => {
            const t: ToolInfo = { name: 'exec', description: 'run command' };
            expect(t.name).toBe('exec');
        });

        it('CredentialStatus', () => {
            const c: CredentialStatus = {
                name: 'API_KEY',
                description: 'key',
                configured: true,
                required: false,
            };
            expect(c.name).toBe('API_KEY');
        });

        it('SkillStatus', () => {
            const s: SkillStatus = {
                name: 'my-skill',
                skill_type: 'daemon',
                enabled: true,
                running: true,
                credentials: [],
            };
            expect(s.skill_type).toBe('daemon');
        });

        it('SkillDetail', () => {
            const d: SkillDetail = {
                name: 'x',
                skill_type: 'daemon',
                enabled: true,
                running: false,
                credentials: [],
                manifest_raw: '',
                env: {},
                log_tail: '',
                dir: '/path',
                entrypoint: 'main.py',
            };
            expect(d.dir).toBe('/path');
        });

        it('ActionResponse', () => {
            const r: ActionResponse = { ok: true, message: 'done' };
            expect(r.ok).toBe(true);
        });

        it('ChatResponse', () => {
            const r: ChatResponse = { reply: 'hi', timestamp: '2025-01-01' };
            expect(r.reply).toBe('hi');
        });
    });

    describe('ToolEventType union', () => {
        const expectedTypes: ToolEventType[] = [
            'thinking',
            'tool_start',
            'tool_result',
            'approval_needed',
            'turn_complete',
            'error',
        ];

        it('covers all expected types', () => {
            expect(expectedTypes).toContain('thinking');
            expect(expectedTypes).toContain('tool_start');
            expect(expectedTypes).toContain('tool_result');
            expect(expectedTypes).toContain('approval_needed');
            expect(expectedTypes).toContain('turn_complete');
            expect(expectedTypes).toContain('error');
            expect(expectedTypes).toHaveLength(6);
        });

        it('ThinkingEvent', () => {
            const e: ThinkingEvent = {
                type: 'thinking',
                timestamp: '2025-01-01',
                turn: 0,
                max_turns: 5,
            };
            expect(e.type).toBe('thinking');
        });

        it('ToolStartEvent', () => {
            const e: ToolStartEvent = {
                type: 'tool_start',
                timestamp: '2025-01-01',
                tool: 'exec',
                reasoning: 'run',
                auto_approved: true,
            };
            expect(e.type).toBe('tool_start');
        });

        it('ToolResultEvent', () => {
            const e: ToolResultEvent = {
                type: 'tool_result',
                timestamp: '2025-01-01',
                tool: 'exec',
                success: true,
                output_preview: 'ok',
            };
            expect(e.type).toBe('tool_result');
        });

        it('ApprovalNeededEvent', () => {
            const e: ApprovalNeededEvent = {
                type: 'approval_needed',
                timestamp: '2025-01-01',
                tool: 'exec',
                id: '1',
                reasoning: 'needs approval',
                turn: 1,
            };
            expect(e.type).toBe('approval_needed');
        });

        it('TurnCompleteEvent', () => {
            const e: TurnCompleteEvent = {
                type: 'turn_complete',
                timestamp: '2025-01-01',
                turns_used: 1,
                has_reply: true,
                tool_calls_total: 0,
            };
            expect(e.type).toBe('turn_complete');
        });

        it('ErrorEvent', () => {
            const e: ErrorEvent = {
                type: 'error',
                timestamp: '2025-01-01',
                message: 'failed',
            };
            expect(e.type).toBe('error');
        });

        it('ToolEvent accepts all event variants', () => {
            const events: ToolEvent[] = [
                { type: 'thinking', timestamp: 'x', turn: 0, max_turns: 5 },
                { type: 'tool_start', timestamp: 'x', tool: 'e', reasoning: 'r', auto_approved: true },
                { type: 'tool_result', timestamp: 'x', tool: 'e', success: true, output_preview: '' },
                { type: 'approval_needed', timestamp: 'x', tool: 'e', id: '1', reasoning: 'r', turn: 1 },
                { type: 'turn_complete', timestamp: 'x', turns_used: 1, has_reply: true, tool_calls_total: 0 },
                { type: 'error', timestamp: 'x', message: 'err' },
            ];
            expect(events).toHaveLength(6);
        });
    });
});
