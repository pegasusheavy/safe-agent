<script lang="ts">
    import { onMount } from 'svelte';
    import { auth } from '../lib/state.svelte';

    interface User {
        id: string;
        username: string;
        display_name: string;
        role: string;
        email: string;
        telegram_id: number | null;
        whatsapp_id: string | null;
        enabled: boolean;
        last_seen_at: string | null;
        created_at: string;
    }

    let users: User[] = $state([]);
    let loading = $state(true);
    let showCreateForm = $state(false);
    let editingUser: User | null = $state(null);
    let message = $state('');

    // Create form
    let newUsername = $state('');
    let newDisplayName = $state('');
    let newRole = $state('user');
    let newPassword = $state('');
    let newEmail = $state('');
    let newTelegramId = $state('');
    let newWhatsappId = $state('');

    async function fetchUsers() {
        loading = true;
        try {
            const res = await fetch('/api/users');
            const data = await res.json();
            users = data.users || [];
        } catch {
            users = [];
        }
        loading = false;
    }

    async function createUser() {
        message = '';
        const body: any = {
            username: newUsername,
            display_name: newDisplayName || newUsername,
            role: newRole,
            password: newPassword,
        };
        if (newEmail) body.email = newEmail;
        if (newTelegramId) body.telegram_id = parseInt(newTelegramId);
        if (newWhatsappId) body.whatsapp_id = newWhatsappId;

        try {
            const res = await fetch('/api/users', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body),
            });
            if (res.ok) {
                message = `User "${newUsername}" created`;
                newUsername = ''; newDisplayName = ''; newRole = 'user';
                newPassword = ''; newEmail = ''; newTelegramId = ''; newWhatsappId = '';
                showCreateForm = false;
                fetchUsers();
            } else {
                message = 'Failed to create user (username may already exist)';
            }
        } catch {
            message = 'Error creating user';
        }
    }

    async function toggleEnabled(user: User) {
        try {
            await fetch(`/api/users/${user.id}`, {
                method: 'PUT',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ enabled: !user.enabled }),
            });
            fetchUsers();
        } catch {
            message = 'Failed to update user';
        }
    }

    async function changeRole(user: User, role: string) {
        try {
            await fetch(`/api/users/${user.id}`, {
                method: 'PUT',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ role }),
            });
            fetchUsers();
        } catch {
            message = 'Failed to update role';
        }
    }

    async function deleteUser(user: User) {
        if (!confirm(`Delete user "${user.username}"? This cannot be undone.`)) return;
        try {
            await fetch(`/api/users/${user.id}`, { method: 'DELETE' });
            fetchUsers();
        } catch {
            message = 'Failed to delete user';
        }
    }

    function roleColor(role: string): string {
        switch (role) {
            case 'admin': return 'bg-red-500/20 text-red-400';
            case 'viewer': return 'bg-blue-500/20 text-blue-400';
            default: return 'bg-green-500/20 text-green-400';
        }
    }

    function timeAgo(date: string | null): string {
        if (!date) return 'Never';
        const d = new Date(date + 'Z');
        const diff = Date.now() - d.getTime();
        if (diff < 60000) return 'Just now';
        if (diff < 3600000) return `${Math.floor(diff/60000)}m ago`;
        if (diff < 86400000) return `${Math.floor(diff/3600000)}h ago`;
        return `${Math.floor(diff/86400000)}d ago`;
    }

    onMount(fetchUsers);

    const isAdmin = $derived(auth.role === 'admin' || !auth.userId);
</script>

