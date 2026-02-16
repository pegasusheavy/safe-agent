export const dashboard = $state({
    refreshCounter: 0,
    currentTab: 'overview' as 'overview' | 'chat' | 'skills' | 'knowledge' | 'tools',
    currentMemoryTab: 'core' as 'core' | 'conversation' | 'archival',
});

export const auth = $state({
    checked: false,
    authenticated: false,
});

export function refreshAll(): void {
    dashboard.refreshCounter++;
}
