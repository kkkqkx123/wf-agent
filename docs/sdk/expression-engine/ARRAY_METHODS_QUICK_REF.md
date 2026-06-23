# Array Methods Quick Reference

## Syntax

```typescript
arrayPath.methodName('propertyName', value?)
```

## Methods Cheat Sheet

| Method | Returns | Description | Example |
|--------|---------|-------------|---------|
| `someEqual(prop, val)` | `boolean` | Any item matches? | `messages.someEqual('role', 'user')` |
| `someContains(prop, str)` | `boolean` | Any contains substring? | `messages.someContains('content', 'error')` |
| `everyEqual(prop, val)` | `boolean` | All items match? | `branches.everyEqual('status', 'COMPLETED')` |
| `everyHas(prop)` | `boolean` | All have property? | `messages.everyHas('content')` |
| `countWhere(prop, val)` | `number` | Count matching items | `messages.countWhere('role', 'user')` |
| `countWhereContains(prop, str)` | `number` | Count containing substring | `messages.countWhereContains('content', '!')` |
| `findEqual(prop, val)` | `object\|null` | First matching item | `messages.findEqual('role', 'system')` |
| `findContains(prop, str)` | `object\|null` | First containing substring | `messages.findContains('content', 'ERROR')` |
| `has(prop, val)` | `boolean` | Alias for someEqual | `messages.has('role', 'admin')` |
| `hasContains(prop, str)` | `boolean` | Alias for someContains | `messages.hasContains('content', 'warn')` |

## Enhanced `in` Operator

```typescript
// Check object properties against array
input.lastMessage in ['user', 'assistant', 'system']

// Checks: role, name, type, id, status, nodeType
```

## Common Patterns

### Error Detection
```typescript
input.messages.someContains('content', 'ERROR')
```

### Validation
```typescript
output.nodeOutput.everyEqual('status', 'COMPLETED')
```

### Counting
```typescript
input.messages.countWhere('role', 'user')
```

### Finding
```typescript
input.messages.findEqual('role', 'system')
```

## Edge Cases

| Scenario | some* | every* | count* | find* |
|----------|-------|--------|--------|-------|
| Empty array | `false` | `true` | `0` | `null` |
| Non-array | `false` | `false` | `0` | `null` |
| Missing property | Compares with `undefined` | Returns `false` | Counts `undefined` matches | Returns first or `null` |

## Limitations (Current)

❌ Cannot use in comparisons: `countWhere(...) > 5`  
❌ Cannot combine with &&: `someEqual(...) && everyEqual(...)`  
❌ Cannot use in ternary: `countWhere(...) > 10 ? 'a' : 'b'`  

✅ Use as standalone boolean conditions  
✅ Access numeric results via event data  

## Tips

- Use `some*` for existence checks (fast - short circuits)
- Use `every*` for validation
- Use `count*` for metrics
- Use `find*` to retrieve specific items
- Property names are case-sensitive
- Values can be strings, numbers, booleans, or null
