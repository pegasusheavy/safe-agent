import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import StatsPanel from '../StatsPanel.svelte';
import * as api from '../../lib/api';

describe('StatsPanel', () => {
    beforeEach(() => {
        vi.spyOn(api, 'api').mockResolvedValue({
            total_ticks: 42,
            total_actions: 100,
            total_approved: 95,
            total_rejected: 5,
            started_at: '2025-01-01T00:00:00Z',
        });
    });

    it('renders stats when loaded', async () => {
        render(StatsPanel);
        await vi.waitFor(() => {
            expect(screen.getByText('42')).toBeInTheDocument();
            expect(screen.getByText('100')).toBeInTheDocument();
            expect(screen.getByText('95')).toBeInTheDocument();
            expect(screen.getByText('5')).toBeInTheDocument();
        });
    });

    it('renders Loading when stats not yet loaded', () => {
        vi.spyOn(api, 'api').mockImplementation(() => new Promise(() => {}));
        render(StatsPanel);
        expect(screen.getByText('Loading...')).toBeInTheDocument();
    });
});
