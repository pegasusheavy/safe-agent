// safe-agent dashboard

let currentMemoryTab = 'core';
let currentMainTab = 'overview';

// --- API helpers ---

async function api(method, path, body) {
    const opts = { method, headers: {} };
    if (body) {
        opts.headers['Content-Type'] = 'application/json';
        opts.body = JSON.stringify(body);
    }
    const res = await fetch(path, opts);
    if (!res.ok) throw new Error(`${method} ${path}: ${res.status}`);
    return res.json();
}

// --- Main tabs ---

function switchMainTab(tab) {
    currentMainTab = tab;
    document.querySelectorAll('.main-tab').forEach(t => t.classList.remove('active'));
    event.target.classList.add('active');
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    document.getElementById('tab-' + tab).classList.add('active');
    if (tab === 'knowledge') loadKnowledge();
    if (tab === 'tools') loadTools();
}

// --- Data loading ---

async function loadStatus() {
    try {
        const status = await api('GET', '/api/status');
        const badge = document.getElementById('status-badge');
        const btnPause = document.getElementById('btn-pause');
        const btnResume = document.getElementById('btn-resume');
        const toolsCount = document.getElementById('tools-count');

        toolsCount.textContent = status.tools_count + ' tools';

        if (status.paused) {
            badge.textContent = 'paused';
            badge.className = 'badge paused';
            btnPause.style.display = 'none';
            btnResume.style.display = '';
        } else {
            badge.textContent = 'running';
            badge.className = 'badge running';
            btnPause.style.display = '';
            btnResume.style.display = 'none';
        }
    } catch (e) {
        const badge = document.getElementById('status-badge');
        badge.textContent = 'disconnected';
        badge.className = 'badge';
    }
}

async function loadGoogleStatus() {
    try {
        const status = await api('GET', '/api/google/status');
        const btn = document.getElementById('btn-google');
        if (status.enabled) {
            btn.style.display = '';
            btn.textContent = status.connected ? 'Google Connected' : 'Connect Google';
            btn.disabled = status.connected;
            if (status.connected) {
                btn.style.color = 'var(--green)';
                btn.style.borderColor = 'var(--green)';
            }
        }
    } catch (e) {}
}

async function loadPending() {
    try {
        const actions = await api('GET', '/api/pending');
        const container = document.getElementById('pending-list');

        if (!actions.length) {
            container.innerHTML = '<p class="empty">No pending actions</p>';
            return;
        }

        container.innerHTML = actions.map(a => {
            const actionData = a.action || {};
            const tool = actionData.tool || 'unknown';
            const params = actionData.params ? JSON.stringify(actionData.params, null, 2) : '{}';
            return `
                <div class="action-card">
                    <div class="action-type">${esc(tool)}</div>
                    <div class="action-summary"><pre style="font-size:11px;white-space:pre-wrap;margin:4px 0">${esc(params)}</pre></div>
                    <div class="action-reasoning">${esc(a.reasoning || actionData.reasoning || '')}</div>
                    <div class="action-time">${esc(a.proposed_at)}</div>
                    <div class="action-buttons">
                        <button class="small approve" onclick="approveAction('${esc(a.id)}')">Approve</button>
                        <button class="small reject" onclick="rejectAction('${esc(a.id)}')">Reject</button>
                    </div>
                </div>
            `;
        }).join('');
    } catch (e) {
        console.error('loadPending:', e);
    }
}

async function loadActivity() {
    try {
        const entries = await api('GET', '/api/activity?limit=30');
        const container = document.getElementById('activity-list');

        if (!entries.length) {
            container.innerHTML = '<p class="empty">No activity yet</p>';
            return;
        }

        container.innerHTML = entries.map(e => `
            <div class="activity-entry">
                <div class="activity-status ${esc(e.status)}"></div>
                <div class="activity-text">
                    <strong>${esc(e.action_type)}</strong>: ${esc(e.summary)}
                    ${e.detail ? `<br><span style="color:var(--text-dim)">${esc(e.detail).slice(0, 200)}</span>` : ''}
                </div>
                <div class="activity-time">${esc(e.created_at)}</div>
            </div>
        `).join('');
    } catch (e) {
        console.error('loadActivity:', e);
    }
}

