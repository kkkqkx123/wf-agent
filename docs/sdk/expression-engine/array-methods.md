# Array Methods in Expression Engine

## Overview

The expression engine now supports powerful array query methods for working with arrays of objects. These methods enable complex conditional logic in hook configurations and workflow conditions.

## Available Methods

### Query Methods

#### `someEqual(propertyName, value)`
Returns `true` if **any** item in the array has the specified property equal to the value.

```typescript
// Check if any message has role 'user'
input.messages.someEqual('role', 'user')
```

#### `someContains(propertyName, substring)`
Returns `true` if **any** item's property contains the specified substring.

```typescript
// Check if any message content contains 'error'
input.messages.someContains('content', 'error')
```

#### `everyEqual(propertyName, value)`
Returns `true` if **all** items in the array have the specified property equal to the value.

```typescript
// Check if all fork branches completed
input.forkResults.everyEqual('status', 'COMPLETED')
```

#### `everyHas(propertyName)`
Returns `true` if **all** items have the specified property (non-null).

```typescript
// Check if all messages have content
input.messages.everyHas('content')
```

### Count Methods

#### `countWhere(propertyName, value)`
Returns the **count** of items where the property equals the value.

```typescript
// Count user messages
input.messages.countWhere('role', 'user')

// Use in conditions (requires future parser enhancement)
// input.messages.countWhere('role', 'user') > 5
```

#### `countWhereContains(propertyName, substring)`
Returns the **count** of items where the property contains the substring.

```typescript
// Count messages containing '!'
input.messages.countWhereContains('content', '!')
```

### Find Methods

#### `findEqual(propertyName, value)`
Returns the **first** item where the property equals the value, or `null` if not found.

```typescript
// Find first user message
input.messages.findEqual('role', 'user')
```

#### `findContains(propertyName, substring)`
Returns the **first** item where the property contains the substring, or `null` if not found.

```typescript
// Find first error message
input.messages.findContains('content', 'ERROR')
```

### Existence Methods

#### `has(propertyName, value)`
Alias for `someEqual`. Returns `true` if any item matches.

```typescript
input.messages.has('role', 'system')
```

#### `hasContains(propertyName, substring)`
Alias for `someContains`. Returns `true` if any item contains the substring.

```typescript
input.messages.hasContains('content', 'warning')
```

## Usage Examples

### Example 1: Error Detection

```toml
[[nodes.hooks]]
hookType = "AFTER_EXECUTE"
condition = { expression = "input.messages.someContains('content', 'ERROR')" }
eventName = "error.detected"
```

### Example 2: Verify All Branches Completed

```toml
[[nodes.hooks]]
hookType = "AFTER_EXECUTE"
condition = { expression = "output.nodeOutput.everyEqual('status', 'COMPLETED')" }
eventName = "all.branches.completed"
```

### Example 3: Monitor Conversation Length

```toml
[[nodes.hooks]]
hookType = "BEFORE_ITERATION"
condition = { expression = "input.messages.countWhere('role', 'user')" }
eventName = "conversation.length.check"
# Note: The count value will be available in the event data
```

### Example 4: Find Specific Messages

```toml
[[nodes.hooks]]
hookType = "AFTER_EXECUTE"
condition = { expression = "input.messages.findEqual('role', 'system') != null" }
eventName = "system.message.found"
```

## Edge Cases

### Empty Arrays

- `some*` methods return `false`
- `every*` methods return `true` (vacuously true)
- `count*` methods return `0`
- `find*` methods return `null`

### Non-Array Values

If the path doesn't resolve to an array, all methods return `false` and log a warning.

### Missing Properties

If an item doesn't have the specified property:
- Equality checks (`*Equal`) compare with `undefined`
- Contains checks (`*Contains`) return `false`
- Existence checks (`everyHas`) return `false` for that item

## Enhanced `in` Operator

The `in` operator now supports checking object properties against arrays:

```typescript
// Check if lastMessage role is in valid roles
input.lastMessage in ['user', 'assistant', 'system']

// Equivalent to checking if any common property (role, name, type, id, status) matches
```

This works with these common properties: `role`, `name`, `type`, `id`, `status`, `nodeType`.

## Limitations (Current Version)

The following features require enhanced parser support and are planned for future iterations:

1. **Direct comparisons with array method results**:
   ```typescript
   // Not yet supported (will be added in future)
   input.messages.countWhere('role', 'user') > 5
   ```

2. **Combining array methods with logical operators**:
   ```typescript
   // Not yet supported (will be added in future)
   input.messages.someEqual('role', 'user') && input.messages.countWhere('role', 'assistant') >= 2
   ```

3. **Array methods in ternary conditions**:
   ```typescript
   // Not yet supported (will be added in future)
   input.messages.countWhere('role', 'user') > 10 ? 'long' : 'short'
   ```

### Workarounds

For now, you can:
- Use array methods as standalone boolean conditions
- Use count methods to get numeric values (available in event data)
- Combine simple conditions with logical operators

## Best Practices

1. **Use specific property names**: Always use the exact property name from your data structure
2. **Handle edge cases**: Consider what should happen with empty arrays
3. **Test with real data**: Verify your expressions work with actual data structures
4. **Use appropriate methods**: 
   - Use `some*` for existence checks
   - Use `every*` for validation
   - Use `count*` for metrics
   - Use `find*` for retrieval

## Future Enhancements

Planned improvements for future versions:

1. Support for arithmetic comparisons with count methods
2. Full integration with logical operators (&&, ||)
3. Support in ternary expressions
4. Additional aggregation functions (sum, avg, min, max)
5. Nested property access (e.g., `messages[*].metadata.tags`)
