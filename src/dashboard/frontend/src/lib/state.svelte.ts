import type { ToolEvent } from './types';

const MAX_FEED_EVENTS = 100;

export const dashboard = $state({
    refreshCounter: 0,
    currentTab: 'overview' as 'overview' | 'chat' | 'goals' | 'skills' | 'knowledge' | 'tools' | 'trash' | 'security' | 'operations' | 'settings',
    currentMemoryTab: 'core' as 'core' | 'conversation' | 'archival',
});

export const auth = $state({
    checked: false,
    authenticated: false,
    userId: null as string | null,
    role: null as string | null,
    subject: null as string | null,
    onboardingCompleted: null as boolean | null,
});

/** Live feed of streaming tool progress events. */
export const liveFeed = $state({
    events: [] as ToolEvent[],
    /** True while the LLM is thinking (between `thinking` and `turn_complete`). */
    isThinking: false,
    /** Name of the currently-executing tool, if any. */
    activeTool: null as string | null,
});

export function refreshAll(): void {
    dashboard.refreshCounter++;
}

/** Push a parsed SSE tool event into the live feed. */
export function pushToolEvent(event: ToolEvent): void {
    liveFeed.events = [event, ...liveFeed.events].slice(0, MAX_FEED_EVENTS);

    switch (event.type) {
        case 'thinking':
            liveFeed.isThinking = true;
            liveFeed.activeTool = null;
            break;
        case 'tool_start':
            liveFeed.activeTool = event.tool;
            break;
        case 'tool_result':
            liveFeed.activeTool = null;
            break;
        case 'turn_complete':
        case 'error':
            liveFeed.isThinking = false;
            liveFeed.activeTool = null;
            break;
    }
}

/** Clear the live feed (e.g. on disconnect). */
export function clearLiveFeed(): void {
    liveFeed.events = [];
    liveFeed.isThinking = false;
    liveFeed.activeTool = null;
}
