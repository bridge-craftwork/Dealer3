use crate::ast::*;
use dealer_core::Position;
use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct ConstraintParser;

/// Parse error type
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Parse error: {}", self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<pest::error::Error<Rule>> for ParseError {
    fn from(err: pest::error::Error<Rule>) -> Self {
        ParseError {
            message: err.to_string(),
        }
    }
}

/// Parse a constraint string into an AST
pub fn parse(input: &str) -> Result<Expr, ParseError> {
    let pairs = ConstraintParser::parse(Rule::constraint, input)?;

    // Get the first pair (should be the constraint rule)
    let pair = pairs.into_iter().next().ok_or_else(|| ParseError {
        message: "Empty input".to_string(),
    })?;

    build_ast(pair.into_inner().next().unwrap())
}

/// Parse a program (potentially multi-statement) into a Program AST
pub fn parse_program(input: &str) -> Result<Program, ParseError> {
    let pairs = ConstraintParser::parse(Rule::program, input)?;

    let mut statements = Vec::new();

    for pair in pairs {
        if pair.as_rule() == Rule::EOI {
            continue;
        }

        for statement_pair in pair.into_inner() {
            if statement_pair.as_rule() == Rule::dealer_statement {
                statements.push(build_statement(statement_pair)?);
            }
        }
    }

    Ok(Program { statements })
}

