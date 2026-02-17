import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import Header from '../Header.svelte';
import * as api from '../../lib/api';

describe('Header', () => {
    beforeEach(() => {
        vi.spyOn(api, 'api')
            .mockImplementation((method, path) => {
                if (path === '/api/status') {
                    return Promise.resolve({ paused: false, tools_count: 10 });
                }
                if (path === '/api/agent/pause' || path === '/api/agent/resume' || path === '/api/agent/tick') {
                    return Promise.resolve({ ok: true });
                }
                return Promise.reject(new Error('Unknown path'));
            });
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response()));
    });

    it('renders the agent name', async () => {
        render(Header);
        await vi.waitFor(() => {
            expect(screen.getByText(/safe-agent/)).toBeInTheDocument();
        });
    });

    it('renders pause button when running', async () => {
        render(Header);
        await vi.waitFor(() => {
            expect(screen.getByRole('button', { name: /Pause/i })).toBeInTheDocument();
        });
    });

    it('renders resume button when paused', async () => {
        vi.spyOn(api, 'api').mockImplementation((method, path) => {
            if (path === '/api/status') {
                return Promise.resolve({ paused: true, tools_count: 10 });
            }
            if (path === '/api/agent/resume') {
                return Promise.resolve({ ok: true });
            }
            return Promise.reject(new Error('Unknown path'));
        });
        render(Header);
        await vi.waitFor(() => {
            expect(screen.getByRole('button', { name: /Resume/i })).toBeInTheDocument();
        });
    });
});
