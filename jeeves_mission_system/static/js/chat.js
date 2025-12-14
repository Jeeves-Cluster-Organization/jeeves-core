/**
 * ChatClient - Main chat interface for Code Analysis Agent
 *
 * Features:
 * - Real-time messaging via WebSocket
 * - Session management (create, list, switch)
 * - Message history with markdown rendering
 * - Dark mode support
 * - Auto-scroll and typing indicators
 * - Agent activity view (6-agent pipeline)
 */

class ChatClient {
    constructor() {
        this.baseUrl = window.location.origin;
        this.ws = null;
        this.wsToken = null;  // WebSocket auth token (optional, loaded from config)
        this.currentSessionId = null;
        this.userId = 'demo-user';
        this.sessions = [];
        this.messages = [];
        this.isConnected = false;
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 10;
        this.streamingMessageId = null;
        this.isSidebarOpen = false;
        this.isViewingOldSession = false;  // Track if viewing past conversation
        this.isInternalViewOpen = false;  // Track internal/agent view state
        this.agentActivity = [];  // Store agent activity events
        this.pendingClarificationThreadId = null;  // Track pending clarification for P1 compliance

        // Bind methods
        this.init = this.init.bind(this);
        this.connectWebSocket = this.connectWebSocket.bind(this);
        this.handleWebSocketMessage = this.handleWebSocketMessage.bind(this);
        this.sendMessage = this.sendMessage.bind(this);
        this.loadSessions = this.loadSessions.bind(this);
        this.loadMessages = this.loadMessages.bind(this);
    }

    async init() {
        console.log('Initializing ChatClient...');

        // Load theme
        this.loadTheme();

        // Setup event listeners
        this.setupEventListeners();

        // Get user ID from input
        const userIdInput = document.getElementById('user-id');
        if (userIdInput.value) {
            this.userId = userIdInput.value;
        }

        // Connect WebSocket
        this.connectWebSocket();

        // Load sessions
        await this.loadSessions();

        console.log('ChatClient initialized');
    }

