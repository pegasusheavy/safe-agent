<script lang="ts">
    import { onMount } from 'svelte';
    import { t } from '../lib/i18n';
    import { api } from '../lib/api';
    import type { KnowledgeNode, KnowledgeNeighbor } from '../lib/types';

    interface GraphNode {
        id: number;
        label: string;
        type: string;
        x: number;
        y: number;
        vx: number;
        vy: number;
        radius: number;
    }

    interface GraphEdge {
        source: number;
        target: number;
        relation: string;
    }

    let canvas: HTMLCanvasElement;
    let nodes = $state<GraphNode[]>([]);
    let edges = $state<GraphEdge[]>([]);
    let selectedNode = $state<GraphNode | null>(null);
    let loading = $state(true);
    let zoom = $state(1);
    let panX = $state(0);
    let panY = $state(0);
    let dragging = $state(false);
    let dragNode: GraphNode | null = null;
    let lastMouse = { x: 0, y: 0 };
    let animFrame: number;

    const TYPE_COLORS: Record<string, string> = {
        person: '#ff9800',
        concept: '#2196f3',
        event: '#9c27b0',
        place: '#4caf50',
        bookmark: '#e91e63',
        tag: '#00bcd4',
        fact: '#ff5722',
        entity: '#607d8b',
    };

    function typeColor(type: string): string {
        return TYPE_COLORS[type.toLowerCase()] ?? '#a89c90';
    }

    async function loadGraph() {
        loading = true;
        try {
            const rawNodes = await api<KnowledgeNode[]>('GET', '/api/knowledge/nodes?limit=200');
            const w = canvas?.width ?? 800;
            const h = canvas?.height ?? 500;

            nodes = rawNodes.map((n, i) => ({
                id: n.id,
                label: n.label,
                type: n.node_type ?? 'node',
                x: w / 2 + (Math.random() - 0.5) * w * 0.6,
                y: h / 2 + (Math.random() - 0.5) * h * 0.6,
                vx: 0,
                vy: 0,
                radius: Math.min(8 + (n.content?.length ?? 0) / 50, 20),
            }));

            // Load edges by querying neighbors for each node (batch first 50)
            const edgeSet = new Set<string>();
            const newEdges: GraphEdge[] = [];
            const nodeIds = nodes.slice(0, 80).map(n => n.id);

            const batchSize = 10;
            for (let i = 0; i < nodeIds.length; i += batchSize) {
                const batch = nodeIds.slice(i, i + batchSize);
                const results = await Promise.all(
                    batch.map(id =>
                        api<KnowledgeNeighbor[]>('GET', `/api/knowledge/nodes/${id}/neighbors`)
                            .then(neighbors => ({ id, neighbors }))
                            .catch(() => ({ id, neighbors: [] as KnowledgeNeighbor[] }))
                    )
                );
                for (const { id, neighbors } of results) {
                    for (const nb of neighbors) {
                        const targetNode = nodes.find(n => n.label === nb.node.label);
                        if (!targetNode) continue;
                        const key = [Math.min(id, targetNode.id), Math.max(id, targetNode.id)].join('-');
                        if (!edgeSet.has(key)) {
                            edgeSet.add(key);
                            newEdges.push({ source: id, target: targetNode.id, relation: nb.edge.relation });
                        }
                    }
                }
            }
            edges = newEdges;
        } catch (e) {
            console.error('loadGraph:', e);
        }
        loading = false;
    }

    function simulate() {
        const alpha = 0.3;
        const repulsion = 2000;
        const attraction = 0.005;
        const damping = 0.85;

        const nodeMap = new Map(nodes.map(n => [n.id, n]));

        for (const a of nodes) {
            // Repulsion from all other nodes
            for (const b of nodes) {
                if (a.id === b.id) continue;
                const dx = a.x - b.x;
                const dy = a.y - b.y;
                const dist = Math.max(Math.sqrt(dx * dx + dy * dy), 1);
                const force = repulsion / (dist * dist);
                a.vx += (dx / dist) * force * alpha;
                a.vy += (dy / dist) * force * alpha;
            }

            // Centering force
            const cx = (canvas?.width ?? 800) / 2;
            const cy = (canvas?.height ?? 500) / 2;
            a.vx += (cx - a.x) * 0.001;
            a.vy += (cy - a.y) * 0.001;
        }

        // Attraction along edges
        for (const e of edges) {
            const a = nodeMap.get(e.source);
            const b = nodeMap.get(e.target);
            if (!a || !b) continue;
            const dx = b.x - a.x;
            const dy = b.y - a.y;
            const dist = Math.sqrt(dx * dx + dy * dy);
            const force = dist * attraction;
            a.vx += dx * force;
            a.vy += dy * force;
            b.vx -= dx * force;
            b.vy -= dy * force;
        }

        // Apply velocities
        for (const n of nodes) {
            if (dragNode && n.id === dragNode.id) continue;
            n.vx *= damping;
            n.vy *= damping;
            n.x += n.vx;
            n.y += n.vy;
        }
    }

    function render() {
        if (!canvas) return;
        const ctx = canvas.getContext('2d');
        if (!ctx) return;

        const w = canvas.width;
        const h = canvas.height;
        const isDark = document.documentElement.getAttribute('data-theme') !== 'light';

        ctx.clearRect(0, 0, w, h);
        ctx.save();
        ctx.translate(panX, panY);
        ctx.scale(zoom, zoom);

        // Draw edges
        ctx.lineWidth = 1;
        const nodeMap = new Map(nodes.map(n => [n.id, n]));
        for (const e of edges) {
            const a = nodeMap.get(e.source);
            const b = nodeMap.get(e.target);
            if (!a || !b) continue;
            ctx.beginPath();
            ctx.moveTo(a.x, a.y);
            ctx.lineTo(b.x, b.y);
            ctx.strokeStyle = isDark ? 'rgba(168,156,144,0.2)' : 'rgba(107,97,88,0.2)';
            ctx.stroke();
        }

        // Draw nodes
        for (const n of nodes) {
            ctx.beginPath();
            ctx.arc(n.x, n.y, n.radius, 0, Math.PI * 2);
            ctx.fillStyle = typeColor(n.type);
            if (selectedNode?.id === n.id) {
                ctx.shadowColor = typeColor(n.type);
                ctx.shadowBlur = 15;
            }
            ctx.fill();
            ctx.shadowBlur = 0;

            // Label
            ctx.fillStyle = isDark ? '#f5f0eb' : '#1a1410';
            ctx.font = '10px "Fira Sans", sans-serif';
            ctx.textAlign = 'center';
            const label = n.label.length > 18 ? n.label.slice(0, 16) + '…' : n.label;
            ctx.fillText(label, n.x, n.y + n.radius + 12);
        }

        ctx.restore();

        simulate();
        animFrame = requestAnimationFrame(render);
    }

    function hitTest(mx: number, my: number): GraphNode | null {
        const x = (mx - panX) / zoom;
        const y = (my - panY) / zoom;
        for (const n of nodes) {
            const dx = n.x - x;
            const dy = n.y - y;
            if (dx * dx + dy * dy < (n.radius + 4) * (n.radius + 4)) return n;
        }
        return null;
    }

    function handleMouseDown(e: MouseEvent) {
        const rect = canvas.getBoundingClientRect();
        const mx = e.clientX - rect.left;
        const my = e.clientY - rect.top;
        const hit = hitTest(mx, my);
        if (hit) {
            dragNode = hit;
            selectedNode = hit;
        } else {
            dragging = true;
        }
        lastMouse = { x: e.clientX, y: e.clientY };
    }

    function handleMouseMove(e: MouseEvent) {
        if (dragNode) {
            const rect = canvas.getBoundingClientRect();
            dragNode.x = (e.clientX - rect.left - panX) / zoom;
            dragNode.y = (e.clientY - rect.top - panY) / zoom;
            dragNode.vx = 0;
            dragNode.vy = 0;
        } else if (dragging) {
            panX += e.clientX - lastMouse.x;
            panY += e.clientY - lastMouse.y;
        }
        lastMouse = { x: e.clientX, y: e.clientY };
    }

    function handleMouseUp() {
        dragNode = null;
        dragging = false;
    }

    function handleWheel(e: WheelEvent) {
        e.preventDefault();
        const factor = e.deltaY > 0 ? 0.9 : 1.1;
        zoom = Math.max(0.2, Math.min(5, zoom * factor));
    }

    function resetView() {
        zoom = 1;
        panX = 0;
        panY = 0;
        selectedNode = null;
    }

    onMount(() => {
        const resize = () => {
            if (canvas) {
                const rect = canvas.parentElement!.getBoundingClientRect();
                canvas.width = rect.width;
                canvas.height = 500;
            }
        };
        resize();
        window.addEventListener('resize', resize);
        loadGraph().then(() => {
            animFrame = requestAnimationFrame(render);
        });
        return () => {
            cancelAnimationFrame(animFrame);
            window.removeEventListener('resize', resize);
        };
    });

    const selectedEdges = $derived(
        selectedNode
            ? edges.filter(e => e.source === selectedNode!.id || e.target === selectedNode!.id)
            : []
    );