<div class="card">
    <div class="flex items-center justify-between mb-4">
        <h3 class="text-lg font-semibold">
            <i class="fa-solid fa-users mr-1"></i> Users
        </h3>
        {#if isAdmin}
            <button class="btn-primary text-sm" onclick={() => showCreateForm = !showCreateForm}>
                <i class="fa-solid fa-plus mr-1"></i> New User
            </button>
        {/if}
    </div>

    {#if message}
        <div class="bg-surface rounded p-2 text-sm text-muted mb-3">{message}</div>
    {/if}

    <!-- Create User Form -->
    {#if showCreateForm && isAdmin}
        <div class="bg-surface rounded p-4 mb-4 space-y-3">
            <h4 class="text-sm font-medium">Create User</h4>
            <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div>
                    <label class="text-xs text-muted block mb-1">Username *</label>
                    <input type="text" bind:value={newUsername} placeholder="jdoe" class="w-full bg-bg border border-border rounded px-3 py-1.5 text-sm" />
                </div>
                <div>
                    <label class="text-xs text-muted block mb-1">Display Name</label>
                    <input type="text" bind:value={newDisplayName} placeholder="Jane Doe" class="w-full bg-bg border border-border rounded px-3 py-1.5 text-sm" />
                </div>
                <div>
                    <label class="text-xs text-muted block mb-1">Role</label>
                    <select bind:value={newRole} class="w-full bg-bg border border-border rounded px-3 py-1.5 text-sm">
                        <option value="admin">Admin</option>
                        <option value="user">User</option>
                        <option value="viewer">Viewer</option>
                    </select>
                </div>
                <div>
                    <label class="text-xs text-muted block mb-1">Password</label>
                    <input type="password" bind:value={newPassword} class="w-full bg-bg border border-border rounded px-3 py-1.5 text-sm" />
                </div>
                <div>
                    <label class="text-xs text-muted block mb-1">Email</label>
                    <input type="email" bind:value={newEmail} placeholder="jane@example.com" class="w-full bg-bg border border-border rounded px-3 py-1.5 text-sm" />
                </div>
                <div>
                    <label class="text-xs text-muted block mb-1">Telegram User ID</label>
                    <input type="text" bind:value={newTelegramId} placeholder="123456789" class="w-full bg-bg border border-border rounded px-3 py-1.5 text-sm" />
                </div>
                <div>
                    <label class="text-xs text-muted block mb-1">WhatsApp Number</label>
                    <input type="text" bind:value={newWhatsappId} placeholder="+15551234567" class="w-full bg-bg border border-border rounded px-3 py-1.5 text-sm" />
                </div>
            </div>
            <div class="flex gap-2">
                <button class="btn-primary text-sm" onclick={createUser} disabled={!newUsername.trim()}>
                    <i class="fa-solid fa-check mr-1"></i> Create
                </button>
                <button class="btn-secondary text-sm" onclick={() => showCreateForm = false}>Cancel</button>
            </div>
        </div>
    {/if}

    <!-- Users List -->
    {#if loading}
        <p class="text-muted text-sm">Loading users...</p>
    {:else if users.length === 0}
        <p class="text-muted text-sm">No users configured. The agent operates in single-user mode.</p>
    {:else}
        <div class="space-y-2">
            {#each users as user}
                <div class="bg-surface rounded p-3 flex items-center justify-between gap-3 flex-wrap">
                    <div class="flex items-center gap-3 min-w-0">
                        <div class="w-8 h-8 rounded-full bg-accent/20 flex items-center justify-center text-accent font-bold text-sm flex-shrink-0">
                            {user.display_name?.charAt(0)?.toUpperCase() || user.username.charAt(0).toUpperCase()}
                        </div>
                        <div class="min-w-0">
                            <div class="flex items-center gap-2">
                                <span class="font-medium text-fg truncate">{user.display_name || user.username}</span>
                                <span class="text-xs text-muted font-mono">@{user.username}</span>
                                <span class="px-1.5 py-0.5 rounded text-xs font-semibold {roleColor(user.role)}">{user.role}</span>
                                {#if !user.enabled}
                                    <span class="px-1.5 py-0.5 rounded text-xs bg-red-500/20 text-red-400">disabled</span>
                                {/if}
                            </div>
                            <div class="flex gap-3 text-xs text-muted mt-0.5">
                                {#if user.email}
                                    <span><i class="fa-solid fa-envelope mr-0.5"></i> {user.email}</span>
                                {/if}
                                {#if user.telegram_id}
                                    <span><i class="fa-brands fa-telegram mr-0.5"></i> {user.telegram_id}</span>
                                {/if}
                                {#if user.whatsapp_id}
                                    <span><i class="fa-brands fa-whatsapp mr-0.5"></i> {user.whatsapp_id}</span>
                                {/if}
                                <span>Last seen: {timeAgo(user.last_seen_at)}</span>
                            </div>
                        </div>
                    </div>

                    {#if isAdmin}
                        <div class="flex items-center gap-2 flex-shrink-0">
                            <select
                                value={user.role}
                                onchange={(e) => changeRole(user, (e.target as HTMLSelectElement).value)}
                                class="bg-bg border border-border rounded px-2 py-1 text-xs"
                            >
                                <option value="admin">Admin</option>
                                <option value="user">User</option>
                                <option value="viewer">Viewer</option>
                            </select>
                            <button
                                class="text-xs px-2 py-1 rounded {user.enabled ? 'bg-yellow-500/20 text-yellow-400 hover:bg-yellow-500/30' : 'bg-green-500/20 text-green-400 hover:bg-green-500/30'}"
                                onclick={() => toggleEnabled(user)}
                                title={user.enabled ? 'Disable user' : 'Enable user'}
                            >
                                {user.enabled ? 'Disable' : 'Enable'}
                            </button>
                            <button
                                class="text-xs px-2 py-1 rounded bg-red-500/20 text-red-400 hover:bg-red-500/30"
                                onclick={() => deleteUser(user)}
                                title="Delete user"
                            >
                                <i class="fa-solid fa-trash"></i>
                            </button>
                        </div>
                    {/if}
                </div>
            {/each}
        </div>
    {/if}
</div>
