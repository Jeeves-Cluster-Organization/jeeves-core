/**
 * Governance View - System status, agent configuration, and tool health
 */

class GovernanceApp {
    constructor() {
        this.baseUrl = window.location.origin;
        this.dashboardData = null;
    }

    async init() {
        console.log('[Governance] Initializing...');

        // Setup event listeners
        this.setupEventListeners();

        // Load dashboard data
        await this.loadDashboard();

        console.log('[Governance] Initialized');
    }

    setupEventListeners() {
        // Refresh button
        const refreshBtn = document.getElementById('refresh-btn');
        refreshBtn?.addEventListener('click', () => this.loadDashboard());
    }

    async loadDashboard() {
        const loadingState = document.getElementById('loading-state');
        const dashboardContent = document.getElementById('dashboard-content');
        const errorState = document.getElementById('error-state');

        // Show loading
        if (loadingState) loadingState.classList.remove('hidden');
        if (dashboardContent) dashboardContent.classList.add('hidden');
        if (errorState) errorState.classList.add('hidden');

        try {
            const response = await fetch(`${this.baseUrl}/api/v1/governance/dashboard`);

            if (!response.ok) {
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }

            this.dashboardData = await response.json();

            // Hide loading, show content
            if (loadingState) loadingState.classList.add('hidden');
            if (dashboardContent) dashboardContent.classList.remove('hidden');

            // Render all sections
            this.renderSystemHealth();
            this.renderAgents();
            this.renderTools();
            this.renderMemoryLayers();
            this.renderConfig();

        } catch (error) {
            console.error('[Governance] Error loading dashboard:', error);

            if (loadingState) loadingState.classList.add('hidden');
            if (errorState) {
                errorState.classList.remove('hidden');
                const errorMsg = document.getElementById('error-message');
                if (errorMsg) errorMsg.textContent = error.message;
            }

            if (window.Jeeves?.notify) {
                window.Jeeves.notify.error('Failed to load governance data');
            }
        }
    }

    renderSystemHealth() {
        const container = document.getElementById('system-health');
        if (!container || !this.dashboardData) return;

        const data = this.dashboardData;

        // Calculate overall health metrics - no dummy data
        const totalTools = data.tools?.length || 0;
        const healthyTools = data.tools?.filter(t => t.status === 'healthy' || t.success_rate >= 0.9).length || 0;
        const avgSuccessRate = totalTools > 0
            ? (data.tools.reduce((sum, t) => sum + (t.success_rate || 0), 0) / totalTools * 100).toFixed(1)
            : 'N/A';

        // Agents and memory layers - show actual count or "N/A" if no data (P2: no dummy data)
        const agentCount = data.agents?.length > 0 ? data.agents.length : 'N/A';
        const memoryLayerCount = data.memory_layers?.length > 0
            ? data.memory_layers.filter(l => l.status === 'active').length
            : 'N/A';

        container.innerHTML = `
            <div class="health-card">
                <div class="health-card-value text-green-600">${healthyTools}/${totalTools}</div>
                <div class="health-card-label">Healthy Tools</div>
            </div>
            <div class="health-card">
                <div class="health-card-value text-blue-600">${avgSuccessRate}%</div>
                <div class="health-card-label">Avg Success Rate</div>
            </div>
            <div class="health-card">
                <div class="health-card-value text-purple-600">${agentCount}</div>
                <div class="health-card-label">Active Agents</div>
            </div>
            <div class="health-card">
                <div class="health-card-value text-orange-600">${memoryLayerCount}</div>
                <div class="health-card-label">Memory Layers</div>
            </div>
        `;
    }

    renderAgents() {
        const container = document.getElementById('agents-grid');
        if (!container) return;

        // Get agents from API response - no fallback dummy data (P2: Fail loudly)
        const agents = this.dashboardData?.agents || [];

        if (agents.length === 0) {
            container.innerHTML = `
                <div class="text-gray-500 dark:text-gray-400 py-4 text-center col-span-full">
                    Agent status data not available. GetAgents RPC not implemented.
                </div>
            `;
            return;
        }

        container.innerHTML = agents.map(agent => `
            <div class="agent-card">
                <div class="agent-status-indicator ${agent.status === 'active' || agent.enabled !== false ? 'enabled' : 'disabled'}"></div>
                <span class="agent-name">${this.escapeHtml(agent.name)}</span>
            </div>
        `).join('');
    }

