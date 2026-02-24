import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import LoginOverlay from '../LoginOverlay.svelte';

vi.mock('../../lib/state.svelte', () => ({
    auth: { checked: true, authenticated: false },
}));

describe('LoginOverlay', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.stubGlobal('fetch', vi.fn(() =>
            Promise.resolve(new Response(
                JSON.stringify({ password_enabled: true, sso_providers: [] }),
                { status: 200, headers: { 'Content-Type': 'application/json' } },
            )),
        ));
    });

    it('renders the SafeClaw title', () => {
        render(LoginOverlay);
        expect(screen.getByText('SafeClaw')).toBeTruthy();
    });

    it('renders a password input after info loads', async () => {
        render(LoginOverlay);
        await vi.waitFor(() => {
            expect(screen.getByPlaceholderText('Enter password')).toBeTruthy();
        });
    });

    it('renders Sign In button', async () => {
        render(LoginOverlay);
        await vi.waitFor(() => {
            expect(screen.getByText('Sign In')).toBeTruthy();
        });
    });
});
