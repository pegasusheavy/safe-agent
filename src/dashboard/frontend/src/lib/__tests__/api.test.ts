import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { api, UnauthorizedError } from '../api';
import { auth } from '../state.svelte';

describe('api', () => {
    beforeEach(() => {
        vi.stubGlobal(
            'fetch',
            vi.fn((url: string, opts?: RequestInit) => {
                return Promise.resolve(new Response());
            }),
        );
        auth.authenticated = true;
    });

    afterEach(() => {
        vi.unstubAllGlobals();
    });

    describe('api()', () => {
        it('makes GET request with correct URL', async () => {
            (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue(
                new Response(JSON.stringify({ ok: true }), {
                    status: 200,
                    headers: { 'Content-Type': 'application/json' },
                }),
            );
            await api('GET', '/api/status');
            expect(fetch).toHaveBeenCalledWith('/api/status', expect.objectContaining({ method: 'GET' }));
        });

        it('makes POST request with body and Content-Type', async () => {
            (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue(
                new Response(JSON.stringify({ ok: true }), {
                    status: 200,
                    headers: { 'Content-Type': 'application/json' },
                }),
            );
            await api('POST', '/api/agent/pause', { reason: 'test' });
            expect(fetch).toHaveBeenCalledWith(
                '/api/agent/pause',
                expect.objectContaining({
                    method: 'POST',
                    body: JSON.stringify({ reason: 'test' }),
                    headers: expect.objectContaining({
                        'Content-Type': 'application/json',
                    }),
                }),
            );
        });

        it('throws UnauthorizedError on 401', async () => {
            (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue(
                new Response(JSON.stringify({}), { status: 401 }),
            );
            await expect(api('GET', '/api/status')).rejects.toThrow(UnauthorizedError);
            expect(auth.authenticated).toBe(false);
        });

        it('throws on non-ok responses', async () => {
            (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue(
                new Response('Not Found', { status: 404 }),
            );
            await expect(api('GET', '/api/missing')).rejects.toThrow('GET /api/missing: 404');
        });

        it('returns parsed JSON on success', async () => {
            const data = { paused: false, tools_count: 5 };
            (globalThis.fetch as ReturnType<typeof vi.fn>).mockResolvedValue(
                new Response(JSON.stringify(data), {
                    status: 200,
                    headers: { 'Content-Type': 'application/json' },
                }),
            );
            const result = await api<{ paused: boolean; tools_count: number }>('GET', '/api/status');
            expect(result).toEqual(data);
        });
    });

    describe('UnauthorizedError', () => {
        it('has correct name and message', () => {
            const err = new UnauthorizedError();
            expect(err.name).toBe('UnauthorizedError');
            expect(err.message).toBe('Unauthorized');
        });
    });
});
