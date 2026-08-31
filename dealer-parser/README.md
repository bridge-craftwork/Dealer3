# dealer-parser

Parser for dealer constraint language, converting text constraints into an Abstract Syntax Tree (AST).

## Features

- **pest-based grammar**: Declarative PEG parser for easy maintenance
- **Parse once, evaluate many**: AST is `Clone + Send + Sync` for thread-safe sharing
- **Operator precedence**: Handles arithmetic, comparison, and logical operators correctly
- **Position-aware**: Understands bridge positions (north, south, east, west)
- **Built-in functions**: Supports hand evaluation functions (hcp, hearts, spades, etc.)

## Supported Syntax

### Functions
- `hcp(position)` - High Card Points
- `hearts(position)`, `spades(position)`, `diamonds(position)`, `clubs(position)` - Suit lengths
- `controls(position)` - Control count (A=2, K=1)

### Operators
- **Comparison**: `==`, `!=`, `<`, `<=`, `>`, `>=`
- **Logical**: `&&`, `||`
- **Arithmetic**: `+`, `-`, `*`, `/`, `%`

### Positions
- Full names: `north`, `south`, `east`, `west` (case-insensitive)
- Abbreviations: `n`, `s`, `e`, `w`

## Usage

```rust
use dealer_parser::parse;

// Parse a constraint
let ast = parse("hearts(north) >= 5 && hcp(south) <= 13").unwrap();

// AST is Clone, so you can share it across threads
let ast_clone = ast.clone();

// Use in evaluator (see dealer-eval crate)
```

## Example Constraints

```rust
// Simple comparison
parse("hcp(north) >= 15")

// Logical AND
parse("hearts(north) >= 5 && hcp(south) <= 13")

// Logical OR
parse("spades(north) >= 4 || hearts(north) >= 4")

// Arithmetic
parse("hcp(north) + hcp(south) >= 25")

// Complex expression
parse("(hearts(north) >= 5 && hcp(north) >= 12) || hcp(north) >= 20")
```

## AST Structure

The parser produces an AST with these types:

```rust
pub enum Expr {
    BinaryOp { op: BinaryOp, left: Box<Expr>, right: Box<Expr> },
    UnaryOp { op: UnaryOp, expr: Box<Expr> },
    FunctionCall { func: Function, arg: Box<Expr> },
    Literal(i32),
    Position(Position),
}
```

## Architecture

```
Input text
    ↓
[pest parser] (grammar.pest)
    ↓
pest parse tree
    ↓
[AST builder]
    ↓
AST (Clone + Send + Sync)
```

## Testing

```bash
cargo test -p dealer-parser
```

All core parsing functionality is tested:
- Simple comparisons
- Logical operators (AND, OR)
- Arithmetic expressions
- Position parsing
- Error handling

## Future Enhancements

- [ ] Negation operator (`!`)
- [ ] More built-in functions (shape, losers, winners, hascard)
- [ ] Variable definitions
- [ ] Custom user functions
- [ ] Better error messages with line/column info

## License

This project is released into the **public domain** under
[The Unlicense](../LICENSE).
