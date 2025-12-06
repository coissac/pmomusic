# PMOControl WebUI - Design Recommendations & Implementation Plan

## Executive Summary

Based on my analysis of the existing codebase, I'm providing comprehensive recommendations for implementing a Vue.js WebUI for PMOControl. The system already has:
- A complete REST API with OpenAPI documentation (`/api/control/*`)
- SSE endpoints for real-time updates (`/api/control/events/*`)
- Vue 3 + TypeScript + Vite setup
- Existing components (GenericMusicPlayer, UpnpExplorer, Cache Managers, LogView)

---

## Design Decisions & Recommendations

### 1. State Management: **Use Pinia**

**Recommendation: Pinia (Vue 3's official state management)**

**Rationale:**
- **Centralized real-time state**: Essential for managing SSE updates from multiple sources (renderers, media servers)
- **Multi-client synchronization**: Single source of truth for renderer states, volumes, playback positions
- **TypeScript native**: Better type inference than Vuex
- **DevTools integration**: Built-in debugging for SSE event flows
- **Composition API friendly**: Matches existing Vue 3 patterns in codebase
- **Performance**: Lightweight (~1KB), modular stores
- **Official Vue 3 recommendation**: Future-proof choice

**Store Architecture:**
```typescript
// stores/renderers.ts - Renderer state (SSE updates)
// stores/mediaServers.ts - Media server state (SSE updates)
// stores/playback.ts - Current playback session
// stores/ui.ts - UI state (selected renderer, view preferences)
```

**Benefits for your use case:**
- Handle 20+ concurrent clients with shared state
- Real-time SSE event synchronization across all views
- Easy to scale with multi-renderer, multi-server, multi-session architecture

---

### 2. UI Component Library: **Headless UI + Custom Components**

**Recommendation: Hybrid approach - Headless UI components + custom styling**

**Component Library: Shadcn-vue (Headless UI primitives)**

**Rationale:**
- **Lightweight & performant**: Only import what you need
- **Full style control**: Match "carte uniforme, responsive, colorée selon statut" spec exactly
- **TypeScript-first**: Perfect type safety
- **Accessibility built-in**: ARIA compliance out of the box
- **No theme lock-in**: Complete CSS freedom
- **Composable primitives**: Card, Dialog, Dropdown, Slider components

**Why NOT a full framework (Vuetify, Element Plus)?**
- Heavy bundle size (100-500KB vs ~10KB for headless)
- Theme customization overhead
- Your spec requires custom status-based coloring
- Performance critical with 20 concurrent clients

**Alternative if you prefer pre-styled:** PrimeVue
- Good performance
- Customizable themes
- Strong TypeScript support
- But: 150KB+ bundle size

**Custom Components to Build:**
- `RendererCard` - Status-colored cards for each renderer
- `TransportControls` - Play/Pause/Stop/Next buttons
- `VolumeControl` - Slider with mute toggle
- `QueueViewer` - Playlist display with drag-drop
- `MediaServerBrowser` - Container navigation

---

### 3. Existing Components: **Reorganize into Debug Section**

**Recommendation: Keep existing components, create new PMOControl home**

**Structure:**
```
/app (root) → PMOControl Dashboard (NEW)
/app/debug → Dropdown menu
  ├─ /logs → LogView
  ├─ /upnp → UpnpExplorer
  ├─ /covers-cache → CoverCacheManager
  ├─ /audio-cache → AudioCacheManager
  ├─ /api-dashboard → APIDashboard
  └─ /radio-paradise → RadioParadiseExplorer
```

**Rationale:**
- Existing components are valuable for development/debugging
- Don't break existing functionality
- PMOControl becomes primary interface as specified
- Debug tools remain accessible but not prominent
- Matches current App.vue dropdown pattern

**Home Screen (/) - PMOControl Dashboard:**
- Grid of renderer cards (status-colored)
- Active playback session viewer
- Quick controls (play/pause/volume)
- Media server browser panel

---

### 4. Responsive Design: **Mobile-first with 3 breakpoints**

**Recommendation: Follow existing 768px pattern + add tablet/desktop**

**Breakpoints:**
```css
/* Mobile: < 768px (existing pattern) */
- Single column layout
- Stacked renderer cards
- Bottom-fixed playback controls
- Collapsible media browser

/* Tablet: 768px - 1024px */
- Two column layout
- Grid of renderer cards (2 columns)
- Side panel for media browser
- Floating playback controls

/* Desktop: > 1024px */
- Three column layout
- Renderer cards grid (3-4 columns)
- Persistent media browser sidebar
- Always-visible playback controls
```

**Target Devices:**
- **Primary**: Desktop browsers (control station)
- **Secondary**: Tablets (remote control)
- **Tertiary**: Mobile phones (quick controls)

**Performance considerations:**
- Virtualized lists for 20+ renderers (use vue-virtual-scroller)
- Lazy load album art
- Throttle SSE position updates (max 1/sec per renderer)

---

### 5. Icons: **Lucide Icons (SVG library)**

**Recommendation: Lucide Icons (NOT emoji)**

**Rationale:**
- **Professional appearance**: Emojis inconsistent across platforms
- **Customizable**: Size, color, stroke width
- **Lightweight**: Tree-shakeable SVG imports (~1KB per icon)
- **Status coloring**: Icons can match card status colors
- **Accessibility**: Proper ARIA labels
- **Vue components**: `lucide-vue-next` package

**Icon mapping:**
```typescript
Play → PlayCircle
Pause → PauseCircle
Stop → StopCircle
Next → SkipForward
Volume → Volume2 / VolumeX (muted)
Renderer → Speaker / MonitorSpeaker
Server → Server / Database
Queue → ListMusic
```

**Alternative if you prefer minimal bundle:** Heroicons
- Smaller set (fewer icons)
- Tailwind CSS integration
- But: less comprehensive for music player needs

**Why NOT emoji:**
- Platform inconsistencies (iOS ≠ Android ≠ Windows)
- No color control
- Accessibility issues
- Unprofessional for production UI

---

## Technical Architecture

### Real-time SSE Integration

**SSE Event Handling:**
```typescript
// services/controlPointSSE.ts
class ControlPointSSE {
  private eventSource: EventSource
  private renderersStore: ReturnType<typeof useRenderersStore>

  connect() {
    this.eventSource = new EventSource('/api/control/events')

    this.eventSource.addEventListener('control', (e) => {
      const event = JSON.parse(e.data)

      if (event.category === 'renderer') {
        this.handleRendererEvent(event)
      } else if (event.category === 'media_server') {
        this.handleServerEvent(event)
      }
    })
  }

  handleRendererEvent(event: RendererEventPayload) {
    switch (event.type) {
      case 'state_changed':
        this.renderersStore.updateState(event.renderer_id, event.state)
        break
      case 'volume_changed':
        this.renderersStore.updateVolume(event.renderer_id, event.volume)
        break
      // ... handle all event types
    }
  }
}
```

**Store Integration:**
```typescript
// stores/renderers.ts
export const useRenderersStore = defineStore('renderers', () => {
  const renderers = ref<Map<string, RendererState>>(new Map())

  // SSE updates
  function updateState(id: string, state: string) {
    const renderer = renderers.value.get(id)
    if (renderer) {
      renderer.transport_state = state
    }
  }

  // REST API calls
  async function play(id: string) {
    await fetch(`/api/control/renderers/${id}/play`, { method: 'POST' })
    // SSE will update state automatically
  }

  return { renderers, updateState, play }
})
```

### Performance Optimizations

**For 20+ concurrent clients:**

1. **Throttle position updates**:
   ```typescript
   const throttledPositionUpdate = throttle((id, pos) => {
     store.updatePosition(id, pos)
   }, 1000) // Max 1 update/second
   ```

2. **Virtual scrolling** for renderer lists:
   ```bash
   npm install vue-virtual-scroller
   ```

3. **Lazy load album art**:
   ```vue
   <img :src="albumArt" loading="lazy" />
   ```

4. **Debounce volume sliders**:
   ```typescript
   const debouncedVolumeChange = debounce((id, vol) => {
     api.setVolume(id, vol)
   }, 300)
   ```

5. **Memoize computed properties**:
   ```typescript
   const activeRenderers = computed(() =>
     renderers.value.filter(r => r.online)
   )
   ```

---

## Implementation Roadmap

### Phase 1: Core Infrastructure (Week 1)
1. Install Pinia + configure stores
2. Install Lucide Icons
3. Create SSE service layer
4. Setup store structure (renderers, servers, playback, ui)
5. Connect SSE events to stores

### Phase 2: UI Components (Week 2)
5. Build RendererCard component (status-colored)
6. Build TransportControls component
7. Build VolumeControl component
8. Build QueueViewer component
9. Create responsive grid layouts

### Phase 3: Dashboard Assembly (Week 3)
10. Create PMOControl home view
11. Integrate all components
12. Add media server browser panel
13. Implement responsive breakpoints
14. Add loading states & error handling

### Phase 4: Polish & Testing (Week 4)
15. Test with 20+ concurrent clients
16. Performance profiling & optimization
17. Accessibility audit (ARIA, keyboard nav)
18. Cross-browser testing
19. Mobile/tablet testing
20. Documentation

---

## Dependencies to Install

```json
{
  "dependencies": {
    "pinia": "^2.2.8",
    "lucide-vue-next": "^0.470.0",
    "vue-virtual-scroller": "^2.0.0-beta.8"
  },
  "devDependencies": {
    // Already installed: vue, vue-router, typescript, vite
  }
}
```

**Total bundle size estimate:** +15KB gzipped (Pinia + Lucide + Virtual Scroller)

---

## Status-based Coloring Scheme

Based on "carte uniforme, responsive, colorée selon statut" spec:

```css
/* Renderer Card Status Colors */
.renderer-card.playing {
  border-color: #22c55e; /* green */
  background: linear-gradient(135deg, #22c55e10, transparent);
}

.renderer-card.paused {
  border-color: #f59e0b; /* amber */
  background: linear-gradient(135deg, #f59e0b10, transparent);
}

.renderer-card.stopped {
  border-color: #6b7280; /* gray */
  background: linear-gradient(135deg, #6b728010, transparent);
}

.renderer-card.offline {
  border-color: #ef4444; /* red */
  background: linear-gradient(135deg, #ef444410, transparent);
  opacity: 0.6;
}

.renderer-card.transitioning {
  border-color: #3b82f6; /* blue */
  background: linear-gradient(135deg, #3b82f610, transparent);
  animation: pulse 2s infinite;
}
```

---

## Answers to Your Specific Questions

### 1. State Management?
**Answer: Pinia** - Vue 3 official, perfect for SSE real-time updates, TypeScript native, lightweight

### 2. UI Component Library?
**Answer: Headless UI (Shadcn-vue) + Custom Components** - Full control over status-based styling, lightweight, no theme lock-in

### 3. Keep existing components?
**Answer: Yes, reorganize into Debug section** - Keep valuable dev tools, make PMOControl the new home screen

### 4. Responsive breakpoints?
**Answer: Mobile-first with 3 breakpoints** - <768px (mobile), 768-1024px (tablet), >1024px (desktop)

### 5. Icons?
**Answer: Lucide Icons (SVG library)** - Professional, customizable, status-colored, NOT emoji

---

## Risk Mitigation

**Potential challenges:**

1. **SSE connection management across tabs**
   - Solution: Use BroadcastChannel API for cross-tab sync
   - Fallback: LocalStorage events

2. **20+ renderers performance**
   - Solution: Virtual scrolling + throttled updates
   - Monitor: Chrome DevTools Performance profiler

3. **Network reliability (SSE reconnection)**
   - Solution: Exponential backoff reconnection
   - UI indicator for connection status

4. **Album art loading (CORS, 404s)**
   - Solution: Proxy through backend
   - Fallback: Default placeholder image

5. **Browser compatibility (SSE support)**
   - Chrome/Edge: Native support ✅
   - Firefox: Native support ✅
   - Safari: Native support ✅
   - IE11: Use EventSource polyfill

---

## Success Metrics

**Performance targets:**
- Initial load: <2s (FCP)
- SSE event latency: <100ms
- UI interaction: <16ms (60fps)
- Memory usage: <50MB with 20 renderers
- Bundle size: <250KB gzipped

**Functionality checklist:**
- [ ] Display all discovered renderers in real-time
- [ ] Show accurate playback state (play/pause/stop)
- [ ] Volume control works across all renderer types
- [ ] Queue display syncs with server
- [ ] Media server browsing functional
- [ ] Playlist attachment working
- [ ] Responsive on mobile/tablet/desktop
- [ ] Accessible (WCAG AA compliance)
- [ ] 20+ concurrent clients supported

---

## Next Steps

1. **Review & approve** this plan with stakeholders
2. **Clarify any ambiguities** in requirements
3. **Set up development environment** (install dependencies)
4. **Begin Phase 1** (Core Infrastructure)

Would you like me to proceed with implementation, or do you have questions about any of these recommendations?