    renderTools() {
        const tbody = document.getElementById('tools-tbody');
        if (!tbody) return;

        const tools = this.dashboardData?.tools || [];

        if (tools.length === 0) {
            tbody.innerHTML = `
                <tr>
                    <td colspan="5" class="py-4 text-center text-gray-500 dark:text-gray-400">
                        No tool data available
                    </td>
                </tr>
            `;
            return;
        }

        tbody.innerHTML = tools.map(tool => {
            const successRate = tool.success_rate != null
                ? `${(tool.success_rate * 100).toFixed(1)}%`
                : 'N/A';

            const avgLatency = tool.avg_latency_ms != null
                ? `${tool.avg_latency_ms.toFixed(0)}ms`
                : 'N/A';

            const statusClass = tool.success_rate >= 0.95 ? 'ok'
                : tool.success_rate >= 0.8 ? 'warning'
                : 'error';

            const circuitState = tool.circuit_state || 'closed';
            const circuitClass = circuitState.toLowerCase().replace('_', '-');

            return `
                <tr class="text-gray-700 dark:text-gray-300">
                    <td class="py-2 font-medium">${this.escapeHtml(tool.tool_name)}</td>
                    <td class="py-2">
                        <span class="tool-status-badge ${statusClass}">
                            ${statusClass === 'ok' ? 'OK' : statusClass === 'warning' ? 'Warn' : 'Error'}
                        </span>
                    </td>
                    <td class="py-2">${successRate}</td>
                    <td class="py-2">${avgLatency}</td>
                    <td class="py-2">
                        <span class="circuit-badge ${circuitClass}">
                            ${circuitState}
                        </span>
                    </td>
                </tr>
            `;
        }).join('');
    }

    renderMemoryLayers() {
        const container = document.getElementById('memory-layers');
        if (!container) return;

        // Get memory layers from API response - no fallback dummy data (P2: Fail loudly)
        const layers = this.dashboardData?.memory_layers || [];

        if (layers.length === 0) {
            container.innerHTML = `
                <div class="text-gray-500 dark:text-gray-400 py-4 text-center">
                    Memory layer status data not available. GetMemoryLayers RPC not implemented.
                </div>
            `;
            return;
        }

        container.innerHTML = layers.map(layer => {
            const isActive = layer.status === 'active' || layer.status === 'wired';
            const statusIcon = isActive
                ? '<svg class="w-3 h-3 text-green-500" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd"/></svg>'
                : '<svg class="w-3 h-3 text-gray-400" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"/></svg>';

            return `
                <div class="memory-layer-card">
                    <div class="memory-layer-name">${this.escapeHtml(layer.name)}</div>
                    <div class="memory-layer-status ${isActive ? 'text-green-600' : 'text-gray-500'}">
                        ${statusIcon}
                        <span>${layer.status}</span>
                    </div>
                </div>
            `;
        }).join('');
    }

    renderConfig() {
        const container = document.getElementById('config-grid');
        if (!container) return;

        const config = this.dashboardData?.config || {};

        // Default config values if not provided
        const configItems = [
            { label: 'LLM Provider', value: config.llm_provider || 'ollama' },
            { label: 'Database', value: config.database_backend || 'postgres' },
            { label: 'Memory', value: config.memory_enabled !== false ? 'Enabled' : 'Disabled' },
            { label: 'Max Iterations', value: config.max_iterations || '5' },
            { label: 'Confidence Threshold', value: config.confidence_threshold || '0.85' },
            { label: 'Circuit Breaker', value: config.circuit_breaker_threshold || '5 failures' }
        ];

        container.innerHTML = configItems.map(item => `
            <div class="config-item">
                <div class="config-label">${this.escapeHtml(item.label)}</div>
                <div class="config-value">${this.escapeHtml(String(item.value))}</div>
            </div>
        `).join('');
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
}

// Initialize on DOM ready
window.governanceApp = null;
document.addEventListener('DOMContentLoaded', () => {
    window.governanceApp = new GovernanceApp();
    window.governanceApp.init();
});
