# Edge Case Categories — Full Reference

This document provides an exhaustive checklist for each edge case category. Use it when the SKILL.md categories need deeper investigation.

## Empty & Missing States

### Data states
- [ ] Empty array / collection (0 items)
- [ ] Single item in collection
- [ ] Null or undefined response from API
- [ ] API returns success but with empty body
- [ ] Missing optional fields in response object
- [ ] First-time user with no historical data
- [ ] Deleted data that's still referenced (dangling references)

### UI states
- [ ] Empty state component exists and is meaningful (not just blank)
- [ ] Empty state has a call-to-action (not a dead end)
- [ ] Skeleton/placeholder shown while determining if data exists
- [ ] Search with no results shows helpful message

## Error & Failure States

### Network errors
- [ ] Complete network failure (offline)
- [ ] Timeout (slow response > 10s)
- [ ] Intermittent connectivity (request starts, connection drops)

### API errors
- [ ] 400 Bad Request — validation errors shown to user
- [ ] 401 Unauthorized — redirect to login
- [ ] 403 Forbidden — show access denied, not a broken page
- [ ] 404 Not Found — resource was deleted or never existed
- [ ] 409 Conflict — concurrent edit by another user
- [ ] 422 Unprocessable Entity — semantic validation failure
- [ ] 429 Too Many Requests — rate limiting
- [ ] 500 Internal Server Error — generic fallback error UI
- [ ] 503 Service Unavailable — maintenance mode

### Client errors
- [ ] JavaScript runtime errors caught by error boundary
- [ ] Failed to parse JSON response
- [ ] LocalStorage/SessionStorage full or unavailable
- [ ] Third-party script fails to load (analytics, fonts, CDN)

### Recovery
- [ ] Retry mechanism for transient failures
- [ ] User can dismiss error and try again
- [ ] Error state doesn't block the entire page
- [ ] Partial failure (some items load, some don't)

## Loading & Async States

### Timing
- [ ] Loading spinner/skeleton for operations > 300ms
- [ ] Optimistic UI for instant-feel interactions
- [ ] Progress indicator for multi-step operations
- [ ] Timeout handling for long-running operations

### Race conditions
- [ ] Double-click on submit button
- [ ] Rapid toggle on/off (debouncing)
- [ ] Navigation during pending request (abort controller)
- [ ] Multiple overlapping search queries (only use latest result)
- [ ] Stale data after background tab becomes active again

### State management
- [ ] Loading state resets properly after error
- [ ] Success state shown after async completion
- [ ] Form data preserved if submission fails

## Boundary & Overflow

### Text
- [ ] 0 characters (empty string)
- [ ] 1 character
- [ ] Maximum allowed length
- [ ] 10x maximum length (what if validation fails?)
- [ ] Multi-line text in single-line display
- [ ] Text with only whitespace
- [ ] Very long single word (no natural break point)

### Numbers
- [ ] 0
- [ ] Negative numbers
- [ ] Decimal numbers (0.1 + 0.2 !== 0.3)
- [ ] Very large numbers (display formatting)
- [ ] NaN or Infinity from calculations

### Collections
- [ ] 0 items
- [ ] 1 item (singular vs. plural labels)
- [ ] Exactly at page size boundary (e.g., 20/20)
- [ ] Page size + 1 (triggers pagination)
- [ ] Thousands of items (virtual scrolling needed?)
- [ ] Items added/removed while user is viewing list

### Layout
- [ ] Content wider than container
- [ ] Content taller than viewport
- [ ] Image fails to load (broken image icon vs. fallback)
- [ ] Dynamic content pushes layout (CLS)

## User Input Variations

### Text input
- [ ] Paste from clipboard (plain text)
- [ ] Paste from rich text source (Word, Google Docs)
- [ ] Paste HTML/markdown
- [ ] Emoji characters (multi-byte)
- [ ] RTL languages (Arabic, Hebrew)
- [ ] CJK characters (Chinese, Japanese, Korean)
- [ ] Mathematical symbols, currency symbols
- [ ] Control characters (tab, newline)
- [ ] Zero-width characters (invisible but present)
- [ ] Script injection attempts (`<script>`, `onclick=`)

### Interaction patterns
- [ ] Keyboard-only flow (Tab, Shift+Tab, Enter, Escape, Space)
- [ ] Mouse + keyboard switching mid-flow
- [ ] Touch on desktop (Surface, iPad with keyboard)
- [ ] Drag and drop (if applicable)
- [ ] Right-click / context menu
- [ ] Browser autofill vs. manual entry

## Navigation & State

### Browser behavior
- [ ] Back button preserves state
- [ ] Forward button works after going back
- [ ] Refresh preserves or gracefully resets state
- [ ] Deep link (sharing URL) loads correct state
- [ ] Bookmark + return later
- [ ] Open in new tab

### Multi-tab / multi-window
- [ ] Same flow open in two tabs
- [ ] Data modified in one tab while other is stale
- [ ] Session expired in background tab

### Route changes
- [ ] URL parameters validated (malformed values)
- [ ] Missing required URL parameters
- [ ] Navigating to a resource that was deleted
- [ ] Hash/fragment navigation

## Permissions & Access Control

### Authorization
- [ ] UI hides/disables actions user can't perform
- [ ] Server validates permissions (not just UI hiding)
- [ ] Permission change while user has page open
- [ ] Role escalation — user's role changes mid-session

### Resource lifecycle
- [ ] Resource deleted while user is editing it
- [ ] Resource moved while user has it open
- [ ] Concurrent edits by multiple users
- [ ] Optimistic locking / last-write-wins handling

## Responsive & Accessibility

### Responsive
- [ ] Mobile (320px - 480px)
- [ ] Small tablet (481px - 768px)
- [ ] Large tablet / small desktop (769px - 1024px)
- [ ] Desktop (1025px - 1440px)
- [ ] Large desktop (1441px+)
- [ ] Portrait vs. landscape orientation

### Accessibility
- [ ] All interactive elements focusable via keyboard
- [ ] Focus order matches visual order
- [ ] Focus trap in modals/dialogs
- [ ] Focus returns to trigger element when dialog closes
- [ ] ARIA labels on icon-only buttons
- [ ] ARIA live regions for dynamic content updates
- [ ] Color contrast ratio >= 4.5:1 (text) / 3:1 (large text)
- [ ] Not relying solely on color to convey information
- [ ] Reduced motion respected (prefers-reduced-motion)
- [ ] Screen reader announces state changes
- [ ] Skip navigation link present (if applicable)

### Touch
- [ ] Touch targets >= 44x44px
- [ ] No hover-only interactions (tooltips need tap alternative)
- [ ] Swipe gestures have button alternatives
- [ ] No tiny close buttons on mobile
