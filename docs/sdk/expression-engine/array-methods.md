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

// Use in comparisons (Phase 1.1 - Now Supported)
input.messages.countWhere('role', 'user') > 5
input.messages.countWhere('role', 'assistant') >= 2
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

All major limitations from earlier versions have been addressed. The following features are now fully supported:

✅ **Comparison operators with array methods** (Phase 1.1):
```typescript
input.messages.countWhere('role', 'user') > 5
output.results.avg('score') >= 80
```

✅ **Logical operator combinations** (Phase 2.1):
```typescript
input.messages.someEqual('role', 'user') && input.messages.countWhere('role', 'assistant') >= 2
```

✅ **Ternary expressions** (Phase 2.2):
```typescript
input.messages.countWhere('role', 'user') > 10 ? 'long' : 'short'
```

✅ **Nested property access** (Phase 1.3):
```typescript
input.messages.someEqual('metadata.tags.0', 'important')
```

### New Advanced Features (Phase 3)

#### Aggregation Functions (Phase 3.1)

##### `sum(propertyName)`
Returns the sum of all numeric property values.

```typescript
// Sum all scores
input.results.sum('score')
```

##### `avg(propertyName)`
Returns the average of all numeric property values.

```toml
[[nodes.hooks]]
hookType = "AFTER_EXECUTE"
condition = { expression = "output.results.avg('score') > 80" }
eventName = "high.average.score"
```

##### `min(propertyName)` / `max(propertyName)`
Returns the minimum or maximum numeric property value.

```toml
[[nodes.hooks]]
hookType = "AFTER_EXECUTE"
condition = { expression = "output.results.max('duration') < 5000" }
eventName = "fast.execution"
```

#### Comparison-based Filters (Phase 3.2)

##### `someGreaterThan(prop, value)` / `someLessThan(prop, value)`
Returns true if any item's property is greater/less than the value.

```typescript
input.scores.someGreaterThan('value', 50)
input.scores.someLessThan('value', 10)
```

##### `everyGreaterThan(prop, value)` / `everyLessThan(prop, value)`
Returns true if all items' properties are greater/less than the value.

```typescript
input.scores.everyGreaterThan('value', 5)
input.scores.everyLessThan('value', 150)
```

#### Array Transformation Methods (Phase 3.4)

##### `map(propertyName)`
Extracts property values into a new array.

```typescript
// Get all user names
input.users.map('name')  // ['Alice', 'Bob', 'Charlie']
```

##### `distinct(propertyName)`
Returns unique values of a property.

```typescript
// Get unique roles
input.users.distinct('role')  // ['admin', 'user', 'moderator']
```

##### `first()` / `last()`
Returns the first or last element of the array.

```typescript
input.users.first()   // First user object
input.users.last()    // Last user object
```

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

Potential improvements for future versions:

1. Generic filter expressions with custom predicates (requires advanced parser)
2. Additional string manipulation methods
3. Date/time comparison functions
4. Regular expression support in contains checks