</script>

<div class="relative">
    {#if loading}
        <div class="absolute inset-0 flex items-center justify-center bg-surface/80 z-10 rounded-lg">
            <p class="text-text-muted text-sm">
                <i class="fa-solid fa-spinner fa-spin mr-1"></i> {t('knowledge.loading_graph')}
            </p>
        </div>
    {/if}

    <div class="flex items-center justify-between mb-2">
        <p class="text-xs text-text-subtle">{t('knowledge.graph_hint')}</p>
        <div class="flex gap-1">
            <button onclick={() => zoom = Math.min(5, zoom * 1.3)} class="px-2 py-1 text-xs border border-border rounded bg-surface hover:bg-surface-elevated" title={t('knowledge.zoom_in')}>
                <i class="fa-solid fa-plus"></i>
            </button>
            <button onclick={() => zoom = Math.max(0.2, zoom * 0.7)} class="px-2 py-1 text-xs border border-border rounded bg-surface hover:bg-surface-elevated" title={t('knowledge.zoom_out')}>
                <i class="fa-solid fa-minus"></i>
            </button>
            <button onclick={resetView} class="px-2 py-1 text-xs border border-border rounded bg-surface hover:bg-surface-elevated" title={t('knowledge.reset_view')}>
                <i class="fa-solid fa-arrows-to-dot"></i>
            </button>
        </div>
    </div>

    <canvas
        bind:this={canvas}
        class="kg-canvas bg-surface-muted border border-border"
        onmousedown={handleMouseDown}
        onmousemove={handleMouseMove}
        onmouseup={handleMouseUp}
        onmouseleave={handleMouseUp}
        onwheel={handleWheel}
    ></canvas>

    <!-- Legend -->
    <div class="flex flex-wrap gap-3 mt-2">
        {#each Object.entries(TYPE_COLORS) as [type, color]}
            <div class="flex items-center gap-1 text-[10px] text-text-muted">
                <span class="inline-block w-2.5 h-2.5 rounded-full" style="background:{color}"></span>
                {type}
            </div>
        {/each}
    </div>

    <!-- Selected node detail -->
    {#if selectedNode}
        <div class="mt-3 p-3 rounded-lg bg-surface-elevated border border-border">
            <div class="flex items-center justify-between mb-1">
                <span class="text-sm font-semibold text-text">{selectedNode.label}</span>
                <span class="text-[10px] px-1.5 py-0.5 rounded-full border border-border text-text-muted">{selectedNode.type}</span>
            </div>
            {#if selectedEdges.length > 0}
                <div class="mt-2 space-y-1">
                    {#each selectedEdges as edge}
                        {@const other = nodes.find(n => n.id === (edge.source === selectedNode!.id ? edge.target : edge.source))}
                        {#if other}
                            <div class="text-xs text-text-muted">
                                <span class="text-accent-500">{edge.relation}</span> → {other.label}
                            </div>
                        {/if}
                    {/each}
                </div>
            {/if}
        </div>
    {/if}
</div>
