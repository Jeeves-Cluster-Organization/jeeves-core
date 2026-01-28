// Package kernel provides process lifecycle management.
//
// Implements the kernel's process scheduling:
//   - Process creation (Submit)
//   - State transitions (Schedule, Wait, Block, Resume, Terminate)
//   - Priority scheduling (GetNextRunnable)
package kernel

import (
	"container/heap"
	"fmt"
	"sync"
	"time"
)

// =============================================================================
// Valid State Transitions
// =============================================================================

// validTransitions defines allowed state transitions.
var validTransitions = map[ProcessState]map[ProcessState]bool{
	ProcessStateNew: {
		ProcessStateReady:      true,
		ProcessStateTerminated: true,
	},
	ProcessStateReady: {
		ProcessStateRunning:    true,
		ProcessStateTerminated: true,
	},
	ProcessStateRunning: {
		ProcessStateReady:      true, // Preempted
		ProcessStateWaiting:    true, // Waiting for I/O (clarification)
		ProcessStateBlocked:    true, // Resource exhausted
		ProcessStateTerminated: true,
	},
	ProcessStateWaiting: {
		ProcessStateReady:      true,
		ProcessStateTerminated: true,
	},
	ProcessStateBlocked: {
		ProcessStateReady:      true,
		ProcessStateTerminated: true,
	},
	ProcessStateTerminated: {
		ProcessStateZombie: true,
	},
	ProcessStateZombie: {}, // Terminal state
}

// IsValidTransition checks if a state transition is valid.
func IsValidTransition(from, to ProcessState) bool {
	if targets, ok := validTransitions[from]; ok {
		return targets[to]
	}
	return false
}

// =============================================================================
// Priority Queue (heap)
// =============================================================================

// priorityItem represents an item in the priority queue.
type priorityItem struct {
	pid       string
	priority  int       // Lower = higher priority
	createdAt time.Time // For FIFO within same priority
	index     int       // Heap index
}

// priorityQueue implements heap.Interface.
type priorityQueue []*priorityItem

func (pq priorityQueue) Len() int { return len(pq) }

func (pq priorityQueue) Less(i, j int) bool {
	// Lower priority value = higher priority
	if pq[i].priority != pq[j].priority {
		return pq[i].priority < pq[j].priority
	}
	// FIFO for same priority
	return pq[i].createdAt.Before(pq[j].createdAt)
}

func (pq priorityQueue) Swap(i, j int) {
	pq[i], pq[j] = pq[j], pq[i]
	pq[i].index = i
	pq[j].index = j
}

func (pq *priorityQueue) Push(x any) {
	n := len(*pq)
	item := x.(*priorityItem)
	item.index = n
	*pq = append(*pq, item)
}

func (pq *priorityQueue) Pop() any {
	old := *pq
	n := len(old)
	item := old[n-1]
	old[n-1] = nil  // avoid memory leak
	item.index = -1 // for safety
	*pq = old[0 : n-1]
	return item
}

// priorityValue returns the heap priority value (lower = higher priority).
func priorityValue(p SchedulingPriority) int {
	switch p {
	case PriorityRealtime:
		return 0
	case PriorityHigh:
		return 1
	case PriorityNormal:
		return 2
	case PriorityLow:
		return 3
	case PriorityIdle:
		return 4
	default:
		return 2
	}
}

// =============================================================================
// Lifecycle Manager
// =============================================================================

// LifecycleManager manages process lifecycles - the kernel scheduler.
// Thread-safe implementation using locks and a priority heap.
type LifecycleManager struct {
	defaultQuota *ResourceQuota
	processes    map[string]*ProcessControlBlock
	readyQueue   priorityQueue
	mu           sync.RWMutex
}

// NewLifecycleManager creates a new lifecycle manager.
func NewLifecycleManager(defaultQuota *ResourceQuota) *LifecycleManager {
	if defaultQuota == nil {
		defaultQuota = DefaultQuota()
	}
	lm := &LifecycleManager{
		defaultQuota: defaultQuota,
		processes:    make(map[string]*ProcessControlBlock),
		readyQueue:   make(priorityQueue, 0),
	}
	heap.Init(&lm.readyQueue)
	return lm
}

// Submit creates a new process for the given envelope.
// Returns the PCB in NEW state.
func (lm *LifecycleManager) Submit(pid, requestID, userID, sessionID string, priority SchedulingPriority, quota *ResourceQuota) (*ProcessControlBlock, error) {
	lm.mu.Lock()
	defer lm.mu.Unlock()

	// Check for duplicate
	if existing, ok := lm.processes[pid]; ok {
		return existing, nil
	}

	// Create PCB
	if quota == nil {
		quota = lm.defaultQuota
	}

	pcb := &ProcessControlBlock{
		PID:       pid,
		RequestID: requestID,
		UserID:    userID,
		SessionID: sessionID,
		State:     ProcessStateNew,
		Priority:  priority,
		Quota:     quota,
		Usage:     &ResourceUsage{},
		CreatedAt: time.Now().UTC(),
		ChildPIDs: []string{},
	}

	lm.processes[pid] = pcb
	return pcb, nil
}

