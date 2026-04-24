# AGENTS.md - Working with Ink CLI Applications

## Overview

Ink is a React renderer for building command-line interfaces. Unlike web React, Ink renders to terminal output with strict constraints. This guide helps AI agents understand the unique considerations when writing React code for Ink applications.

## Key Differences from Web React

### 1. Terminal Rendering Environment
- **Fixed-width character grid**: Terminals use monospace fonts with fixed character cells
- **No pixel-based layouts**: Everything is measured in character columns and rows
- **Text-only output**: No images, videos, or rich media
- **Limited color support**: 16 colors, 256 colors, or RGB depending on terminal
- **No mouse interaction**: Primarily keyboard-driven (unless terminal supports mouse)

### 2. Layout System
- **Flexbox only**: All elements use `display: flex` by default
- **No CSS**: Styling is done through component props, not CSS classes
- **Character-based dimensions**: Width/height measured in characters, not pixels
- **No scrolling**: Content that exceeds terminal bounds is clipped or wrapped

## Text Handling and Overflow

### Text Wrapping
Text in Ink has specific wrapping behaviors controlled by the `wrap` prop:

```jsx
// Default wrapping - breaks at word boundaries
<Box width={10}>
  <Text>Hello World</Text>
</Box>
// Output: "Hello\nWorld"

// Hard wrapping - breaks anywhere to fill width
<Box width={7}>
  <Text wrap="hard">Hello World</Text>
</Box>
// Output: "Hello W\norld"

// Truncation options
<Box width={7}>
  <Text wrap="truncate">Hello World</Text>
</Box>
// Output: "Hello…"

<Box width={7}>
  <Text wrap="truncate-middle">Hello World</Text>
</Box>
// Output: "He…ld"
```

### Common Text Overflow Issues
❌ **Don't assume unlimited width:**
```jsx
// BAD - Text may overflow terminal width
<Text>This is a very long line that might exceed the terminal width and cause layout issues</Text>
```

✅ **Do constrain text appropriately:**
```jsx
// GOOD - Constrain width and handle wrapping
<Box width="80%">
  <Text wrap="wrap">This is a very long line that will wrap properly within the container</Text>
</Box>
```

## Layout Constraints and Best Practices

### 1. Terminal Width Awareness
Always consider terminal width limitations:

```jsx
import {useWindowSize} from 'ink';

const ResponsiveComponent = () => {
  const {columns} = useWindowSize();
  
  return (
    <Box width={Math.min(columns - 4, 80)}> {/* Leave margin, cap at 80 */}
      <Text>Content that adapts to terminal size</Text>
    </Box>
  );
};
```

### 2. Vertical Space Management
Terminal height is limited - avoid excessive vertical content:

❌ **Don't create unlimited vertical lists:**
```jsx
// BAD - Could exceed terminal height
{items.map(item => (
  <Box key={item.id} height={3}>
    <Text>{item.title}</Text>
  </Box>
))}
```

✅ **Do implement pagination or scrolling:**
```jsx
// GOOD - Paginate or limit visible items
const visibleItems = items.slice(currentPage * pageSize, (currentPage + 1) * pageSize);
return (
  <>
    {visibleItems.map(item => (
      <Box key={item.id}>
        <Text>{item.title}</Text>
      </Box>
    ))}
    <Text dimColor>Page {currentPage + 1} of {Math.ceil(items.length / pageSize)}</Text>
  </>
);
```

### 3. Flexbox Layout Patterns

**Horizontal layouts:**
```jsx
// Side-by-side content
<Box>
  <Box width="50%">
    <Text>Left panel</Text>
  </Box>
  <Box width="50%">
    <Text>Right panel</Text>
  </Box>
</Box>

// Label-value pairs
<Box>
  <Text>Status: </Text>
  <Box flexGrow={1}>
    <Text color="green">Running</Text>
  </Box>
</Box>
```