async function loadMemory() {
    const container = document.getElementById('memory-content');
    const searchBox = document.getElementById('memory-search');

    try {
        if (currentMemoryTab === 'core') {
            searchBox.style.display = 'none';
            const data = await api('GET', '/api/memory/core');
            container.innerHTML = `<div class="memory-text">${esc(data.personality || '(empty)')}</div>`;
        } else if (currentMemoryTab === 'conversation') {
            searchBox.style.display = 'none';
            const messages = await api('GET', '/api/memory/conversation');
            if (!messages.length) {
                container.innerHTML = '<p class="empty">No conversation history</p>';
                return;
            }
            container.innerHTML = messages.map(m => `
                <div class="memory-entry">
                    <strong>${esc(m.role)}</strong> <span style="color:var(--text-dim)">${esc(m.created_at)}</span><br>
                    ${esc(m.content)}
                </div>
            `).join('');
        } else if (currentMemoryTab === 'archival') {
            searchBox.style.display = '';
            const q = document.getElementById('archival-search').value;
            const url = q ? `/api/memory/archival?q=${encodeURIComponent(q)}` : '/api/memory/archival';
            const entries = await api('GET', url);
            if (!entries.length) {
                container.innerHTML = '<p class="empty">No archival entries</p>';
                return;
            }
            container.innerHTML = entries.map(e => `
                <div class="memory-entry">
                    <span class="memory-category">${esc(e.category)}</span>
                    <span style="color:var(--text-dim); font-size:11px"> ${esc(e.created_at)}</span><br>
                    ${esc(e.content)}
                </div>
            `).join('');
        }
    } catch (e) {
        container.innerHTML = `<p class="empty">Error loading memory</p>`;
        console.error('loadMemory:', e);
    }
}

async function loadStats() {
    try {
        const stats = await api('GET', '/api/stats');
        const container = document.getElementById('stats-content');
        container.innerHTML = `
            <div class="stat-grid">
                <div class="stat-item">
                    <div class="stat-value">${stats.total_ticks}</div>
                    <div class="stat-label">Ticks</div>
                </div>
                <div class="stat-item">
                    <div class="stat-value">${stats.total_actions}</div>
                    <div class="stat-label">Actions Executed</div>
                </div>
                <div class="stat-item">
                    <div class="stat-value">${stats.total_approved}</div>
                    <div class="stat-label">Approved</div>
                </div>
                <div class="stat-item">
                    <div class="stat-value">${stats.total_rejected}</div>
                    <div class="stat-label">Rejected</div>
                </div>
            </div>
            <div style="margin-top:12px; font-size:12px; color:var(--text-dim)">
                ${stats.last_tick_at ? 'Last tick: ' + esc(stats.last_tick_at) : 'No ticks yet'}<br>
                Started: ${esc(stats.started_at)}
            </div>
        `;
    } catch (e) {
        console.error('loadStats:', e);
    }
}

async function loadKnowledge() {
    try {
        const [stats, nodes] = await Promise.all([
            api('GET', '/api/knowledge/stats'),
            api('GET', '/api/knowledge/nodes?limit=100'),
        ]);
        document.getElementById('kg-stats').textContent = `${stats.nodes} nodes, ${stats.edges} edges`;
        renderKnowledgeNodes(nodes);
    } catch (e) {
        console.error('loadKnowledge:', e);
    }
}

async function searchKnowledge() {
    const q = document.getElementById('kg-search').value;
    if (!q) { loadKnowledge(); return; }
    try {
        const nodes = await api('GET', `/api/knowledge/search?q=${encodeURIComponent(q)}`);
        renderKnowledgeNodes(nodes);
    } catch (e) {
        console.error('searchKnowledge:', e);
    }
}

