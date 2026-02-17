import { describe, it, expect, beforeEach } from 'vitest';
import {
    dashboard,
    auth,
    liveFeed,
    refreshAll,
    pushToolEvent,
    clearLiveFeed,
} from '../state.svelte';
import type { ToolEvent } from '../types';

describe('state', () => {
    beforeEach(() => {
        clearLiveFeed();
        dashboard.refreshCounter = 0;
        auth.checked = false;
        auth.authenticated = false;
    });

    describe('dashboard initial values', () => {
        it('has correct initial values', () => {
            dashboard.refreshCounter = 0;
            expect(dashboard.refreshCounter).toBe(0);
            expect(dashboard.currentTab).toBe('overview');
            expect(dashboard.currentMemoryTab).toBe('core');
        });
    });

    describe('auth initial values', () => {
        it('has correct initial values', () => {
            auth.checked = false;
            auth.authenticated = false;
            expect(auth.checked).toBe(false);
            expect(auth.authenticated).toBe(false);
        });
    });

    describe('refreshAll', () => {
        it('increments refreshCounter', () => {
            dashboard.refreshCounter = 0;
            refreshAll();
            expect(dashboard.refreshCounter).toBe(1);
            refreshAll();
            expect(dashboard.refreshCounter).toBe(2);
        });
    });

    describe('pushToolEvent', () => {
        it('adds events to liveFeed', () => {
            const evt: ToolEvent = {
                type: 'thinking',
                timestamp: new Date().toISOString(),
                turn: 0,
                max_turns: 5,
            };
            pushToolEvent(evt);
            expect(liveFeed.events).toHaveLength(1);
            expect(liveFeed.events[0]).toEqual(evt);
        });

        it('updates isThinking and activeTool for thinking event', () => {
            pushToolEvent({
                type: 'thinking',
                timestamp: new Date().toISOString(),
                turn: 0,
                max_turns: 5,
            });
            expect(liveFeed.isThinking).toBe(true);
            expect(liveFeed.activeTool).toBeNull();
        });

        it('updates activeTool for tool_start event', () => {
            pushToolEvent({
                type: 'tool_start',
                timestamp: new Date().toISOString(),
                tool: 'exec',
                reasoning: 'run command',
                auto_approved: true,
            });
            expect(liveFeed.activeTool).toBe('exec');
        });

        it('clears activeTool for tool_result event', () => {
            pushToolEvent({
                type: 'tool_start',
                timestamp: new Date().toISOString(),
                tool: 'exec',
                reasoning: 'run',
                auto_approved: true,
            });
            expect(liveFeed.activeTool).toBe('exec');
            pushToolEvent({
                type: 'tool_result',
                timestamp: new Date().toISOString(),
                tool: 'exec',
                success: true,
                output_preview: 'ok',
            });
            expect(liveFeed.activeTool).toBeNull();
        });

        it('clears isThinking for turn_complete event', () => {
            pushToolEvent({
                type: 'thinking',
                timestamp: new Date().toISOString(),
                turn: 0,
                max_turns: 5,
            });
            expect(liveFeed.isThinking).toBe(true);
            pushToolEvent({
                type: 'turn_complete',
                timestamp: new Date().toISOString(),
                turns_used: 1,
                has_reply: true,
                tool_calls_total: 0,
            });
            expect(liveFeed.isThinking).toBe(false);
            expect(liveFeed.activeTool).toBeNull();
        });

        it('clears isThinking for error event', () => {
            pushToolEvent({
                type: 'thinking',
                timestamp: new Date().toISOString(),
                turn: 0,
                max_turns: 5,
            });
            pushToolEvent({
                type: 'error',
                timestamp: new Date().toISOString(),
                message: 'Something went wrong',
            });
            expect(liveFeed.isThinking).toBe(false);
            expect(liveFeed.activeTool).toBeNull();
        });

        it('approval_needed adds event without changing isThinking/activeTool', () => {
            pushToolEvent({
                type: 'approval_needed',
                timestamp: new Date().toISOString(),
                tool: 'exec',
                id: '123',
                reasoning: 'needs approval',
                turn: 1,
            });
            expect(liveFeed.events).toHaveLength(1);
            expect(liveFeed.events[0].type).toBe('approval_needed');
        });
    });

    describe('clearLiveFeed', () => {
        it('resets events, isThinking, and activeTool', () => {
            pushToolEvent({
                type: 'tool_start',
                timestamp: new Date().toISOString(),
                tool: 'exec',
                reasoning: 'run',
                auto_approved: true,
            });
            expect(liveFeed.events.length).toBeGreaterThan(0);
            expect(liveFeed.activeTool).toBe('exec');
            clearLiveFeed();
            expect(liveFeed.events).toHaveLength(0);
            expect(liveFeed.isThinking).toBe(false);
            expect(liveFeed.activeTool).toBeNull();
        });
    });

    describe('event buffer limit', () => {
        it('limits events to MAX_FEED_EVENTS (100)', () => {
            for (let i = 0; i < 150; i++) {
                pushToolEvent({
                    type: 'thinking',
                    timestamp: new Date().toISOString(),
                    turn: 0,
                    max_turns: 5,
                });
            }
            expect(liveFeed.events).toHaveLength(100);
        });
    });
});