/// Build a statement from pest parse tree
fn build_statement(pair: Pair<Rule>) -> Result<Statement, ParseError> {
    let inner = pair.into_inner().next().unwrap();

    match inner.as_rule() {
        Rule::condition_stmt => {
            let expr = build_ast(inner.into_inner().next().unwrap())?;
            Ok(Statement::Condition(expr))
        }
        Rule::produce_stmt => {
            let literal = inner.into_inner().next().unwrap();
            let value = literal.as_str().parse::<usize>().map_err(|_| ParseError {
                message: format!("Invalid produce count: {}", literal.as_str()),
            })?;
            Ok(Statement::Produce(value))
        }
        Rule::generate_stmt => {
            let literal = inner.into_inner().next().unwrap();
            let value = literal.as_str().parse::<usize>().map_err(|_| ParseError {
                message: format!("Invalid generate count: {}", literal.as_str()),
            })?;
            Ok(Statement::Generate(value))
        }
        Rule::action_stmt => {
            let mut averages = Vec::new();
            let mut frequencies = Vec::new();
            let mut format = None;

            // Parse comma-separated action components
            for component in inner.into_inner() {
                match component.as_rule() {
                    Rule::action_component => {
                        let comp_inner = component.into_inner().next().unwrap();
                        match comp_inner.as_rule() {
                            Rule::average_spec => {
                                let mut parts = comp_inner.into_inner();
                                let first = parts.next().unwrap();

                                // Check if first element is a string literal (label) or expression
                                let (label, expr_pair) = if first.as_rule() == Rule::string_literal
                                {
                                    // Has label - strip quotes
                                    let label_str = first.as_str();
                                    let label = label_str[1..label_str.len() - 1].to_string();
                                    (Some(label), parts.next().unwrap())
                                } else {
                                    // No label - first element is the expression
                                    (None, first)
                                };

                                let expr = build_ast(expr_pair)?;
                                averages.push(AverageSpec { label, expr });
                            }
                            Rule::frequency_spec => {
                                let mut parts = comp_inner.into_inner();
                                let first = parts.next().unwrap();

                                // Check if first element is a string literal (label) or expression
                                let (label, expr_pair) = if first.as_rule() == Rule::string_literal
                                {
                                    // Has label - strip quotes
                                    let label_str = first.as_str();
                                    let label = label_str[1..label_str.len() - 1].to_string();
                                    (Some(label), parts.next().unwrap())
                                } else {
                                    // No label - first element is the expression
                                    (None, first)
                                };

                                let expr = build_ast(expr_pair)?;

                                // Check for optional range (min max)
                                let range = if let Some(min_pair) = parts.next() {
                                    let min = min_pair.as_str().parse::<i32>().map_err(|_| {
                                        ParseError {
                                            message: format!(
                                                "Invalid frequency range min: {}",
                                                min_pair.as_str()
                                            ),
                                        }
                                    })?;
                                    let max =
                                        parts.next().unwrap().as_str().parse::<i32>().map_err(
                                            |_| ParseError {
                                                message: "Invalid frequency range max".to_string(),
                                            },
                                        )?;
                                    Some((min, max))
                                } else {
                                    None
                                };

                                frequencies.push(FrequencySpec { label, expr, range });
                            }
                            Rule::action_type => {
                                let action_type = ActionType::parse(comp_inner.as_str())
                                    .ok_or_else(|| ParseError {
                                        message: format!(
                                            "Invalid action type: {}",
                                            comp_inner.as_str()
                                        ),
                                    })?;
                                format = Some(action_type);
                            }
                            _ => {
                                return Err(ParseError {
                                    message: format!(
                                        "Unexpected action component: {:?}",
                                        comp_inner.as_rule()
                                    ),
                                });
                            }
                        }
                    }
                    _ => {
                        return Err(ParseError {
                            message: format!("Unexpected action rule: {:?}", component.as_rule()),
                        });
                    }
                }
            }

            Ok(Statement::Action {
                averages,
                frequencies,
                format,
            })
        }
        Rule::dealer_stmt => {
            let compass_str = inner.into_inner().next().unwrap().as_str().to_lowercase();
            let position = match compass_str.as_str() {
                "north" | "n" => Position::North,
                "south" | "s" => Position::South,
                "east" | "e" => Position::East,
                "west" | "w" => Position::West,
                _ => {
                    return Err(ParseError {
                        message: format!("Invalid dealer position: {}", compass_str),
                    })
                }
            };
            Ok(Statement::Dealer(position))
        }
        Rule::vulnerable_stmt => {
            let vuln_str = inner.into_inner().next().unwrap().as_str();
            let vuln = VulnerabilityType::parse(vuln_str).ok_or_else(|| ParseError {
                message: format!("Invalid vulnerability: {}", vuln_str),
            })?;
            Ok(Statement::Vulnerable(vuln))
        }
        Rule::predeal_stmt => {
            let mut parts = inner.into_inner();

            // Parse position
            let compass_str = parts.next().unwrap().as_str().to_lowercase();
            let position = match compass_str.as_str() {
                "north" | "n" => Position::North,
                "south" | "s" => Position::South,
                "east" | "e" => Position::East,
                "west" | "w" => Position::West,
                _ => {
                    return Err(ParseError {
                        message: format!("Invalid predeal position: {}", compass_str),
                    })
                }
            };

            // Parse cards - each card_pair may contain multiple cards (e.g., "ST62" = 3 cards)
            let mut cards = Vec::new();
            for card_pair in parts {
                let card_str = card_pair.as_str();
                let parsed_cards = parse_cards(card_str)?;
                cards.extend(parsed_cards);
            }

            Ok(Statement::Predeal { position, cards })
        }
        Rule::csvrpt_stmt => {
            let mut csv_terms = Vec::new();

            for term_pair in inner.into_inner() {
                if term_pair.as_rule() == Rule::csv_term {
                    // Check if csv_term has inner content or if it's a direct match (like "deal")
                    let term_str = term_pair.as_str().to_lowercase();

                    let csv_term = if term_str == "deal" {
                        CsvTerm::Deal
                    } else if let Some(term_inner) = term_pair.into_inner().next() {
                        match term_inner.as_rule() {
                            Rule::expr => CsvTerm::Expression(build_ast(term_inner)?),
                            Rule::string_literal => {
                                let s = term_inner.as_str();
                                // Strip quotes
                                CsvTerm::String(s[1..s.len() - 1].to_string())
                            }
                            Rule::compass => {
                                let compass_str = term_inner.as_str().to_lowercase();
                                let position = match compass_str.as_str() {
                                    "north" | "n" => Position::North,
                                    "south" | "s" => Position::South,
                                    "east" | "e" => Position::East,
                                    "west" | "w" => Position::West,
                                    _ => {
                                        return Err(ParseError {
                                            message: format!("Invalid compass: {}", compass_str),
                                        })
                                    }
                                };
                                CsvTerm::Compass(position)
                            }
                            Rule::side => {
                                let side_str = term_inner.as_str().to_lowercase();
                                match side_str.as_str() {
                                    "ns" => CsvTerm::Side(Side::NS),
                                    "ew" => CsvTerm::Side(Side::EW),
                                    _ => {
                                        return Err(ParseError {
                                            message: format!("Invalid side: {}", side_str),
                                        })
                                    }
                                }
                            }
                            _ => {
                                return Err(ParseError {
                                    message: format!(
                                        "Unexpected csv_term rule: {:?}",
                                        term_inner.as_rule()
                                    ),
                                })
                            }
                        }
                    } else {
                        return Err(ParseError {
                            message: format!("Unexpected csv_term format: {}", term_str),
                        });
                    };

                    csv_terms.push(csv_term);
                }
            }

            Ok(Statement::CsvReport(csv_terms))
        }
        Rule::average_stmt => {
            // Standalone average statement: average "label"? expr
            let mut parts = inner.into_inner();
            let first = parts.next().unwrap();

            let (label, expr_pair) = if first.as_rule() == Rule::string_literal {
                // Has label - strip quotes
                let label_str = first.as_str();
                let label = label_str[1..label_str.len() - 1].to_string();
                (Some(label), parts.next().unwrap())
            } else {
                // No label - first element is the expression
                (None, first)
            };

            let expr = build_ast(expr_pair)?;
            Ok(Statement::Action {
                averages: vec![AverageSpec { label, expr }],
                frequencies: Vec::new(),
                format: None,
            })
        }
        Rule::frequency_stmt => {
            // Standalone frequency statement: frequency "label"? (expr, min, max)
            let mut parts = inner.into_inner();
            let first = parts.next().unwrap();

            let (label, expr_pair) = if first.as_rule() == Rule::string_literal {
                // Has label - strip quotes
                let label_str = first.as_str();
                let label = label_str[1..label_str.len() - 1].to_string();
                (Some(label), parts.next().unwrap())
            } else {
                // No label - first element is the expression
                (None, first)
            };

            let expr = build_ast(expr_pair)?;

            // Parse range (min, max)
            let min = parts
                .next()
                .unwrap()
                .as_str()
                .parse::<i32>()
                .map_err(|_| ParseError {
                    message: "Invalid frequency range min".to_string(),
                })?;
            let max = parts
                .next()
                .unwrap()
                .as_str()
                .parse::<i32>()
                .map_err(|_| ParseError {
                    message: "Invalid frequency range max".to_string(),
                })?;

            Ok(Statement::Action {
                averages: Vec::new(),
                frequencies: vec![FrequencySpec {
                    label,
                    expr,
                    range: Some((min, max)),
                }],
                format: None,
            })
        }
        Rule::print_stmt => {
            // Standalone print statement: printpbn, printall, etc.
            let action_type = ActionType::parse(inner.as_str()).ok_or_else(|| ParseError {
                message: format!("Invalid print statement: {}", inner.as_str()),
            })?;
            Ok(Statement::Action {
                averages: Vec::new(),
                frequencies: Vec::new(),
                format: Some(action_type),
            })
        }
        Rule::assignment => {
            let mut parts = inner.into_inner();
            let name = parts.next().unwrap().as_str().to_string();
            let expr = build_ast(parts.next().unwrap())?;
            Ok(Statement::Assignment { name, expr })
        }
        Rule::expr => {
            let expr = build_ast(inner)?;
            Ok(Statement::Expression(expr))
        }
        _ => Err(ParseError {
            message: format!("Unexpected statement rule: {:?}", inner.as_rule()),
        }),
    }
}