function renderKnowledgeNodes(nodes) {
    const container = document.getElementById('kg-nodes');
    if (!nodes.length) {
        container.innerHTML = '<p class="empty">No knowledge nodes</p>';
        return;
    }
    container.innerHTML = nodes.map(n => `
        <div class="action-card" onclick="loadNodeNeighbors(${n.id})" style="cursor:pointer">
            <div class="action-type">${esc(n.node_type || 'node')}</div>
            <div style="font-size:14px;font-weight:600;margin-bottom:4px">${esc(n.label)}</div>
            <div style="font-size:12px;color:var(--text-dim)">${esc(n.content).slice(0, 200)}</div>
            <div style="font-size:11px;color:var(--text-dim);margin-top:4px">
                confidence: ${(n.confidence || 1).toFixed(2)} &middot; ${esc(n.updated_at)}
            </div>
        </div>
    `).join('');
}

async function loadNodeNeighbors(id) {
    try {
        const neighbors = await api('GET', `/api/knowledge/nodes/${id}/neighbors`);
        const container = document.getElementById('kg-nodes');
        if (!neighbors.length) {
            alert('No neighbors for this node.');
            return;
        }
        let html = `<button class="small" onclick="loadKnowledge()" style="margin-bottom:12px">&larr; Back</button>`;
        html += neighbors.map(n => `
            <div class="action-card">
                <div class="action-type">${esc(n.edge.relation)}</div>
                <div style="font-size:14px">${esc(n.node.label)} <span style="color:var(--text-dim);font-size:11px">(${esc(n.node.node_type)})</span></div>
            </div>
        `).join('');
        container.innerHTML = html;
    } catch (e) {
        console.error('loadNodeNeighbors:', e);
    }
}

async function loadTools() {
    try {
        const tools = await api('GET', '/api/tools');
        const container = document.getElementById('tools-list');
        if (!tools.length) {
            container.innerHTML = '<p class="empty">No tools registered</p>';
            return;
        }
        container.innerHTML = tools.map(t => `
            <div class="action-card">
                <div class="action-type">${esc(t.name)}</div>
                <div style="font-size:13px;color:var(--text-dim)">${esc(t.description)}</div>
            </div>
        `).join('');
    } catch (e) {
        console.error('loadTools:', e);
    }
}

// --- Actions ---

async function approveAction(id) {
    await api('POST', `/api/pending/${id}/approve`);
    refreshAll();
}

async function rejectAction(id) {
    await api('POST', `/api/pending/${id}/reject`);
    refreshAll();
}

async function approveAll() {
    await api('POST', '/api/pending/approve-all');
    refreshAll();
}

async function rejectAll() {
    await api('POST', '/api/pending/reject-all');
    refreshAll();
}

async function pauseAgent() {
    await api('POST', '/api/agent/pause');
    refreshAll();
}

async function resumeAgent() {
    await api('POST', '/api/agent/resume');
    refreshAll();
}

async function forceTick() {
    await api('POST', '/api/agent/tick');
    refreshAll();
}

function connectGoogle() {
    window.open('/auth/google', '_blank', 'width=600,height=700');
}

function switchMemoryTab(tab) {
    currentMemoryTab = tab;
    document.querySelectorAll('#panel-memory .tab').forEach(t => t.classList.remove('active'));
    event.target.classList.add('active');
    loadMemory();
}

function searchArchival(e) {
    if (e.key === 'Enter') loadMemory();
}

// --- Helpers ---

function esc(s) {
    if (s == null) return '';
    const d = document.createElement('div');
    d.textContent = String(s);
    return d.innerHTML;
}

// --- Refresh & SSE ---

function refreshAll() {
    loadStatus();
    loadGoogleStatus();
    loadPending();
    loadActivity();
    loadMemory();
    loadStats();
}

// Initial load
refreshAll();

// SSE for live updates
const evtSource = new EventSource('/api/events');
evtSource.onmessage = () => refreshAll();
evtSource.onerror = () => {
    console.warn('SSE connection lost, will retry...');
};

// Fallback polling every 30s
setInterval(refreshAll, 30000);
