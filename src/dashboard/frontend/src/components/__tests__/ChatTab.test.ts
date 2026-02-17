import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import ChatTab from '../ChatTab.svelte';

vi.mock('../../lib/api', () => ({
    api: vi.fn(() => Promise.resolve([])),
}));

vi.mock('../../lib/state.svelte', () => ({
    dashboard: { refreshCounter: 0, currentTab: 'chat', currentMemoryTab: 'core' },
    liveFeed: { isThinking: false, activeTool: null, events: [] },
}));

describe('ChatTab', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders chat input area', () => {
        render(ChatTab);
        expect(screen.getByPlaceholderText(/message/i)).toBeTruthy();
    });

    it('renders send button', () => {
        render(ChatTab);
        expect(screen.getByText('Send')).toBeTruthy();
    });
});