/// Parse a single card from a string like "AS", "KH", "2C" (rank+suit format for hascard)
fn parse_card(card_str: &str) -> Result<dealer_core::Card, ParseError> {
    if card_str.len() != 2 {
        return Err(ParseError {
            message: format!("Card must be exactly 2 characters, got {}", card_str),
        });
    }

    let chars: Vec<char> = card_str.chars().collect();
    let rank_char = chars[0];
    let suit_char = chars[1];

    let rank = match rank_char {
        'A' => dealer_core::Rank::Ace,
        'K' => dealer_core::Rank::King,
        'Q' => dealer_core::Rank::Queen,
        'J' => dealer_core::Rank::Jack,
        'T' => dealer_core::Rank::Ten,
        '9' => dealer_core::Rank::Nine,
        '8' => dealer_core::Rank::Eight,
        '7' => dealer_core::Rank::Seven,
        '6' => dealer_core::Rank::Six,
        '5' => dealer_core::Rank::Five,
        '4' => dealer_core::Rank::Four,
        '3' => dealer_core::Rank::Three,
        '2' => dealer_core::Rank::Two,
        _ => {
            return Err(ParseError {
                message: format!("Invalid rank: {}", rank_char),
            })
        }
    };

    let suit = match suit_char {
        'S' => dealer_core::Suit::Spades,
        'H' => dealer_core::Suit::Hearts,
        'D' => dealer_core::Suit::Diamonds,
        'C' => dealer_core::Suit::Clubs,
        _ => {
            return Err(ParseError {
                message: format!("Invalid suit: {}", suit_char),
            })
        }
    };

    Ok(dealer_core::Card::new(suit, rank))
}