**Vertical layouts:**
```jsx
// Stacked content
<Box flexDirection="column">
  <Text>Header</Text>
  <Box flexGrow={1}>
    <Text>Main content</Text>
  </Box>
  <Text>Footer</Text>
</Box>
```

## Ink-Specific Components

### 1. Text Component
- **All text must be wrapped in `<Text>`**
- Only text nodes and nested `<Text>` components allowed inside
- No `<Box>` or other components inside `<Text>`

```jsx
// ✅ Correct
<Text color="green">Success: <Text bold>Operation completed</Text></Text>

// ❌ Incorrect
<Text>Status: <Box><Text>Running</Text></Box></Text>
```

### 2. Box Component
- Primary layout component (like `<div>` but with `display: flex`)
- Supports Flexbox properties, padding, margin, borders
- Use for all layout and positioning

### 3. Static Component
- For content that doesn't change after rendering
- Useful for logs, completed tasks, permanent output
- Renders above dynamic content

```jsx
<Static items={completedTasks}>
  {task => (
    <Box key={task.id}>
      <Text color="green">✓ {task.name}</Text>
    </Box>
  )}
</Static>
```

### 4. Spacer Component
- Flexible space that expands along the major axis
- Useful for pushing content to edges

```jsx
<Box>
  <Text>Left</Text>
  <Spacer />
  <Text>Right</Text>
</Box>
```

## Input and Interaction

### Keyboard Input
```jsx
import {useInput} from 'ink';

const InteractiveComponent = () => {
  useInput((input, key) => {
    if (input === 'q') {
      process.exit(0);
    }
    
    if (key.upArrow) {
      // Handle up arrow
    }
    
    if (key.return) {
      // Handle enter key
    }
  });
  
  return <Text>Press 'q' to quit</Text>;
};
```

### Focus Management
```jsx
import {useFocus} from 'ink';

const FocusableComponent = () => {
  const {isFocused} = useFocus();
  
  return (
    <Text color={isFocused ? 'blue' : 'white'}>
      {isFocused ? '> ' : '  '}Focusable item
    </Text>
  );
};
```

## Performance Considerations

### 1. Minimize Re-renders
Terminal rendering is expensive - avoid unnecessary updates:

```jsx
// Use React.memo for stable components
const StatusLine = React.memo(({status}) => (
  <Text color="blue">Status: {status}</Text>
));

// Debounce rapid updates
const [debouncedValue] = useDebounce(rapidlyChangingValue, 100);
```

### 2. Animation Considerations
```jsx
import {useAnimation} from 'ink';

const Spinner = () => {
  const {frame} = useAnimation({interval: 80}); // Not too fast
  const chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
  
  return <Text>{chars[frame % chars.length]}</Text>;
};
```

### 3. Control Frame Rate
```jsx
// Limit updates for better performance
render(<App />, {
  maxFps: 30, // Default is 30, lower for less CPU usage
});
```

## Common Pitfalls and Solutions

### 1. Text Overflow
❌ **Problem:** Text exceeds terminal width
```jsx
<Text>Very long text that might overflow the terminal width causing display issues</Text>
```

✅ **Solution:** Use width constraints and wrapping
```jsx
<Box width="100%">
  <Text wrap="wrap">Very long text that might overflow the terminal width causing display issues</Text>
</Box>
```

### 2. Nested Box Issues
❌ **Problem:** Unnecessary nesting causing layout issues
```jsx
<Box>
  <Box>
    <Box>
      <Text>Over-nested content</Text>
    </Box>
  </Box>
</Box>
```

✅ **Solution:** Flatten structure when possible
```jsx
<Box padding={1}>
  <Text>Properly structured content</Text>
</Box>
```

### 3. Color and Styling
❌ **Problem:** Assuming rich styling support
```jsx
<Text style={{fontSize: '16px', fontFamily: 'Arial'}}>Styled text</Text>
```

✅ **Solution:** Use Ink's supported styling props
```jsx
<Text color="blue" bold underline>Styled text</Text>
```