// Schedule transitions a process from NEW to READY and adds to ready queue.
func (lm *LifecycleManager) Schedule(pid string) error {
	lm.mu.Lock()
	defer lm.mu.Unlock()

	pcb, ok := lm.processes[pid]
	if !ok {
		return fmt.Errorf("unknown pid: %s", pid)
	}

	if pcb.State != ProcessStateNew {
		return fmt.Errorf("cannot schedule pid %s: state is %s, expected new", pid, pcb.State)
	}

	// Transition to READY
	pcb.State = ProcessStateReady

	// Add to ready queue
	item := &priorityItem{
		pid:       pid,
		priority:  priorityValue(pcb.Priority),
		createdAt: pcb.CreatedAt,
	}
	heap.Push(&lm.readyQueue, item)

	return nil
}

// GetNextRunnable returns the next process to run.
// Transitions the process from READY to RUNNING.
func (lm *LifecycleManager) GetNextRunnable() *ProcessControlBlock {
	lm.mu.Lock()
	defer lm.mu.Unlock()

	for lm.readyQueue.Len() > 0 {
		item := heap.Pop(&lm.readyQueue).(*priorityItem)

		pcb, ok := lm.processes[item.pid]
		if !ok {
			continue // Process was removed
		}

		if pcb.State != ProcessStateReady {
			continue // State changed since queuing
		}

		// Transition to RUNNING
		now := time.Now().UTC()
		pcb.State = ProcessStateRunning
		if pcb.StartedAt == nil {
			pcb.StartedAt = &now
		}
		pcb.LastScheduledAt = &now

		return pcb
	}

	return nil
}

// TransitionState transitions a process to a new state.
func (lm *LifecycleManager) TransitionState(pid string, newState ProcessState, reason string) error {
	lm.mu.Lock()
	defer lm.mu.Unlock()

	pcb, ok := lm.processes[pid]
	if !ok {
		return fmt.Errorf("unknown pid: %s", pid)
	}

	oldState := pcb.State

	// Validate transition
	if !IsValidTransition(oldState, newState) {
		return fmt.Errorf("invalid transition from %s to %s for pid %s", oldState, newState, pid)
	}

	// Perform transition
	pcb.State = newState

	// Handle specific transitions
	switch newState {
	case ProcessStateReady:
		// Re-add to ready queue
		item := &priorityItem{
			pid:       pid,
			priority:  priorityValue(pcb.Priority),
			createdAt: time.Now().UTC(),
		}
		heap.Push(&lm.readyQueue, item)

	case ProcessStateTerminated:
		now := time.Now().UTC()
		pcb.CompletedAt = &now
		if pcb.StartedAt != nil {
			pcb.Usage.ElapsedSeconds = now.Sub(*pcb.StartedAt).Seconds()
		}
	}

	return nil
}

// GetProcess returns a process by ID.
func (lm *LifecycleManager) GetProcess(pid string) *ProcessControlBlock {
	lm.mu.RLock()
	defer lm.mu.RUnlock()
	return lm.processes[pid]
}

// ListProcesses returns processes matching criteria.
func (lm *LifecycleManager) ListProcesses(state *ProcessState, userID string) []*ProcessControlBlock {
	lm.mu.RLock()
	defer lm.mu.RUnlock()

	var result []*ProcessControlBlock
	for _, pcb := range lm.processes {
		if state != nil && pcb.State != *state {
			continue
		}
		if userID != "" && pcb.UserID != userID {
			continue
		}
		result = append(result, pcb)
	}
	return result
}

// Terminate terminates a process.
func (lm *LifecycleManager) Terminate(pid, reason string, force bool) error {
	lm.mu.Lock()
	defer lm.mu.Unlock()

	pcb, ok := lm.processes[pid]
	if !ok {
		return fmt.Errorf("unknown pid: %s", pid)
	}

	if pcb.IsTerminated() {
		return nil // Already terminated
	}

	// Check if forcible
	if pcb.State == ProcessStateRunning && !force {
		return fmt.Errorf("cannot terminate running process %s without force", pid)
	}

	// Terminate
	now := time.Now().UTC()
	pcb.State = ProcessStateTerminated
	pcb.CompletedAt = &now
	if pcb.StartedAt != nil {
		pcb.Usage.ElapsedSeconds = now.Sub(*pcb.StartedAt).Seconds()
	}

	return nil
}

// Cleanup removes a terminated process from the process table.
func (lm *LifecycleManager) Cleanup(pid string) error {
	lm.mu.Lock()
	defer lm.mu.Unlock()

	pcb, ok := lm.processes[pid]
	if !ok {
		return fmt.Errorf("unknown pid: %s", pid)
	}

	if !pcb.IsTerminated() {
		return fmt.Errorf("cannot cleanup active process %s (state: %s)", pid, pcb.State)
	}

	delete(lm.processes, pid)
	return nil
}

// GetQueueDepth returns the number of processes in the ready queue.
func (lm *LifecycleManager) GetQueueDepth() int {
	lm.mu.RLock()
	defer lm.mu.RUnlock()
	return lm.readyQueue.Len()
}

// GetProcessCount returns the count of processes by state.
func (lm *LifecycleManager) GetProcessCount() map[ProcessState]int {
	lm.mu.RLock()
	defer lm.mu.RUnlock()

	counts := make(map[ProcessState]int)
	for _, pcb := range lm.processes {
		counts[pcb.State]++
	}
	return counts
}

// GetTotalProcesses returns the total number of processes.
func (lm *LifecycleManager) GetTotalProcesses() int {
	lm.mu.RLock()
	defer lm.mu.RUnlock()
	return len(lm.processes)
}