/// Parse cards from a string like "SA", "HKQ", "DT62", "C95", or just "S" (suit only)
/// dealer.exe predeal format: suit character followed by zero or more rank characters
/// A suit alone (e.g., "S") returns an empty vector, meaning no specific cards for that suit
fn parse_cards(card_str: &str) -> Result<Vec<dealer_core::Card>, ParseError> {
    if card_str.is_empty() {
        return Err(ParseError {
            message: "Card spec cannot be empty".to_string(),
        });
    }

    let chars: Vec<char> = card_str.chars().collect();
    let suit_char = chars[0];

    let suit = match suit_char {
        'S' => dealer_core::Suit::Spades,
        'H' => dealer_core::Suit::Hearts,
        'D' => dealer_core::Suit::Diamonds,
        'C' => dealer_core::Suit::Clubs,
        _ => {
            return Err(ParseError {
                message: format!("Invalid suit: {}", suit_char),
            })
        }
    };

    let mut cards = Vec::new();
    for &rank_char in &chars[1..] {
        let rank = match rank_char {
            'A' => dealer_core::Rank::Ace,
            'K' => dealer_core::Rank::King,
            'Q' => dealer_core::Rank::Queen,
            'J' => dealer_core::Rank::Jack,
            'T' => dealer_core::Rank::Ten,
            '9' => dealer_core::Rank::Nine,
            '8' => dealer_core::Rank::Eight,
            '7' => dealer_core::Rank::Seven,
            '6' => dealer_core::Rank::Six,
            '5' => dealer_core::Rank::Five,
            '4' => dealer_core::Rank::Four,
            '3' => dealer_core::Rank::Three,
            '2' => dealer_core::Rank::Two,
            _ => {
                return Err(ParseError {
                    message: format!("Invalid rank: {}", rank_char),
                })
            }
        };
        cards.push(dealer_core::Card::new(suit, rank));
    }

    Ok(cards)
}

