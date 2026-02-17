import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import ActivityLog from '../ActivityLog.svelte';
import * as api from '../../lib/api';

describe('ActivityLog', () => {
    beforeEach(() => {
        vi.spyOn(api, 'api').mockResolvedValue([]);
    });

    it('renders "No activity yet" when empty', async () => {
        render(ActivityLog);
        await vi.waitFor(() => {
            expect(screen.getByText('No activity yet')).toBeInTheDocument();
        });
    });

    it('renders entries when loaded', async () => {
        vi.spyOn(api, 'api').mockResolvedValue([
            {
                action_type: 'exec',
                summary: 'Ran ls',
                status: 'ok',
                created_at: '2025-01-01T12:00:00Z',
            },
        ]);
        render(ActivityLog);
        await vi.waitFor(() => {
            expect(screen.getByText(/exec/)).toBeInTheDocument();
            expect(screen.getByText(/Ran ls/)).toBeInTheDocument();
        });
    });
});