    setupEventListeners() {
        // Theme toggle
        const themeToggle = document.getElementById('theme-toggle');
        themeToggle?.addEventListener('click', () => this.toggleTheme());

        // User ID change
        const userIdInput = document.getElementById('user-id');
        userIdInput?.addEventListener('change', async (e) => {
            this.userId = e.target.value;
            this.currentSessionId = null;
            await this.loadSessions();
            this.clearMessages();
        });

        // New session
        const newSessionBtn = document.getElementById('new-session-btn');
        newSessionBtn?.addEventListener('click', () => this.createSession());

        // Send message
        const sendBtn = document.getElementById('send-btn');
        sendBtn?.addEventListener('click', () => this.sendMessage());

        // Message input
        const messageInput = document.getElementById('message-input');
        messageInput?.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                this.sendMessage();
            }
        });

        messageInput?.addEventListener('input', (e) => {
            this.autoResizeTextarea(e.target);
            const sendBtn = document.getElementById('send-btn');
            sendBtn.disabled = !e.target.value.trim();
        });

        // Sidebar toggle for mobile
        const toggleSidebar = document.getElementById('toggle-sidebar');
        const sidebarOverlay = document.getElementById('sidebar-overlay');

        toggleSidebar?.addEventListener('click', () => this.setSidebarOpen(!this.isSidebarOpen));
        sidebarOverlay?.addEventListener('click', () => this.setSidebarOpen(false));
        window.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && this.isSidebarOpen) {
                this.setSidebarOpen(false);
            }
        });

        // Ensure the sidebar auto-closes when leaving mobile view
        const desktopMediaQuery = window.matchMedia('(min-width: 1024px)');
        desktopMediaQuery.addEventListener('change', (event) => {
            if (event.matches) {
                this.setSidebarOpen(false);
            }
        });

        // Internal view toggle
        const toggleInternalView = document.getElementById('toggle-internal-view');
        toggleInternalView?.addEventListener('click', () => this.toggleInternalView());
    }

    // =========================================================================
    // Internal View Management
    // =========================================================================

    toggleInternalView() {
        this.isInternalViewOpen = !this.isInternalViewOpen;

        const panel = document.getElementById('internal-panel');
        const toggleIcon = document.getElementById('internal-toggle-icon');

        if (panel) {
            panel.classList.toggle('hidden', !this.isInternalViewOpen);
        }

        if (toggleIcon) {
            toggleIcon.style.transform = this.isInternalViewOpen ? 'rotate(180deg)' : '';
        }
    }

    addAgentActivity(event_type, payload, timestamp, category, severity) {
        // Add activity to the list with UnifiedEvent metadata
        this.agentActivity.unshift({
            event: event_type,  // Use event_type for backward compat with renderActivityItem
            payload,
            timestamp: timestamp || new Date().toISOString(),
            category,
            severity
        });

        // Keep only last 50 activities
        if (this.agentActivity.length > 50) {
            this.agentActivity = this.agentActivity.slice(0, 50);
        }

        // Render the internal view
        this.renderInternalView();
    }

    renderInternalView() {
        const container = document.getElementById('internal-content');
        if (!container) return;

        if (this.agentActivity.length === 0) {
            container.innerHTML = `
                <div class="text-gray-500 dark:text-gray-400 text-center py-4">
                    Send a message to see agent activity
                </div>
            `;
            return;
        }

        container.innerHTML = this.agentActivity.map(activity => this.renderActivityItem(activity)).join('');
    }

    renderActivityItem(activity) {
        const { event, payload, timestamp } = activity;
        const time = new Date(timestamp).toLocaleTimeString('en-US', {
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit'
        });

        // Determine agent name and status based on event
        // Supports all 7 agents: Perception → Intent → Planner → Traverser → Synthesizer → Critic → Integration
        let agentName = 'System';
        let statusClass = 'bg-gray-100 dark:bg-slate-700';
        let statusIcon = '';
        let details = '';

        // Generic agent events (agent.started, agent.completed) - use payload.agent for name
        if (event.startsWith('agent.')) {
            agentName = payload?.agent ? this._formatAgentName(payload.agent) : 'Agent';
            statusClass = event.includes('started') ? 'bg-indigo-50 dark:bg-indigo-900/30' : 'bg-teal-50 dark:bg-teal-900/30';
            statusIcon = event.includes('started')
                ? '<svg class="w-3 h-3 text-indigo-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"></path><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>'
                : '<svg class="w-3 h-3 text-teal-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>';
            details = this._buildAgentDetails(payload);
        }
        // Perception agent
        else if (event.startsWith('perception.')) {
            agentName = 'Perception';
            statusClass = 'bg-cyan-50 dark:bg-cyan-900/30';
            statusIcon = '<svg class="w-3 h-3 text-cyan-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"></path><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"></path></svg>';
            if (payload?.session_scope) {
                details = `Scope: ${payload.session_scope}`;
            }
        }
        // Intent agent
        else if (event.startsWith('intent.')) {
            agentName = 'Intent';
            statusClass = 'bg-violet-50 dark:bg-violet-900/30';
            statusIcon = '<svg class="w-3 h-3 text-violet-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"></path></svg>';
            if (payload?.intent) {
                details = `Intent: ${payload.intent}`;
            }
            if (payload?.confidence) {
                details += ` | Confidence: ${(payload.confidence * 100).toFixed(0)}%`;
            }
            if (payload?.complexity) {
                details += ` | Complexity: ${payload.complexity}`;
            }
        }
        // Planner agent
        else if (event.startsWith('planner.')) {
            agentName = 'Planner';
            statusClass = 'bg-blue-50 dark:bg-blue-900/30';
            statusIcon = '<svg class="w-3 h-3 text-blue-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2"></path></svg>';
            if (payload?.step_count) {
                details = `Steps: ${payload.step_count}`;
            }
            if (payload?.tools && payload.tools.length > 0) {
                details += ` | Tools: ${payload.tools.slice(0, 3).join(', ')}`;
                if (payload.tools.length > 3) details += '...';
            }
            if (payload?.strategy) {
                details += ` | Strategy: ${payload.strategy}`;
            }
        }
        // Traverser agent (tool execution)
        else if (event.startsWith('traverser.') || event.startsWith('executor.')) {
            agentName = 'Traverser';
            statusClass = 'bg-purple-50 dark:bg-purple-900/30';
            statusIcon = '<svg class="w-3 h-3 text-purple-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z"></path></svg>';
            if (payload?.tool_name) {
                details = `Tool: ${payload.tool_name}`;
            }
            if (payload?.status) {
                details += ` | Status: ${payload.status}`;
            }
            if (payload?.steps_executed) {
                details = `Steps: ${payload.steps_executed}`;
            }
            if (payload?.files_found) {
                details += ` | Files: ${payload.files_found}`;
            }
        }
        // Synthesizer agent
        else if (event.startsWith('synthesizer.')) {
            agentName = 'Synthesizer';
            statusClass = 'bg-pink-50 dark:bg-pink-900/30';
            statusIcon = '<svg class="w-3 h-3 text-pink-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19.428 15.428a2 2 0 00-1.022-.547l-2.387-.477a6 6 0 00-3.86.517l-.318.158a6 6 0 01-3.86.517L6.05 15.21a2 2 0 00-1.806.547M8 4h8l-1 1v5.172a2 2 0 00.586 1.414l5 5c1.26 1.26.367 3.414-1.415 3.414H4.828c-1.782 0-2.674-2.154-1.414-3.414l5-5A2 2 0 009 10.172V5L8 4z"></path></svg>';
            if (payload?.entities_count) {
                details = `Entities: ${payload.entities_count}`;
            }
            if (payload?.flows_count) {
                details += ` | Flows: ${payload.flows_count}`;
            }
            if (payload?.patterns_count) {
                details += ` | Patterns: ${payload.patterns_count}`;
            }
        }
        // Critic agent
        else if (event.startsWith('critic.')) {
            agentName = 'Critic';
            statusClass = 'bg-yellow-50 dark:bg-yellow-900/30';
            statusIcon = '<svg class="w-3 h-3 text-yellow-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>';
            if (payload?.action || payload?.verdict) {
                details = `Verdict: ${payload.action || payload.verdict}`;
            }
            if (payload?.confidence) {
                details += ` | Confidence: ${(payload.confidence * 100).toFixed(0)}%`;
            }
            if (payload?.alignment_score) {
                details += ` | Alignment: ${(payload.alignment_score * 100).toFixed(0)}%`;
            }
        }
        // Integration agent
        else if (event.startsWith('integration.')) {
            agentName = 'Integration';
            statusClass = 'bg-emerald-50 dark:bg-emerald-900/30';
            statusIcon = '<svg class="w-3 h-3 text-emerald-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"></path></svg>';
            if (payload?.response_length) {
                details = `Response: ${payload.response_length} chars`;
            }
            if (payload?.citations_count) {
                details += ` | Citations: ${payload.citations_count}`;
            }
        }
        // Orchestrator events
        else if (event.startsWith('orchestrator.')) {
            agentName = 'Orchestrator';
            if (event === 'orchestrator.completed' || event === 'orchestrator.started') {
                statusClass = 'bg-green-50 dark:bg-green-900/30';
                statusIcon = '<svg class="w-3 h-3 text-green-500" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd"></path></svg>';
            } else if (event === 'orchestrator.stage_transition') {
                statusClass = 'bg-amber-50 dark:bg-amber-900/30';
                statusIcon = '<svg class="w-3 h-3 text-amber-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 5l7 7-7 7M5 5l7 7-7 7"></path></svg>';
                if (payload?.from_stage && payload?.to_stage) {
                    details = `Stage ${payload.from_stage} → ${payload.to_stage}`;
                }
                if (payload?.remaining_goals !== undefined) {
                    details += ` | Goals remaining: ${payload.remaining_goals}`;
                }
            } else {
                statusClass = 'bg-red-50 dark:bg-red-900/30';
                statusIcon = '<svg class="w-3 h-3 text-red-500" fill="currentColor" viewBox="0 0 20 20"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"></path></svg>';
            }
        }

        return `
            <div class="${statusClass} rounded-lg p-2 mb-2">
                <div class="flex items-center gap-1.5 mb-1">
                    ${statusIcon}
                    <span class="font-medium text-gray-800 dark:text-white">${agentName}</span>
                    <span class="text-gray-400 dark:text-gray-500 ml-auto text-xs">${time}</span>
                </div>
                <div class="text-gray-600 dark:text-gray-300 text-sm">
                    ${event.split('.').pop()}
                </div>
                ${details ? `<div class="text-gray-500 dark:text-gray-400 text-xs mt-1">${details}</div>` : ''}
            </div>
        `;
    }

    _formatAgentName(name) {
        // Capitalize first letter of agent name
        if (!name) return 'Agent';
        return name.charAt(0).toUpperCase() + name.slice(1);
    }

    _buildAgentDetails(payload) {
        // Build details string from common payload fields
        let details = '';
        if (payload?.intent) details += `Intent: ${payload.intent}`;
        if (payload?.confidence) {
            if (details) details += ' | ';
            details += `Confidence: ${(payload.confidence * 100).toFixed(0)}%`;
        }
        if (payload?.step_count) {
            if (details) details += ' | ';
            details += `Steps: ${payload.step_count}`;
        }
        return details;
    }

    setSidebarOpen(isOpen) {
        const sidebar = document.getElementById('sessions-sidebar');
        const overlay = document.getElementById('sidebar-overlay');
        const toggleSidebar = document.getElementById('toggle-sidebar');

        this.isSidebarOpen = Boolean(isOpen);

        sidebar?.classList.toggle('open', this.isSidebarOpen);
        overlay?.classList.toggle('active', this.isSidebarOpen);
        if (overlay) {
            overlay.style.pointerEvents = this.isSidebarOpen ? 'auto' : 'none';
        }
        toggleSidebar?.setAttribute('aria-expanded', String(this.isSidebarOpen));
    }

    // =========================================================================
    // WebSocket Management
    // =========================================================================

    connectWebSocket() {
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        // Support optional WebSocket token (for production environments)
        // In development, auth is disabled by default
        const token = this.wsToken || 'local-dev-token';
        const wsUrl = `${protocol}//${window.location.host}/ws?token=${encodeURIComponent(token)}`;

        console.log('Connecting to WebSocket:', wsUrl);

        try {
            this.ws = new WebSocket(wsUrl);

            this.ws.onopen = () => {
                console.log('WebSocket connected');
                this.isConnected = true;
                this.reconnectAttempts = 0;
                this.updateConnectionStatus('Connected', 'green');
            };

            this.ws.onmessage = (event) => {
                if (event.data === 'ping') {
                    this.ws.send('pong');
                    return;
                }
                if (event.data === 'pong') return;

                try {
                    const message = JSON.parse(event.data);
                    this.handleWebSocketMessage(message);
                } catch (error) {
                    console.error('Failed to parse WebSocket message:', error);
                }
            };

            this.ws.onerror = (error) => {
                console.error('WebSocket error:', error);
                this.updateConnectionStatus('Error', 'red');
            };

            this.ws.onclose = () => {
                console.log('WebSocket disconnected');
                this.isConnected = false;
                this.updateConnectionStatus('Disconnected', 'gray');

                // Attempt reconnection
                if (this.reconnectAttempts < this.maxReconnectAttempts) {
                    this.reconnectAttempts++;
                    const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 30000);
                    console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts})`);
                    setTimeout(() => this.connectWebSocket(), delay);
                }
            };
        } catch (error) {
            console.error('Failed to create WebSocket:', error);
            this.updateConnectionStatus('Error', 'red');
        }
    }

    handleWebSocketMessage(message) {
        console.log('Received WebSocket message:', message);

        // UnifiedEvent format (constitutional standard)
        if (message.type !== 'event') {
            console.warn('WebSocket message is not a UnifiedEvent:', message);
            return;
        }

        // Extract UnifiedEvent fields
        const event_type = message.event_type;
        const category = message.category;
        const payload = message.payload || {};
        const session_id = message.session_id;
        const request_id = message.request_id;
        const severity = message.severity;
        const timestamp = message.timestamp;

        // Guard against missing event_type
        if (!event_type) {
            console.warn('UnifiedEvent has no event_type field:', message);
            return;
        }

        // Capture agent events for internal view by category
        // 7-Agent Pipeline: Perception → Intent → Planner → Executor → Synthesizer → Critic → Integration
        if (category === 'agent_lifecycle' ||
            category === 'tool_execution' ||
            category === 'critic_decision' ||
            category === 'stage_transition' ||
            category === 'pipeline_flow') {
            this.addAgentActivity(event_type, payload, timestamp, category, severity);
        }

        // Route by event_type
        switch (event_type) {
            case 'orchestrator.completed':
                this.handleOrchestratorCompleted(payload);
                break;
            case 'orchestrator.confirmation_requested':
                // Handle confirmation via WebSocket (same as completed with confirmation)
                this.handleOrchestratorCompleted(payload);
                break;
            case 'orchestrator.clarification':
                // P1 Compliance: If uncertain, say so explicitly
                this.handleOrchestratorClarification(payload);
                break;
            case 'orchestrator.error':
                this.handleOrchestratorFailed(payload);
                break;
            case 'session.created':
            case 'session.updated':
            case 'session.deleted':
                this.loadSessions();
                break;
            default:
                // Log unhandled events for debugging
                if (severity === 'error' || severity === 'critical') {
                    console.error('Unhandled error event:', event_type, payload);
                } else {
                    console.debug('Unhandled event:', event_type, category);
                }
        }
    }

    handleOrchestratorCompleted(payload) {
        // Hide typing indicator
        this.hideTypingIndicator();

        // Handle confirmation needed
        if (payload.confirmation_needed) {
            this.addConfirmationMessage({
                role: 'assistant',
                content: payload.confirmation_message || 'Confirmation required',
                confirmation_id: payload.confirmation_id,
                created_at: new Date().toISOString()
            });
            return;
        }

        // Add assistant message
        if (payload.response_text) {
            this.addMessage({
                role: 'assistant',
                content: payload.response_text,
                created_at: new Date().toISOString()
            });
        }
    }

    handleOrchestratorFailed(payload) {
        this.hideTypingIndicator();
        this.showNotification('Request failed: ' + (payload.error || 'Unknown error'), 'error');
    }

    handleOrchestratorClarification(payload) {
        // P1 Compliance: System is uncertain, communicate explicitly to user
        this.hideTypingIndicator();
        this.addClarificationMessage({
            role: 'assistant',
            content: payload.clarification_question || 'Could you please clarify your request?',
            thread_id: payload.thread_id,
            created_at: new Date().toISOString()
        });
    }

    // =========================================================================
    // Session Management
    // =========================================================================

    async loadSessions() {
        try {
            const response = await fetch(
                `${this.baseUrl}/api/v1/chat/sessions?user_id=${encodeURIComponent(this.userId)}&limit=50`
            );

            if (!response.ok) {
                throw new Error(`Failed to load sessions: ${response.statusText}`);
            }

            const data = await response.json();
            // Handle both array response and {sessions: [], total: N} format
            // Be defensive: ensure sessions is always an array
            if (Array.isArray(data)) {
                this.sessions = data;
            } else if (Array.isArray(data?.sessions)) {
                this.sessions = data.sessions;
            } else {
                console.warn('Unexpected sessions response format:', data);
                this.sessions = [];
            }
            this.renderSessionList();
        } catch (error) {
            console.error('Error loading sessions:', error);
            this.showNotification('Failed to load sessions', 'error');
        }
    }

    confirmDeleteSession(sessionId) {
        const session = this.sessions.find(s => s.session_id === sessionId);
        const title = session?.title || 'this session';

        if (confirm(`Are you sure you want to delete "${title}"? This action cannot be undone.`)) {
            this.deleteSession(sessionId);
        }
    }

    async deleteSession(sessionId) {
        try {
            const response = await fetch(
                `${this.baseUrl}/api/v1/chat/sessions/${sessionId}?user_id=${encodeURIComponent(this.userId)}`,
                { method: 'DELETE' }
            );

            if (!response.ok) {
                throw new Error(`Failed to delete session: ${response.statusText}`);
            }

            this.showNotification('Session deleted successfully', 'success');

            // If deleted session was active, clear messages
            if (this.currentSessionId === sessionId) {
                this.currentSessionId = null;
                this.clearMessages();
            }

            // Reload sessions
            await this.loadSessions();
        } catch (error) {
            console.error('Error deleting session:', error);
            this.showNotification('Failed to delete session', 'error');
        }
    }

    async createSession() {
        try {
            // Clear current conversation to start fresh
            this.clearCurrentConversation();
            this.isViewingOldSession = false;  // Starting new active session

            const response = await fetch(`${this.baseUrl}/api/v1/chat/sessions`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    user_id: this.userId,
                    title: `New Chat - ${new Date().toLocaleString()}`
                })
            });

            if (!response.ok) {
                throw new Error(`Failed to create session: ${response.statusText}`);
            }

            const session = await response.json();
            this.currentSessionId = session.session_id;
            await this.loadSessions();

            this.showNotification('New chat session started', 'success');

            // Focus on input
            const input = document.getElementById('message-input');
            if (input) input.focus();
        } catch (error) {
            console.error('Error creating session:', error);
            this.showNotification('Failed to create session', 'error');
        }
    }

    async switchSession(sessionId) {
        this.currentSessionId = sessionId;
        await this.loadMessages();

        // Update active state in UI
        document.querySelectorAll('.session-item').forEach(item => {
            item.classList.remove('active');
            if (item.dataset.sessionId === sessionId) {
                item.classList.add('active');
            }
        });

        // Hide welcome message
        const welcomeMessage = document.getElementById('welcome-message');
        if (welcomeMessage) {
            welcomeMessage.classList.add('hidden');
        }
    }

    renderSessionList() {
        const sessionList = document.getElementById('session-list');
        const emptyMessage = document.getElementById('empty-sessions');

        if (!sessionList) return;

        if (this.sessions.length === 0) {
            if (emptyMessage) emptyMessage.classList.remove('hidden');
            return;
        }

        if (emptyMessage) emptyMessage.classList.add('hidden');

        // Group sessions by time
        const grouped = this.groupSessionsByTime(this.sessions);

        let html = '';
        for (const [group, sessions] of Object.entries(grouped)) {
            html += `
                <div class="mb-3">
                    <h3 class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2">
                        ${group}
                    </h3>
                    ${sessions.map(session => this.renderSessionItem(session)).join('')}
                </div>
            `;
        }

        sessionList.innerHTML = html;

        // Add click listeners to view sessions (read-only)
        document.querySelectorAll('.session-item').forEach(item => {
            const sessionId = item.dataset.sessionId;
            item.addEventListener('click', (e) => {
                // Don't trigger view if clicking delete button
                if (e.target.closest('.session-delete-btn')) return;
                this.viewSession(sessionId);
            });
        });

        // Add click listeners for delete buttons
        document.querySelectorAll('.session-delete-btn').forEach(btn => {
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                const sessionId = btn.dataset.sessionId;
                this.confirmDeleteSession(sessionId);
            });
        });
    }

    renderSessionItem(session) {
        const isActive = session.session_id === this.currentSessionId;
        const title = session.title || 'Untitled Chat';
        const messageCount = session.message_count || 0;

        return `
            <div class="session-item p-3 rounded-lg cursor-pointer hover:bg-gray-100 dark:hover:bg-slate-700 transition-colors ${isActive ? 'active bg-blue-50 dark:bg-slate-700 border-l-4 border-blue-600' : ''} group"
                 data-session-id="${session.session_id}"
                 title="Click to view conversation history">
                <div class="flex items-start justify-between">
                    <div class="flex-1 min-w-0">
                        <h4 class="text-sm font-medium text-gray-800 dark:text-white truncate">
                            ${this.escapeHtml(title)}
                        </h4>
                        <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">
                            ${messageCount} ${messageCount === 1 ? 'message' : 'messages'} • ${this.formatTimeAgo(session.last_activity)}
                        </p>
                    </div>
                    <button class="session-delete-btn p-1.5 text-gray-400 hover:text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30 rounded opacity-0 group-hover:opacity-100 transition-all flex-shrink-0"
                            data-session-id="${session.session_id}"
                            title="Delete conversation">
                        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"></path>
                        </svg>
                    </button>
                </div>
            </div>
        `;
    }

    async viewSession(sessionId) {
        // Load session messages in view-only mode
        // This allows users to see past conversations but not continue them
        const previousSessionId = this.currentSessionId;
        this.currentSessionId = sessionId;
        this.isViewingOldSession = true;  // Mark as viewing old session

        try {
            await this.loadMessages();

            // Update UI to show this session as active
            document.querySelectorAll('.session-item').forEach(item => {
                item.classList.remove('active', 'bg-blue-50', 'dark:bg-slate-700', 'border-l-4', 'border-blue-600');
                if (item.dataset.sessionId === sessionId) {
                    item.classList.add('active', 'bg-blue-50', 'dark:bg-slate-700', 'border-l-4', 'border-blue-600');
                }
            });

            // Show banner about viewing old session
            this.showNotification('Viewing past conversation. Send a message to start a new chat.', 'info');

            // Close sidebar on mobile after selection
            if (window.innerWidth < 1024) {
                this.setSidebarOpen(false);
            }
        } catch (error) {
            console.error('Error viewing session:', error);
            this.currentSessionId = previousSessionId;
            this.showNotification('Failed to load session', 'error');
        }
    }

    groupSessionsByTime(sessions) {
        const now = new Date();
        const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
        const yesterday = new Date(today);
        yesterday.setDate(yesterday.getDate() - 1);
        const weekAgo = new Date(today);
        weekAgo.setDate(weekAgo.getDate() - 7);

        const groups = {
            'Today': [],
            'Yesterday': [],
            'This Week': [],
            'Older': []
        };

        // Guard against non-array sessions
        if (!Array.isArray(sessions)) {
            console.warn('groupSessionsByTime received non-array:', sessions);
            return {};
        }

        sessions.forEach(session => {
            // Handle missing last_activity - use created_at as fallback or put in 'Older'
            const activityTime = session.last_activity || session.created_at;
            const sessionDate = activityTime ? new Date(activityTime) : new Date(0);

            if (sessionDate >= today) {
                groups['Today'].push(session);
            } else if (sessionDate >= yesterday) {
                groups['Yesterday'].push(session);
            } else if (sessionDate >= weekAgo) {
                groups['This Week'].push(session);
            } else {
                groups['Older'].push(session);
            }
        });

        // Remove empty groups
        return Object.fromEntries(
            Object.entries(groups).filter(([_, sessions]) => sessions.length > 0)
        );
    }

    // =========================================================================
    // Message Management
    // =========================================================================

    async loadMessages() {
        if (!this.currentSessionId) {
            this.clearMessages();
            return;
        }

        try {
            const response = await fetch(
                `${this.baseUrl}/api/v1/chat/sessions/${this.currentSessionId}/messages?user_id=${encodeURIComponent(this.userId)}&limit=100`
            );

            if (!response.ok) {
                throw new Error(`Failed to load messages: ${response.statusText}`);
            }

            const data = await response.json();
            // Handle both array response and {messages: [], total: N} format
            this.messages = Array.isArray(data) ? data : (data.messages || []);
            this.renderMessages();
        } catch (error) {
            console.error('Error loading messages:', error);
            this.showNotification('Failed to load messages', 'error');
        }
    }

    async sendMessage() {
        const input = document.getElementById('message-input');
        const content = input.value.trim();

        if (!content) return;

        // Clear input
        input.value = '';
        input.style.height = 'auto';
        document.getElementById('send-btn').disabled = true;

        // Check if this is a response to a pending clarification
        if (this.pendingClarificationThreadId) {
            // Add user message to UI
            this.addMessage({
                role: 'user',
                content: content,
                created_at: new Date().toISOString()
            });
            // Route to clarification handler
            const handled = await this.handleClarification(content);
            if (handled) {
                input.focus();
                return;
            }
            // If not handled, continue as normal message
        }

        // If viewing old session, start fresh conversation
        if (this.isViewingOldSession) {
            this.clearCurrentConversation();
            this.isViewingOldSession = false;
        }

        // Add user message to UI immediately
        this.addMessage({
            role: 'user',
            content: content,
            created_at: new Date().toISOString()
        });

        // Show typing indicator
        this.showTypingIndicator();

        try {
            // Check if we need to create a new session or continue existing one
            let sessionId = this.currentSessionId;

            if (!sessionId) {
                // Create new session only if we don't have one
                const sessionResponse = await fetch(`${this.baseUrl}/api/v1/chat/sessions`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({
                        user_id: this.userId,
                        title: `Chat - ${new Date().toLocaleString()}`
                    })
                });

                if (!sessionResponse.ok) {
                    throw new Error(`Failed to create session: ${sessionResponse.statusText}`);
                }

                const session = await sessionResponse.json();
                sessionId = session.session_id;
                this.currentSessionId = sessionId;
            }

            // Send message to existing or newly created session
            const response = await fetch(`${this.baseUrl}/api/v1/chat/messages?user_id=${encodeURIComponent(this.userId)}`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    message: content,
                    session_id: sessionId  // Pass session ID to continue conversation
                })
            });

            if (!response.ok) {
                const errorText = await response.text();
                throw new Error(`HTTP ${response.status}: ${errorText}`);
            }

            const result = await response.json();

            // Hide typing indicator
            this.hideTypingIndicator();

            // Check if confirmation is needed
            if (result.confirmation_needed) {
                this.addConfirmationMessage({
                    role: 'assistant',
                    content: result.confirmation_message || 'Confirmation required',
                    confirmation_id: result.confirmation_id,
                    created_at: new Date().toISOString()
                });
            } else if (result.clarification_needed) {
                // P1 Compliance: If uncertain, say so explicitly
                this.addClarificationMessage({
                    role: 'assistant',
                    content: result.clarification_question || 'Could you please clarify your request?',
                    thread_id: result.thread_id,
                    created_at: new Date().toISOString()
                });
            } else if (result.response) {
                // Display regular response
                this.addMessage({
                    role: 'assistant',
                    content: result.response,
                    created_at: new Date().toISOString()
                });
            }

            // Refresh session list (to update message count)
            await this.loadSessions();

        } catch (error) {
            console.error('Error sending message:', error);
            this.hideTypingIndicator();
            this.showNotification('Failed to send message: ' + error.message, 'error');
        } finally {
            // Re-enable input
            input.focus();
        }
    }

    clearCurrentConversation() {
        // Clear messages (start fresh)
        this.messages = [];
        const container = document.getElementById('messages-container');
        Array.from(container.children).forEach(el => {
            if (el.id !== 'welcome-message') {
                el.remove();
            }
        });

        // Hide welcome message
        const welcomeEl = document.getElementById('welcome-message');
        if (welcomeEl) {
            welcomeEl.style.display = 'none';
        }

        // Reset current session (will be set by API response)
        this.currentSessionId = null;
    }

    addMessage(message) {
        this.messages.push(message);
        this.renderMessages();
        this.scrollToBottom();
    }

    addConfirmationMessage(message) {
        // Add confirmation message with special marker
        message.is_confirmation = true;
        this.messages.push(message);
        this.renderMessages();
        this.scrollToBottom();

        // Add event listeners for confirmation buttons
        setTimeout(() => {
            const confirmBtn = document.getElementById(`confirm-${message.confirmation_id}`);
            const rejectBtn = document.getElementById(`reject-${message.confirmation_id}`);

            confirmBtn?.addEventListener('click', () => this.handleConfirmation(message.confirmation_id, 'yes'));
            rejectBtn?.addEventListener('click', () => this.handleConfirmation(message.confirmation_id, 'no'));
        }, 100);
    }

    async handleConfirmation(confirmationId, response) {
        try {
            // Disable buttons
            const confirmBtn = document.getElementById(`confirm-${confirmationId}`);
            const rejectBtn = document.getElementById(`reject-${confirmationId}`);
            if (confirmBtn) confirmBtn.disabled = true;
            if (rejectBtn) rejectBtn.disabled = true;

            // Show typing indicator
            this.showTypingIndicator();

            // Send confirmation response
            const apiResponse = await fetch(`${this.baseUrl}/api/v1/confirmations`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    confirmation_id: confirmationId,
                    user_response: response,
                    user_id: this.userId
                })
            });

            if (!apiResponse.ok) {
                throw new Error(`Failed to send confirmation: ${apiResponse.statusText}`);
            }

            const result = await apiResponse.json();

            // Hide typing indicator
            this.hideTypingIndicator();

            // Handle response
            if (result.response_text) {
                this.addMessage({
                    role: 'assistant',
                    content: result.response_text,
                    created_at: new Date().toISOString()
                });
            }

            // Reload sessions to reflect any changes
            await this.loadSessions();

        } catch (error) {
            console.error('Error handling confirmation:', error);
            this.hideTypingIndicator();
            this.showNotification('Failed to process confirmation', 'error');
        }
    }

    addClarificationMessage(message) {
        // P1 Compliance: System uncertain, needs user clarification
        message.is_clarification = true;
        this.messages.push(message);
        this.renderMessages();
        this.scrollToBottom();

        // Store thread_id for resume capability
        this.pendingClarificationThreadId = message.thread_id;

        // Focus on input for user to type clarification
        setTimeout(() => {
            const input = document.getElementById('message-input');
            if (input) {
                input.placeholder = 'Type your clarification...';
                input.focus();
            }
        }, 100);
    }

    async handleClarification(clarificationText) {
        if (!this.pendingClarificationThreadId) {
            // No pending clarification, send as normal message
            return false;
        }

        try {
            this.showTypingIndicator();

            // Send clarification to resume the flow
            const response = await fetch(`${this.baseUrl}/api/v1/chat/clarifications`, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    thread_id: this.pendingClarificationThreadId,
                    clarification: clarificationText,
                    user_id: this.userId,
                    session_id: this.currentSessionId
                })
            });

            if (!response.ok) {
                throw new Error(`Failed to submit clarification: ${response.statusText}`);
            }

            const result = await response.json();

            // Clear pending clarification
            this.pendingClarificationThreadId = null;

            // Reset input placeholder
            const input = document.getElementById('message-input');
            if (input) {
                input.placeholder = 'Type your message... (Shift+Enter for new line)';
            }

            this.hideTypingIndicator();

            // Handle response
            if (result.clarification_needed) {
                // Still needs more clarification
                this.addClarificationMessage({
                    role: 'assistant',
                    content: result.clarification_question,
                    thread_id: result.thread_id,
                    created_at: new Date().toISOString()
                });
            } else if (result.response) {
                this.addMessage({
                    role: 'assistant',
                    content: result.response,
                    created_at: new Date().toISOString()
                });
            }

            return true;

        } catch (error) {
            console.error('Error handling clarification:', error);
            this.hideTypingIndicator();
            this.showNotification('Failed to submit clarification', 'error');
            return false;
        }
    }

    renderMessages() {
        const container = document.getElementById('messages-container');
        const welcomeMessage = document.getElementById('welcome-message');

        if (!container) return;

        if (this.messages.length === 0) {
            if (welcomeMessage) welcomeMessage.classList.remove('hidden');
            return;
        }

        if (welcomeMessage) welcomeMessage.classList.add('hidden');

        const messagesHtml = this.messages.map(msg => this.renderMessage(msg)).join('');

        // Keep welcome and typing indicator, replace only messages
        const typingIndicator = document.getElementById('typing-indicator');
        const existingTyping = typingIndicator?.outerHTML || '';

        container.innerHTML = messagesHtml + existingTyping;

        this.scrollToBottom();
    }

    renderMessage(message) {
        const isUser = message.role === 'user';
        const avatar = isUser ? '👤' : '🤖';
        const bgColor = isUser
            ? 'bg-blue-600 text-white'
            : 'bg-white dark:bg-slate-800 text-gray-800 dark:text-white';
        const alignment = isUser ? 'justify-end' : 'justify-start';

        const content = isUser
            ? this.escapeHtml(message.content)
            : this.renderMarkdown(message.content);

        // Add confirmation buttons if this is a confirmation message
        const confirmationButtons = message.is_confirmation ? `
            <div class="flex gap-2 mt-4">
                <button id="confirm-${message.confirmation_id}"
                        class="flex-1 bg-green-600 hover:bg-green-700 text-white px-4 py-2 rounded-lg font-medium transition-all duration-200">
                    ✓ Confirm
                </button>
                <button id="reject-${message.confirmation_id}"
                        class="flex-1 bg-red-600 hover:bg-red-700 text-white px-4 py-2 rounded-lg font-medium transition-all duration-200">
                    ✗ Cancel
                </button>
            </div>
        ` : '';

        // Add clarification indicator if this is a clarification request
        const clarificationIndicator = message.is_clarification ? `
            <div class="mt-3 pt-3 border-t border-amber-200 dark:border-amber-800">
                <div class="flex items-center gap-2 text-amber-600 dark:text-amber-400 text-sm">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path>
                    </svg>
                    <span>Please respond to help me understand your request better</span>
                </div>
            </div>
        ` : '';

        return `
            <div class="flex ${alignment} gap-3 message-bubble max-w-3xl mx-auto">
                ${!isUser ? `
                    <div class="flex-shrink-0 w-8 h-8 rounded-full bg-blue-600 flex items-center justify-center text-white">
                        ${avatar}
                    </div>
                ` : ''}
                <div class="flex-1 ${bgColor} rounded-lg p-4 shadow-sm max-w-2xl">
                    <div class="message-content">
                        ${content}
                    </div>
                    ${confirmationButtons}
                    ${clarificationIndicator}
                    <div class="text-xs ${isUser ? 'text-blue-200' : 'text-gray-500 dark:text-gray-400'} mt-2">
                        ${this.formatTimeAgo(message.created_at)}
                    </div>
                </div>
                ${isUser ? `
                    <div class="flex-shrink-0 w-8 h-8 rounded-full bg-gray-600 flex items-center justify-center text-white">
                        ${avatar}
                    </div>
                ` : ''}
            </div>
        `;
    }

    clearMessages() {
        this.messages = [];
        const container = document.getElementById('messages-container');
        if (container) {
            container.innerHTML = `
                <div id="welcome-message" class="max-w-3xl mx-auto text-center py-16">
                    <h2 class="text-3xl font-bold text-gray-800 dark:text-white mb-4">
                        Code Analysis Agent
                    </h2>
                    <p class="text-gray-600 dark:text-gray-400">
                        Select a session or start a new analysis
                    </p>
                </div>
            `;
        }
    }

    // =========================================================================
    // UI Utilities
    // =========================================================================

    showTypingIndicator() {
        const indicator = document.getElementById('typing-indicator');
        if (indicator) {
            indicator.classList.remove('hidden');
            this.scrollToBottom();
        }
    }

    hideTypingIndicator() {
        const indicator = document.getElementById('typing-indicator');
        if (indicator) {
            indicator.classList.add('hidden');
        }
    }

    scrollToBottom(smooth = true) {
        const container = document.getElementById('messages-container');
        if (container) {
            container.scrollTo({
                top: container.scrollHeight,
                behavior: smooth ? 'smooth' : 'auto'
            });
        }
    }

    autoResizeTextarea(textarea) {
        textarea.style.height = 'auto';
        textarea.style.height = Math.min(textarea.scrollHeight, 200) + 'px';
    }

    updateConnectionStatus(status, color) {
        const statusText = document.getElementById('connection-status');
        const statusDot = document.getElementById('connection-status-dot');

        if (statusText) statusText.textContent = status;
        if (statusDot) {
            statusDot.className = `w-3 h-3 rounded-full`;
            const colors = {
                green: 'bg-green-500',
                red: 'bg-red-500',
                gray: 'bg-gray-400',
                yellow: 'bg-yellow-500'
            };
            statusDot.classList.add(colors[color] || 'bg-gray-400');
        }
    }

    showNotification(message, type = 'info') {
        const container = document.getElementById('notification-container');
        if (!container) return;

        const colors = {
            success: 'bg-green-500',
            error: 'bg-red-500',
            info: 'bg-blue-500',
            warning: 'bg-yellow-500'
        };

        const notification = document.createElement('div');
        notification.className = `notification ${colors[type]} text-white px-6 py-3 rounded-lg shadow-lg flex items-center gap-2`;
        notification.textContent = message;

        container.appendChild(notification);

        setTimeout(() => {
            notification.classList.add('hide');
            setTimeout(() => notification.remove(), 300);
        }, 3000);
    }

    // =========================================================================
    // Theme Management
    // =========================================================================

    loadTheme() {
        const theme = localStorage.getItem('theme') || 'light';
        if (theme === 'dark') {
            document.documentElement.classList.add('dark');
        }
    }

    toggleTheme() {
        const isDark = document.documentElement.classList.toggle('dark');
        localStorage.setItem('theme', isDark ? 'dark' : 'light');
    }

    // =========================================================================
    // Utilities
    // =========================================================================

    renderMarkdown(text) {
        if (typeof marked !== 'undefined') {
            marked.setOptions({
                highlight: function(code, lang) {
                    if (typeof hljs !== 'undefined' && lang && hljs.getLanguage(lang)) {
                        try {
                            return hljs.highlight(code, { language: lang }).value;
                        } catch (err) {}
                    }
                    return code;
                }
            });
            return marked.parse(text);
        }
        return this.escapeHtml(text).replace(/\n/g, '<br>');
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    formatTimeAgo(timestamp) {
        const now = new Date();
        const date = new Date(timestamp);
        const seconds = Math.floor((now - date) / 1000);

        if (seconds < 60) return 'just now';
        if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
        if (seconds < 86400) return `${Math.floor(seconds / 3600)} hr ago`;
        if (seconds < 604800) return `${Math.floor(seconds / 86400)} days ago`;

        return date.toLocaleDateString();
    }
}

// Initialize when DOM is ready
let chatClient; // Global reference for onclick handlers
document.addEventListener('DOMContentLoaded', () => {
    chatClient = new ChatClient();
    chatClient.init();
});