/// Build AST from pest parse tree
fn build_ast(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    match pair.as_rule() {
        Rule::expr => build_ast(pair.into_inner().next().unwrap()),

        Rule::ternary => {
            let mut pairs = pair.into_inner();
            let condition = build_ast(pairs.next().unwrap())?;

            // Check if there are more elements (the ? and : parts)
            if let Some(true_pair) = pairs.next() {
                let true_expr = build_ast(true_pair)?;
                let false_expr = build_ast(pairs.next().unwrap())?;
                Ok(Expr::ternary(condition, true_expr, false_expr))
            } else {
                // No ternary operator, just pass through the condition
                Ok(condition)
            }
        }

        Rule::logical_or => {
            let mut pairs = pair.into_inner();
            let mut expr = build_ast(pairs.next().unwrap())?;

            while let Some(_op_pair) = pairs.next() {
                // Skip the operator token (or_op), get the right operand
                let right = build_ast(pairs.next().unwrap())?;
                expr = Expr::binary(BinaryOp::Or, expr, right);
            }
            Ok(expr)
        }

        Rule::logical_and => {
            let mut pairs = pair.into_inner();
            let mut expr = build_ast(pairs.next().unwrap())?;

            while let Some(_op_pair) = pairs.next() {
                // Skip the operator token (and_op), get the right operand
                let right = build_ast(pairs.next().unwrap())?;
                expr = Expr::binary(BinaryOp::And, expr, right);
            }
            Ok(expr)
        }

        Rule::logical_not => {
            let mut inner_pairs = pair.into_inner();
            let first = inner_pairs.next().unwrap();

            // Check if first element is not_op
            if first.as_rule() == Rule::not_op {
                // We have a NOT operation - next element is the operand
                let operand = build_ast(inner_pairs.next().unwrap())?;
                Ok(Expr::unary(UnaryOp::Not, operand))
            } else {
                // No NOT operator, just pass through to comparison
                build_ast(first)
            }
        }

        Rule::comparison => {
            // Chained comparisons: a==b==c becomes (a==b) && (b==c)
            let mut pairs = pair.into_inner();
            let first = build_ast(pairs.next().unwrap())?;

            // Collect all operators and operands
            let mut operands = vec![first];
            let mut operators = Vec::new();

            while let Some(op_pair) = pairs.next() {
                let op = match op_pair.as_str() {
                    "==" => BinaryOp::Eq,
                    "!=" => BinaryOp::Ne,
                    "<" => BinaryOp::Lt,
                    "<=" => BinaryOp::Le,
                    ">" => BinaryOp::Gt,
                    ">=" => BinaryOp::Ge,
                    _ => {
                        return Err(ParseError {
                            message: format!("Unknown comparison operator: {}", op_pair.as_str()),
                        })
                    }
                };
                operators.push(op);
                operands.push(build_ast(pairs.next().unwrap())?);
            }

            if operators.is_empty() {
                // No comparison, just return the operand
                Ok(operands.into_iter().next().unwrap())
            } else if operators.len() == 1 {
                // Single comparison: a op b
                let right = operands.pop().unwrap();
                let left = operands.pop().unwrap();
                Ok(Expr::binary(operators[0], left, right))
            } else {
                // Chained comparison: a op1 b op2 c ... becomes (a op1 b) && (b op2 c) && ...
                let mut comparisons = Vec::new();
                for i in 0..operators.len() {
                    comparisons.push(Expr::binary(
                        operators[i],
                        operands[i].clone(),
                        operands[i + 1].clone(),
                    ));
                }
                // AND all the comparisons together
                let mut result = comparisons.remove(0);
                for cmp in comparisons {
                    result = Expr::binary(BinaryOp::And, result, cmp);
                }
                Ok(result)
            }
        }

        Rule::additive => {
            let mut pairs = pair.into_inner();
            let mut expr = build_ast(pairs.next().unwrap())?;

            while let Some(op_pair) = pairs.next() {
                let op = match op_pair.as_str() {
                    "+" => BinaryOp::Add,
                    "-" => BinaryOp::Sub,
                    _ => {
                        return Err(ParseError {
                            message: format!("Unknown additive operator: {}", op_pair.as_str()),
                        })
                    }
                };
                let right = build_ast(pairs.next().unwrap())?;
                expr = Expr::binary(op, expr, right);
            }
            Ok(expr)
        }

        Rule::multiplicative => {
            let mut pairs = pair.into_inner();
            let mut expr = build_ast(pairs.next().unwrap())?;

            while let Some(op_pair) = pairs.next() {
                let op = match op_pair.as_str() {
                    "*" => BinaryOp::Mul,
                    "/" => BinaryOp::Div,
                    "%" => BinaryOp::Mod,
                    _ => {
                        return Err(ParseError {
                            message: format!(
                                "Unknown multiplicative operator: {}",
                                op_pair.as_str()
                            ),
                        })
                    }
                };
                let right = build_ast(pairs.next().unwrap())?;
                expr = Expr::binary(op, expr, right);
            }
            Ok(expr)
        }

        Rule::unary => {
            let mut pairs = pair.into_inner();
            let first = pairs.next().unwrap();

            match first.as_rule() {
                Rule::not_op => {
                    let inner = build_ast(pairs.next().unwrap())?;
                    Ok(Expr::unary(UnaryOp::Not, inner))
                }
                _ if first.as_str() == "-" => {
                    let inner = build_ast(pairs.next().unwrap())?;
                    Ok(Expr::unary(UnaryOp::Negate, inner))
                }
                _ => build_ast(first),
            }
        }

        Rule::paren_expr => {
            let inner = pair.into_inner().next().unwrap();
            build_ast(inner)
        }

        Rule::function_call => {
            let mut pairs = pair.into_inner();
            let func_name = pairs.next().unwrap().as_str();

            // Collect all arguments
            let mut args = Vec::new();
            for arg_pair in pairs {
                args.push(build_ast(arg_pair)?);
            }

            let func = Function::parse(func_name).ok_or_else(|| ParseError {
                message: format!("Unknown function: {}", func_name),
            })?;

            Ok(Expr::call_multi(func, args))
        }

        Rule::function_name => {
            // This shouldn't be called directly
            Err(ParseError {
                message: "Unexpected function_name rule".to_string(),
            })
        }

        Rule::position => {
            let pos_str = pair.as_str().to_lowercase();
            let position = match pos_str.as_str() {
                "north" | "n" => Position::North,
                "south" | "s" => Position::South,
                "east" | "e" => Position::East,
                "west" | "w" => Position::West,
                _ => {
                    return Err(ParseError {
                        message: format!("Unknown position: {}", pos_str),
                    })
                }
            };
            Ok(Expr::Position(position))
        }

        Rule::literal => {
            let value = pair.as_str().parse::<i32>().map_err(|e| ParseError {
                message: format!("Invalid integer literal: {}", e),
            })?;
            Ok(Expr::Literal(value))
        }

        Rule::card => {
            let card_str = pair.as_str();
            let card = parse_card(card_str)?;
            Ok(Expr::Card(card))
        }

        Rule::suit => {
            let suit_str = pair.as_str().to_lowercase();
            let suit = match suit_str.as_str() {
                "spades" => dealer_core::Suit::Spades,
                "hearts" => dealer_core::Suit::Hearts,
                "diamonds" => dealer_core::Suit::Diamonds,
                "clubs" => dealer_core::Suit::Clubs,
                _ => {
                    return Err(ParseError {
                        message: format!("Unknown suit: {}", suit_str),
                    })
                }
            };
            Ok(Expr::Suit(suit))
        }

        Rule::shape_pattern => {
            let mut specs = Vec::new();
            let mut include = true; // First spec is always included

            for inner_pair in pair.into_inner() {
                match inner_pair.as_rule() {
                    Rule::shape_spec => {
                        let shape = parse_shape_spec(inner_pair)?;
                        specs.push(ShapeSpec { include, shape });
                        include = true; // Reset for next spec
                    }
                    Rule::shape_op => {
                        include = inner_pair.as_str() == "+";
                    }
                    _ => {}
                }
            }

            Ok(Expr::ShapePattern(ShapePattern::new(specs)))
        }

        Rule::ident => {
            // Variable reference
            let name = pair.as_str().to_string();
            Ok(Expr::Variable(name))
        }

        _ => Err(ParseError {
            message: format!("Unexpected rule: {:?}", pair.as_rule()),
        }),
    }
}