### 4. Dynamic Content Height
❌ **Problem:** Unlimited dynamic content
```jsx
{messages.map(msg => (
  <Text key={msg.id}>{msg.content}</Text>
))}
```

✅ **Solution:** Implement scrolling or pagination
```jsx
const visibleMessages = messages.slice(-maxVisible);
return (
  <Box flexDirection="column" height={maxVisible}>
    {visibleMessages.map(msg => (
      <Text key={msg.id}>{msg.content}</Text>
    ))}
  </Box>
);
```

## Testing Terminal UIs

### 1. Use ink-testing-library
```jsx
import {render} from 'ink-testing-library';

const {lastFrame, stdin} = render(<MyComponent />);

// Test output
expect(lastFrame()).toMatch(/Expected text/);

// Test input
stdin.write('q');
expect(lastFrame()).toMatch(/Quit message/);
```

### 2. Test Different Terminal Sizes
```jsx
// Test with different widths
const {lastFrame} = render(<MyComponent />, {columns: 40});
expect(lastFrame()).toMatch(/Wrapped content/);
```

## Accessibility Considerations

### Screen Reader Support
```jsx
// Provide meaningful labels
<Box aria-role="checkbox" aria-state={{checked: true}}>
  <Text>Accept terms</Text>
</Box>

// Use descriptive labels for progress indicators
<Box>
  <Box width="50%" backgroundColor="green" />
  <Text aria-label="Progress: 50%">50%</Text>
</Box>
```

## Best Practices Summary

1. **Always constrain content width** - Use `width` props or percentage widths
2. **Handle text wrapping explicitly** - Set appropriate `wrap` values
3. **Consider terminal size** - Use `useWindowSize()` for responsive layouts
4. **Minimize vertical content** - Implement pagination for long lists
5. **Use semantic structure** - Proper component hierarchy with `<Box>` and `<Text>`
6. **Test with different terminal sizes** - Ensure layouts work across screen sizes
7. **Optimize for performance** - Avoid unnecessary re-renders and high frame rates
8. **Provide keyboard navigation** - Implement proper focus management
9. **Consider accessibility** - Use ARIA labels where appropriate
10. **Handle edge cases** - Empty states, loading states, error conditions

## Example: Well-Structured Ink Component

```jsx
import React, {useState} from 'react';
import {Box, Text, useInput, useWindowSize, Spacer} from 'ink';

const TaskList = ({tasks}) => {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const {columns} = useWindowSize();
  
  useInput((input, key) => {
    if (key.upArrow && selectedIndex > 0) {
      setSelectedIndex(selectedIndex - 1);
    }
    if (key.downArrow && selectedIndex < tasks.length - 1) {
      setSelectedIndex(selectedIndex + 1);
    }
  });
  
  const maxWidth = Math.min(columns - 4, 80);
  
  return (
    <Box flexDirection="column" width={maxWidth}>
      <Box borderStyle="round" padding={1}>
        <Text bold>Task List ({tasks.length})</Text>
      </Box>
      
      <Box flexDirection="column" marginTop={1}>
        {tasks.map((task, index) => (
          <Box key={task.id} backgroundColor={index === selectedIndex ? 'blue' : undefined}>
            <Text color={task.completed ? 'green' : 'white'}>
              {task.completed ? '✓' : '○'} 
            </Text>
            <Text> </Text>
            <Box width="100%">
              <Text wrap="truncate">{task.title}</Text>
            </Box>
            <Spacer />
            <Text dimColor>{task.priority}</Text>
          </Box>
        ))}
      </Box>
      
      <Box marginTop={1}>
        <Text dimColor>Use ↑↓ to navigate</Text>
      </Box>
    </Box>
  );
};
```

This example demonstrates:
- Proper width constraints and responsive design
- Keyboard input handling
- Appropriate use of Ink components
- Text truncation for overflow handling
- Clear visual hierarchy and spacing
- Accessibility considerations with clear navigation hints