/// Parse a shape specification like "any 4333" or "54xx"
fn parse_shape_spec(pair: Pair<Rule>) -> Result<Shape, ParseError> {
    let mut is_any = false;
    let mut digits_str = "";

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::shape_any => is_any = true,
            Rule::shape_digits_any => digits_str = inner.as_str(),
            Rule::shape_digits_marked => {
                digits_str = inner.as_str();
                // Strip %s prefix if present
                digits_str = digits_str.strip_prefix("%s").unwrap_or(digits_str);
            }
            _ => {}
        }
    }

    let chars: Vec<char> = digits_str.chars().collect();
    if chars.len() != 4 {
        return Err(ParseError {
            message: format!("Shape must be exactly 4 characters, got {}", digits_str),
        });
    }

    // Check if any wildcards
    let has_wildcard = chars.iter().any(|&c| c == 'x' || c == 'X');

    if has_wildcard {
        // Wildcard pattern
        let mut pattern = [None; 4];
        for (i, &ch) in chars.iter().enumerate() {
            if ch == 'x' || ch == 'X' {
                pattern[i] = None;
            } else if ch.is_ascii_digit() {
                let digit = ch.to_digit(10).unwrap() as u8;
                if digit > 13 {
                    return Err(ParseError {
                        message: format!("Shape digit {} is too large (max 13)", digit),
                    });
                }
                pattern[i] = Some(digit);
            } else {
                return Err(ParseError {
                    message: format!("Invalid character in shape: {}", ch),
                });
            }
        }
        if is_any {
            // "any 6xxx" = any permutation of this wildcard pattern
            Ok(Shape::AnyWildcard(pattern))
        } else {
            Ok(Shape::Wildcard(pattern))
        }
    } else {
        // Exact or "any" distribution
        let mut pattern = [0u8; 4];
        for (i, &ch) in chars.iter().enumerate() {
            if !ch.is_ascii_digit() {
                return Err(ParseError {
                    message: format!("Invalid character in shape: {}", ch),
                });
            }
            let digit = ch.to_digit(10).unwrap() as u8;
            if digit > 13 {
                return Err(ParseError {
                    message: format!("Shape digit {} is too large (max 13)", digit),
                });
            }
            pattern[i] = digit;
        }

        // Validate that digits sum to 13
        let sum: u8 = pattern.iter().sum();
        if sum != 13 {
            return Err(ParseError {
                message: format!("Shape digits must sum to 13, got {}", sum),
            });
        }

        if is_any {
            Ok(Shape::AnyDistribution(pattern))
        } else {
            Ok(Shape::Exact(pattern))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_comparison() {
        let ast = parse("hcp(north) >= 15").unwrap();

        match ast {
            Expr::BinaryOp { op, left, right } => {
                assert_eq!(op, BinaryOp::Ge);
                match *left {
                    Expr::FunctionCall { func, .. } => assert_eq!(func, Function::Hcp),
                    _ => panic!("Expected function call"),
                }
                match *right {
                    Expr::Literal(15) => (),
                    _ => panic!("Expected literal 15"),
                }
            }
            _ => panic!("Expected binary operation"),
        }
    }

    #[test]
    fn test_parse_logical_and() {
        let ast = parse("hearts(north) >= 5 && hcp(south) <= 13").unwrap();

        match ast {
            Expr::BinaryOp { op, .. } => {
                assert_eq!(op, BinaryOp::And);
            }
            _ => panic!("Expected AND operation"),
        }
    }

    #[test]
    fn test_parse_positions() {
        assert!(parse("hcp(north) > 0").is_ok());
        assert!(parse("hcp(south) > 0").is_ok());
        assert!(parse("hcp(east) > 0").is_ok());
        assert!(parse("hcp(west) > 0").is_ok());
        assert!(parse("hcp(n) > 0").is_ok());
        assert!(parse("hcp(N) > 0").is_ok());
    }

    #[test]
    fn test_parse_arithmetic() {
        let ast = parse("hcp(north) + hcp(south) >= 25").unwrap();

        match ast {
            Expr::BinaryOp { op, left, .. } => {
                assert_eq!(op, BinaryOp::Ge);
                match *left {
                    Expr::BinaryOp { op, .. } => assert_eq!(op, BinaryOp::Add),
                    _ => panic!("Expected addition"),
                }
            }
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn test_parse_logical_not() {
        // Test ! operator
        let ast = parse("!(hcp(north) < 10)").unwrap();
        match ast {
            Expr::UnaryOp { op, expr } => {
                assert_eq!(op, UnaryOp::Not);
                match *expr {
                    Expr::BinaryOp { op, .. } => assert_eq!(op, BinaryOp::Lt),
                    _ => panic!("Expected binary op in NOT operand"),
                }
            }
            _ => panic!("Expected unary NOT operation"),
        }
    }

    #[test]
    fn test_parse_not_keyword() {
        // Test not keyword
        let ast = parse("not (hcp(north) < 10)").unwrap();
        match ast {
            Expr::UnaryOp { op, .. } => {
                assert_eq!(op, UnaryOp::Not);
            }
            _ => panic!("Expected unary NOT operation"),
        }
    }

    #[test]
    fn test_parse_error() {
        assert!(parse("invalid syntax here").is_err());
        assert!(parse("hcp(north) >=").is_err());
    }

    #[test]
    fn test_parse_program_single_expression() {
        let program = parse_program("hcp(north) >= 15").unwrap();
        assert_eq!(program.statements.len(), 1);
        match &program.statements[0] {
            Statement::Expression(_) => (),
            _ => panic!("Expected expression statement"),
        }
    }

    #[test]
    fn test_parse_program_with_assignment() {
        let program = parse_program("opener = hcp(north) >= 15\nopener").unwrap();
        assert_eq!(program.statements.len(), 2);

        match &program.statements[0] {
            Statement::Assignment { name, .. } => {
                assert_eq!(name, "opener");
            }
            _ => panic!("Expected assignment statement"),
        }

        match &program.statements[1] {
            Statement::Expression(Expr::Variable(name)) => {
                assert_eq!(name, "opener");
            }
            _ => panic!("Expected variable reference"),
        }
    }

    #[test]
    fn test_parse_program_multiple_assignments() {
        let input =
            "strong = hcp(north) >= 15\nlong_hearts = hearts(north) >= 5\nstrong && long_hearts";
        let program = parse_program(input).unwrap();
        assert_eq!(program.statements.len(), 3);

        // Check first assignment
        match &program.statements[0] {
            Statement::Assignment { name, .. } => assert_eq!(name, "strong"),
            _ => panic!("Expected assignment"),
        }

        // Check second assignment
        match &program.statements[1] {
            Statement::Assignment { name, .. } => assert_eq!(name, "long_hearts"),
            _ => panic!("Expected assignment"),
        }

        // Check final expression
        match &program.statements[2] {
            Statement::Expression(_) => (),
            _ => panic!("Expected expression"),
        }
    }

    #[test]
    fn test_parse_program_semicolon_separator() {
        let program = parse_program("opener = hcp(north) >= 15; opener").unwrap();
        assert_eq!(program.statements.len(), 2);
    }

    #[test]
    fn test_parse_variable_in_expression() {
        let program = parse_program("x = hcp(north)\nx >= 15").unwrap();
        assert_eq!(program.statements.len(), 2);

        match &program.statements[1] {
            Statement::Expression(Expr::BinaryOp { left, .. }) => match **left {
                Expr::Variable(ref name) => assert_eq!(name, "x"),
                _ => panic!("Expected variable reference"),
            },
            _ => panic!("Expected expression"),
        }
    }

    #[test]
    fn test_parse_ternary_operator() {
        // Simple ternary
        let ast = parse("hcp(north) >= 15 ? 1 : 0").unwrap();
        match ast {
            Expr::Ternary {
                condition,
                true_expr,
                false_expr,
            } => {
                // Condition should be a binary op
                match *condition {
                    Expr::BinaryOp { op, .. } => assert_eq!(op, BinaryOp::Ge),
                    _ => panic!("Expected binary op in condition"),
                }
                // True branch should be 1
                match *true_expr {
                    Expr::Literal(1) => (),
                    _ => panic!("Expected literal 1 in true branch"),
                }
                // False branch should be 0
                match *false_expr {
                    Expr::Literal(0) => (),
                    _ => panic!("Expected literal 0 in false branch"),
                }
            }
            _ => panic!("Expected ternary expression"),
        }
    }

    #[test]
    fn test_parse_nested_ternary() {
        // Nested ternary: hcp(north) >= 15 ? (hearts(north) >= 5 ? 2 : 1) : 0
        let ast = parse("hcp(north) >= 15 ? (hearts(north) >= 5 ? 2 : 1) : 0").unwrap();
        match ast {
            Expr::Ternary { true_expr, .. } => {
                // True branch should be another ternary
                match *true_expr {
                    Expr::Ternary { .. } => (),
                    _ => panic!("Expected nested ternary in true branch"),
                }
            }
            _ => panic!("Expected ternary expression"),
        }
    }

    #[test]
    fn test_parse_predeal_with_suit_only() {
        // Predeal with suit-only holdings (no specific cards for that suit)
        // dealer.exe allows "S,H,DAK,CAK" where S and H have no cards specified
        let program = parse_program("predeal north S,H,DAK,CAK").unwrap();

        // Find the Predeal statement
        let predeal = program.statements.iter().find_map(|s| {
            if let Statement::Predeal { position, cards } = s {
                Some((position, cards))
            } else {
                None
            }
        });
        let (pos, cards) = predeal.expect("Should have a predeal statement");
        assert_eq!(*pos, Position::North);
        // Only DAK and CAK should have cards, S and H are empty
        assert_eq!(cards.len(), 4); // DA, DK, CA, CK
    }

    #[test]
    fn test_parse_predeal_cards() {
        // Test parsing of predeal card specs
        let program = parse_program("predeal south SAK,HQ,D,CAKQJT").unwrap();

        // Find the Predeal statement
        let predeal = program.statements.iter().find_map(|s| {
            if let Statement::Predeal { position, cards } = s {
                Some((position, cards))
            } else {
                None
            }
        });
        let (pos, cards) = predeal.expect("Should have a predeal statement");
        assert_eq!(*pos, Position::South);
        // SAK = 2, HQ = 1, D = 0, CAKQJT = 5
        assert_eq!(cards.len(), 8);
    }

    #[test]
    fn test_parse_chained_comparison() {
        // Chained comparison: a==b==c becomes (a==b) && (b==c)
        let ast = parse("spades(west)==hearts(west)==3").unwrap();

        // Should be: (spades(west)==hearts(west)) && (hearts(west)==3)
        match ast {
            Expr::BinaryOp {
                op: BinaryOp::And,
                left,
                right,
            } => {
                // Left should be spades(west)==hearts(west)
                match *left {
                    Expr::BinaryOp {
                        op: BinaryOp::Eq, ..
                    } => (),
                    _ => panic!("Expected left to be Eq comparison"),
                }
                // Right should be hearts(west)==3
                match *right {
                    Expr::BinaryOp {
                        op: BinaryOp::Eq, ..
                    } => (),
                    _ => panic!("Expected right to be Eq comparison"),
                }
            }
            _ => panic!("Expected AND operation for chained comparison"),
        }
    }

    #[test]
    fn test_parse_chained_comparison_with_parens() {
        // Chained comparison with parenthesized OR: a==b==(3 or 4)
        // This is from GIB_1C-P-Resp.dlr: spades(west)==hearts(west)==(3 or 4)
        let ast = parse("spades(west)==hearts(west)==(3 or 4)").unwrap();

        // Should be: (spades(west)==hearts(west)) && (hearts(west)==(3 or 4))
        match ast {
            Expr::BinaryOp {
                op: BinaryOp::And, ..
            } => (),
            _ => panic!("Expected AND operation for chained comparison"),
        }
    }

    #[test]
    fn test_parse_triple_chained_comparison() {
        // Triple chain: a==b==c==d becomes (a==b) && (b==c) && (c==d)
        let ast = parse("1==2==3==4").unwrap();

        // Should be: ((1==2) && (2==3)) && (3==4)
        match ast {
            Expr::BinaryOp {
                op: BinaryOp::And,
                left,
                right,
            } => {
                // Left should be (1==2) && (2==3)
                match *left {
                    Expr::BinaryOp {
                        op: BinaryOp::And, ..
                    } => (),
                    _ => panic!("Expected left to be AND"),
                }
                // Right should be (3==4)
                match *right {
                    Expr::BinaryOp {
                        op: BinaryOp::Eq, ..
                    } => (),
                    _ => panic!("Expected right to be Eq"),
                }
            }
            _ => panic!("Expected AND operation for triple chain"),
        }
    }

    #[test]
    fn test_parse_bare_action() {
        // dealer.exe accepts 'action' without arguments (means no output, just count)
        let program = parse_program("action").unwrap();
        assert_eq!(program.statements.len(), 1);
        match &program.statements[0] {
            Statement::Action {
                averages,
                frequencies,
                format,
            } => {
                assert!(averages.is_empty());
                assert!(frequencies.is_empty());
                assert!(format.is_none());
            }
            _ => panic!("Expected Action statement"),
        }
    }

    #[test]
    fn test_parse_generate_stmt() {
        let program = parse_program("generate 1000000").unwrap();
        assert_eq!(program.statements.len(), 1);
        assert_eq!(program.statements[0], Statement::Generate(1_000_000));
    }

    #[test]
    fn test_generate_does_not_clobber_condition() {
        // Bug fix: 'generate 1000000' was parsed as two expressions
        // (Variable("generate") and Literal(1000000)), clobbering the real condition
        let input = "hcp(north) >= 15\ngenerate 1000000\nproduce 30\naction\nprintoneline,";
        let program = parse_program(input).unwrap();

        // Find the expression/condition statement
        let has_condition = program
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Expression(_) | Statement::Condition(_)));
        assert!(has_condition, "Should have a condition expression");

        // Verify generate is parsed as its own statement type, not as Expression
        let has_generate = program
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Generate(1_000_000)));
        assert!(has_generate, "Should have Generate(1000000) statement");

        // Verify no Expression(Literal(1000000)) that would clobber the condition
        let has_literal_expr = program
            .statements
            .iter()
            .any(|s| matches!(s, Statement::Expression(Expr::Literal(1_000_000))));
        assert!(
            !has_literal_expr,
            "Should NOT have Expression(Literal(1000000))"
        );
    }
}
