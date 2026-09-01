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
                build_statements(statement_pair, &mut statements)?;
            }
        }
    }

    Ok(Program { statements })
}

/// Rows in the count table. Kept here so the parser can reject an out-of-range
/// `altcount` where the script is written, rather than at evaluation time; the
/// evaluator's `counts::NUM_ROWS` is the same number and a test ties them.
pub const NUM_COUNT_ROWS: usize = 12;

/// Ranks a count row can name: ace down to two.
pub const MAX_COUNT_VALUES: usize = 13;

fn read_count_values(pairs: pest::iterators::Pairs<Rule>) -> Result<Vec<i32>, ParseError> {
    pairs
        .map(|p| {
            // Through `literal_value`, so a count row can be written in
            // hundredths — `altcount 8 6.25 4.25 1.5 0.75 .25`, which is what
            // decimals exist for.
            literal_value(p.as_str()).map_err(|_| ParseError {
                message: format!("Invalid point count value: {}", p.as_str()),
            })
        })
        .collect()
}

fn check_count_length(values: &[i32]) -> Result<(), ParseError> {
    if values.len() > MAX_COUNT_VALUES {
        return Err(ParseError {
            message: format!(
                "too many pointcount values: {} given, at most {} (ace down to two)",
                values.len(),
                MAX_COUNT_VALUES
            ),
        });
    }
    Ok(())
}

/// Build a statement from pest parse tree
/// Build a `printes(...)` list from its rule pair.
fn build_es_terms(pair: pest::iterators::Pair<Rule>) -> Result<Vec<EsTerm>, ParseError> {
    let mut terms = Vec::new();
    for term in pair.into_inner() {
        if term.as_rule() != Rule::es_term {
            continue;
        }
        let inner = term.into_inner().next().ok_or_else(|| ParseError {
            message: "empty printes term".to_string(),
        })?;
        terms.push(match inner.as_rule() {
            // Kept exactly as written: the original reads no escapes inside
            // quotes, so what is between them is what gets printed.
            Rule::string_literal => {
                let raw = inner.as_str();
                EsTerm::String(raw[1..raw.len() - 1].to_string())
            }
            Rule::newline => EsTerm::Newline,
            _ => EsTerm::Expression(build_ast(inner)?),
        });
    }
    Ok(terms)
}

/// Build the seat list of a `print(...)` action.
fn build_print_hands(pair: pest::iterators::Pair<Rule>) -> Result<Vec<Position>, ParseError> {
    let mut seats = Vec::new();
    for compass in pair.into_inner() {
        if compass.as_rule() != Rule::compass {
            continue;
        }
        let seat = match compass.as_str().to_lowercase().as_str() {
            "north" | "n" => Position::North,
            "east" | "e" => Position::East,
            "south" | "s" => Position::South,
            "west" | "w" => Position::West,
            other => {
                return Err(ParseError {
                    message: format!("Invalid seat in print(): {}", other),
                })
            }
        };
        // The original ORs the seats into a bitmask, so naming one twice is
        // the same as naming it once.
        if !seats.contains(&seat) {
            seats.push(seat);
        }
    }
    Ok(seats)
}

/// The term list shared by `csvrpt` and `printrpt`.
///
/// One function because it is one list: DealerV2_4's `printrpt` is its
/// `csvrpt` to the screen, down to the quoting and the commas, and two copies
/// of this would drift the first time a term type was added.
fn build_csv_terms(inner: Pair<Rule>) -> Result<Vec<CsvTerm>, ParseError> {
    let mut csv_terms = Vec::new();

    for term_pair in inner.into_inner() {
        if term_pair.as_rule() == Rule::csv_term {
            // Check if csv_term has inner content or if it's a direct match (like "deal")
            let term_str = term_pair.as_str().to_lowercase();

            let csv_term = if term_str == "deal" {
                CsvTerm::Deal
            } else if let Some(term_inner) = term_pair.into_inner().next() {
                match term_inner.as_rule() {
                    Rule::trix_spec => {
                        let target = term_inner.into_inner().next();
                        let seats = match target {
                            // `trix(deal)`: the keyword is consumed by the rule
                            // itself, so there is no inner pair to look at.
                            None => Position::ALL.to_vec(),
                            Some(compass) => vec![compass_position(compass.as_str())?],
                        };
                        CsvTerm::Trix(seats)
                    }
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

    Ok(csv_terms)
}

/// Append the statements one written statement stands for.
///
/// Almost always exactly one. `predeal` is the exception: it may name several
/// seats, and each becomes its own `Statement::Predeal` rather than the AST
/// growing a list. That keeps every consumer — which all walk the statements
/// and act on each `Predeal` they meet — working unchanged, and it is what the
/// statement means anyway, since the original's parser calls
/// `predeal_holding(compass, ...)` once per holding as it reduces.
fn build_statements(pair: Pair<Rule>, out: &mut Vec<Statement>) -> Result<(), ParseError> {
    let inner = match pair.clone().into_inner().next() {
        Some(inner) => inner,
        None => return Ok(()),
    };
    if inner.as_rule() == Rule::predeal_stmt {
        for group in inner.into_inner() {
            out.push(build_predeal_group(group)?);
        }
        return Ok(());
    }
    out.push(build_statement(pair)?);
    Ok(())
}

/// One seat of a `predeal`, and the cards it names.
fn build_predeal_group(group: Pair<Rule>) -> Result<Statement, ParseError> {
    let mut parts = group.into_inner();

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

    // Each holding may name several cards: `ST62` is three of them.
    let mut cards = Vec::new();
    for card_pair in parts {
        cards.extend(parse_cards(card_pair.as_str())?);
    }

    Ok(Statement::Predeal { position, cards })
}

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
            let mut printes = Vec::new();
            let mut print_hands: Vec<Position> = Vec::new();
            let mut print_reports = Vec::new();

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
                                frequencies.push(build_frequency(comp_inner.into_inner())?);
                            }
                            Rule::printes_spec => {
                                printes.push(build_es_terms(comp_inner)?);
                            }
                            Rule::printrpt_spec => {
                                print_reports.push(build_csv_terms(comp_inner)?);
                            }
                            Rule::printhands_spec => {
                                for seat in build_print_hands(comp_inner)? {
                                    if !print_hands.contains(&seat) {
                                        print_hands.push(seat);
                                    }
                                }
                            }
                            Rule::action_type => {
                                format = Some(action_type_from(comp_inner)?);
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
                printes,
                print_hands,
                print_reports,
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
        Rule::title_stmt => {
            let raw = inner
                .into_inner()
                .next()
                .ok_or_else(|| ParseError {
                    message: "title needs a quoted string".to_string(),
                })?
                .as_str();
            // The quotes are the delimiters, not part of the title, and the
            // original reads no escapes between them.
            Ok(Statement::Title(raw[1..raw.len() - 1].to_string()))
        }
        Rule::seed_stmt => {
            let raw = inner
                .into_inner()
                .next()
                .ok_or_else(|| ParseError {
                    message: "seed needs a number".to_string(),
                })?
                .as_str();
            let seed: u32 = raw.parse().map_err(|_| ParseError {
                message: format!("Invalid seed: {}", raw),
            })?;
            Ok(Statement::Seed(seed))
        }
        Rule::pointcount_stmt => {
            let values = read_count_values(inner.into_inner())?;
            check_count_length(&values)?;
            Ok(Statement::PointCount(values))
        }

        Rule::altcount_stmt => {
            let mut parts = inner.into_inner();
            let row_pair = parts.next().ok_or_else(|| ParseError {
                message: "altcount needs a count number".to_string(),
            })?;
            let row: i64 = row_pair.as_str().parse().map_err(|_| ParseError {
                message: format!("Invalid altcount number: {}", row_pair.as_str()),
            })?;
            // The original accepts any number here and writes past the end of a
            // twelve-row table. That is a memory bug, not behaviour to copy.
            if !(0..NUM_COUNT_ROWS as i64).contains(&row) {
                return Err(ParseError {
                    message: format!(
                        "altcount {} is out of range: the counts are numbered 0 to {} \
                         (0 is hcp, 1 is controls, and 2 is pt0)",
                        row,
                        NUM_COUNT_ROWS - 1
                    ),
                });
            }
            let values = read_count_values(parts)?;
            check_count_length(&values)?;
            Ok(Statement::AltCount {
                row: row as usize,
                values,
            })
        }

        // `predeal` is handled by `build_statements`, which is the only thing
        // that can turn one written statement into several. Reaching here means
        // something called `build_statement` directly with one.
        Rule::csvrpt_stmt => Ok(Statement::CsvReport(build_csv_terms(inner)?)),
        Rule::printrpt_stmt => Ok(Statement::PrintReport(build_csv_terms(inner)?)),
        Rule::average_stmt => {
            // Standalone average statement: average "label"? expr
            let mut parts = inner.into_inner();
            let first = parts.next().unwrap();

            let (label, expr_pair) = if first.as_rule() == Rule::string_literal {
                let label_str = first.as_str();
                let label = label_str[1..label_str.len() - 1].to_string();
                (Some(label), parts.next().unwrap())
            } else {
                (None, first)
            };

            let expr = build_ast(expr_pair)?;

            Ok(Statement::Action {
                averages: vec![AverageSpec { label, expr }],
                frequencies: Vec::new(),
                format: None,
                printes: Vec::new(),
                print_hands: Vec::new(),
                print_reports: Vec::new(),
            })
        }
        Rule::frequency_stmt => {
            // Standalone frequency statement, in either dimension.
            let frequency = build_frequency(inner.into_inner())?;

            Ok(Statement::Action {
                averages: Vec::new(),
                frequencies: vec![frequency],
                format: None,
                printes: Vec::new(),
                print_hands: Vec::new(),
                print_reports: Vec::new(),
            })
        }
        Rule::print_stmt => {
            // Standalone print statement: printpbn, printall, printside(ns), etc.
            let action_type = action_type_from(inner)?;
            Ok(Statement::Action {
                averages: Vec::new(),
                frequencies: Vec::new(),
                format: Some(action_type),
                printes: Vec::new(),
                print_hands: Vec::new(),
                print_reports: Vec::new(),
            })
        }
        Rule::printes_stmt => {
            let spec = inner.into_inner().next().ok_or_else(|| ParseError {
                message: "printes with no list".to_string(),
            })?;
            Ok(Statement::Action {
                averages: Vec::new(),
                frequencies: Vec::new(),
                format: None,
                printes: vec![build_es_terms(spec)?],
                print_hands: Vec::new(),
                print_reports: Vec::new(),
            })
        }
        Rule::printhands_stmt => {
            let spec = inner.into_inner().next().ok_or_else(|| ParseError {
                message: "print with no seats".to_string(),
            })?;
            Ok(Statement::Action {
                averages: Vec::new(),
                frequencies: Vec::new(),
                format: None,
                printes: Vec::new(),
                print_hands: build_print_hands(spec)?,
                print_reports: Vec::new(),
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

/// Encode a contract token such as `x3N` or `x4Hxx` as a number.
///
/// The grammar has already checked the shape, so the only work here is the
/// arithmetic both references do: `level * 5 + strain`, plus 40 for each level
/// of doubling. The leading sigil carries no meaning and is skipped, exactly as
/// `make_contract` skips it in dealer.exe and in DealerV2_4.
fn contract_code(token: &str) -> Result<i32, ParseError> {
    let mut chars = token.chars();
    chars.next(); // the sigil, `x` or `z`

    let level = match chars.next().and_then(|c| c.to_digit(10)) {
        Some(level) => level as i32,
        None => {
            return Err(ParseError {
                message: format!("Contract has no level: {}", token),
            })
        }
    };

    let strain = match chars.next() {
        Some('C') => 0,
        Some('D') => 1,
        Some('H') => 2,
        Some('S') => 3,
        Some('N') => 4,
        _ => {
            return Err(ParseError {
                message: format!("Contract has no strain: {}", token),
            })
        }
    };

    let doubled = chars.filter(|c| *c == 'x').count() as i32;

    Ok(40 * doubled + level * 5 + strain)
}

/// The number a literal denotes, in the units the script is written in.
///
/// A plain integer is itself. A decimal is a hundred times itself — DealerV2_4's
/// `(int)(100. * atof(yytext))` — so `6.25` is 625 and `.5` is 50. The grammar
/// has already checked the shape, including that at most two digits follow the
/// point, so the arithmetic here cannot lose anything.
fn literal_value(text: &str) -> Result<i32, ParseError> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text),
    };

    let Some((whole, fraction)) = digits.split_once('.') else {
        return digits
            .parse::<i32>()
            .map(|n| sign * n)
            .map_err(|e| ParseError {
                message: format!("Invalid integer literal: {}", e),
            });
    };

    // "6." is six, and ".5" is five tenths; either side may be empty, never both.
    let whole: i32 = if whole.is_empty() {
        0
    } else {
        whole.parse().unwrap_or(0)
    };
    let hundredths: i32 = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i32>().unwrap_or(0) * 10,
        _ => fraction.parse::<i32>().unwrap_or(0),
    };

    Ok(sign * (whole * 100 + hundredths))
}

/// One `frequency`, in either spelling and either dimension.
///
/// The statement and the `action` component share `frequency_args` in the
/// grammar, and share this in the parser, so a change to one reaches both.
/// Shape: an optional label, then an expression and its two bounds, then
/// optionally a second expression and its two bounds.
fn build_frequency(pairs: pest::iterators::Pairs<Rule>) -> Result<FrequencySpec, ParseError> {
    let mut parts: Vec<_> = pairs.collect();

    // An optional label, then the rest.
    let label = match parts.first() {
        Some(p) if p.as_rule() == Rule::string_literal => {
            let text = parts.remove(0);
            let text = text.as_str();
            Some(text[1..text.len() - 1].to_string())
        }
        _ => None,
    };

    // The grammar allows exactly three or six of these: one expression and two
    // bounds, or two of each.
    let two_dimensional =
        match parts.len() {
            3 => false,
            6 => true,
            n => return Err(ParseError {
                message: format!(
                    "frequency takes an expression and two bounds, or two of each, not {} parts",
                    n
                ),
            }),
        };

    let bound = |pair: &pest::iterators::Pair<Rule>, what: &str| -> Result<i32, ParseError> {
        literal_value(pair.as_str()).map_err(|_| ParseError {
            message: format!("Invalid frequency range {}: {}", what, pair.as_str()),
        })
    };

    let expr = build_ast(parts[0].clone())?;
    let range = Some((bound(&parts[1], "min")?, bound(&parts[2], "max")?));

    let second = if two_dimensional {
        let expr2 = build_ast(parts[3].clone())?;
        Some((
            expr2,
            (
                bound(&parts[4], "second min")?,
                bound(&parts[5], "second max")?,
            ),
        ))
    } else {
        None
    };

    Ok(FrequencySpec {
        label,
        expr,
        range,
        second,
    })
}

/// A compass word as a `Position`, in any of the language's spellings.
fn compass_position(text: &str) -> Result<Position, ParseError> {
    match text.to_lowercase().as_str() {
        "north" | "n" => Ok(Position::North),
        "south" | "s" => Ok(Position::South),
        "east" | "e" => Ok(Position::East),
        "west" | "w" => Ok(Position::West),
        other => Err(ParseError {
            message: format!("Invalid compass: {}", other),
        }),
    }
}

/// The action a `print...` word names, in either of its spellings.
///
/// `printside(ns)` and `printns` are one action, as are `printside(ew)` and
/// `printew`, so both forms resolve to the same `ActionType` here rather than
/// being carried separately through the rest of the program.
fn action_type_from(pair: pest::iterators::Pair<Rule>) -> Result<ActionType, ParseError> {
    let text = pair.as_str();
    let inner = match pair.as_rule() {
        Rule::action_type | Rule::print_stmt => {
            pair.into_inner().next().ok_or_else(|| ParseError {
                message: format!("Empty action: {}", text),
            })?
        }
        _ => pair,
    };

    match inner.as_rule() {
        Rule::printside_spec => {
            let side = inner.into_inner().next().ok_or_else(|| ParseError {
                message: "printside needs a side".to_string(),
            })?;
            match side.as_str().to_lowercase().as_str() {
                "ns" => Ok(ActionType::PrintNS),
                "ew" => Ok(ActionType::PrintEW),
                other => Err(ParseError {
                    message: format!("Unknown side: {}", other),
                }),
            }
        }
        _ => ActionType::parse(inner.as_str()).ok_or_else(|| ParseError {
            message: format!("Invalid action type: {}", inner.as_str()),
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
                Rule::neg_op => {
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

        Rule::par_call => {
            // `par_name` then the side. A side word becomes the number the
            // evaluator reads — 0 for North-South, 1 for East-West — so a
            // compass or a computed value works in the same slot.
            let mut args = Vec::new();
            for arg in pair.into_inner() {
                match arg.as_rule() {
                    Rule::par_name => continue,
                    Rule::side => {
                        let side = arg.as_str().to_lowercase();
                        args.push(Expr::Literal(i32::from(side == "ew")));
                    }
                    _ => args.push(build_ast(arg)?),
                }
            }
            Ok(Expr::call_multi(Function::Par, args))
        }

        Rule::score_call => {
            // score_name, then the three arguments. The first two may have
            // arrived as tokens, which the rules below have already turned
            // into the numbers they stand for.
            let mut args = Vec::new();
            for arg_pair in pair.into_inner() {
                if arg_pair.as_rule() == Rule::score_name {
                    continue;
                }
                args.push(build_ast(arg_pair)?);
            }

            Ok(Expr::call_multi(Function::Score, args))
        }

        Rule::contract_token => {
            // Both references encode a contract as level * 5 + strain, plus 40
            // per level of doubling, and dealer3 uses the same numbers so there
            // is one encoding rather than two.
            Ok(Expr::Literal(contract_code(pair.as_str())?))
        }

        Rule::vuln_token => {
            // NON_VUL and VUL in tree.h are 0 and 1, which is what `score`
            // already reads.
            Ok(Expr::Literal(i32::from(pair.as_str() == "vul")))
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

        Rule::denomination_word => {
            // dealer.exe resolves NOTRUMPS to 4 in its own grammar, and dealer3
            // numbers strains the same way, so the word simply is that number.
            Ok(Expr::Literal(4))
        }

        Rule::literal => Ok(Expr::Literal(literal_value(pair.as_str())?)),

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

/// A suit length written as one character: `0`-`9`, then `:;<=` for ten to
/// thirteen. Past `9` the digits run on through ASCII, which is what the
/// original does internally when it fills a wildcard.
fn shape_len(ch: char) -> Option<u8> {
    match ch {
        '0'..='9' => Some(ch as u8 - b'0'),
        ':' | ';' | '<' | '=' => Some(ch as u8 - b'0'),
        _ => None,
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
            } else if let Some(length) = shape_len(ch) {
                if length > 13 {
                    return Err(ParseError {
                        message: format!("Shape length {} is too large (max 13)", length),
                    });
                }
                pattern[i] = Some(length);
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
            let Some(length) = shape_len(ch) else {
                return Err(ParseError {
                    message: format!("Invalid character in shape: {}", ch),
                });
            };
            if length > 13 {
                return Err(ParseError {
                    message: format!("Shape length {} is too large (max 13)", length),
                });
            }
            pattern[i] = length;
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

    /// One `predeal` naming several seats, which is dealer.exe's
    /// `predealargs: predealarg | predealargs predealarg`.
    ///
    /// Each seat becomes its own statement rather than the AST growing a list,
    /// so every consumer that walks the statements keeps working.
    #[test]
    fn predeal_may_name_more_than_one_seat() {
        let program = parse_program("predeal north SAKQ south SJ32").unwrap();
        let seats: Vec<_> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Predeal { position, cards } => Some((*position, cards.len())),
                _ => None,
            })
            .collect();
        assert_eq!(seats, vec![(Position::North, 3), (Position::South, 3)]);
    }

    /// The holdings of one seat are comma-separated and the seats are not,
    /// which is the only thing telling them apart.
    #[test]
    fn each_seat_keeps_its_own_comma_separated_holdings() {
        let program = parse_program("predeal north SAKQ,HT98 east DA south SJ32").unwrap();
        let seats: Vec<_> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Predeal { position, cards } => Some((*position, cards.len())),
                _ => None,
            })
            .collect();
        assert_eq!(
            seats,
            vec![
                (Position::North, 6),
                (Position::East, 1),
                (Position::South, 3)
            ]
        );
    }

    /// The case the grammar has to get right: `S` is both a void in spades and
    /// an abbreviation for South. Matching the comma-list before trying another
    /// seat is what keeps this one seat rather than two.
    #[test]
    fn a_void_holding_is_not_read_as_the_south_seat() {
        let program = parse_program("predeal north S,HAKQ south SJ32").unwrap();
        let seats: Vec<_> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Predeal { position, cards } => Some((*position, cards.len())),
                _ => None,
            })
            .collect();
        // North: nothing in spades, three hearts. South: three spades.
        assert_eq!(seats, vec![(Position::North, 3), (Position::South, 3)]);
    }

    /// Naming a seat twice accumulates, as the original's repeated reduction of
    /// `predeal_holding(compass, ...)` does.
    #[test]
    fn a_seat_may_be_named_more_than_once() {
        let program = parse_program("predeal north SAKQ north HAK").unwrap();
        let seats: Vec<_> = program
            .statements
            .iter()
            .filter_map(|s| match s {
                Statement::Predeal { position, cards } => Some((*position, cards.len())),
                _ => None,
            })
            .collect();
        assert_eq!(seats, vec![(Position::North, 3), (Position::North, 2)]);
    }

    /// Still one statement when only one seat is named.
    #[test]
    fn one_seat_is_still_one_statement() {
        let program = parse_program("predeal north SAKQ").unwrap();
        assert_eq!(
            program
                .statements
                .iter()
                .filter(|s| matches!(s, Statement::Predeal { .. }))
                .count(),
            1
        );
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
                ..
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

/// Where the script's condition is written, as byte offsets into the source.
///
/// The last one wins, as in the original: `def: expr` sets the decision tree
/// afresh each time it reduces, so a script with two of them is filtered by the
/// second. Both spellings count — `condition <expr>` and a bare expression, the
/// form every practice scenario uses.
///
/// Wanted so that levelling can add `and levelTheDeal` to a script that has no
/// placeholder for it, without guessing where the condition ends by looking for
/// the next keyword.
pub struct ConditionSpan {
    /// Where the statement begins: the `condition` keyword, or the expression
    /// itself when the condition is a bare expression.
    ///
    /// Distinct from `expr` because the two are not always on the same line.
    /// `condition` alone on one line with its expression on the next is common
    /// in the wild, and anything inserted "before the condition" has to go
    /// before the keyword — putting it after leaves the keyword looking for an
    /// expression and finding the first line of whatever was inserted.
    pub statement: usize,
    /// Where the expression begins.
    pub expr: usize,
    /// Where the expression ends, with trailing whitespace trimmed off.
    pub end: usize,
}

pub fn condition_span(input: &str) -> Option<ConditionSpan> {
    let pairs = ConstraintParser::parse(Rule::program, input).ok()?;
    let mut found = None;
    for pair in pairs {
        if pair.as_rule() == Rule::EOI {
            continue;
        }
        for statement in pair.into_inner() {
            if statement.as_rule() != Rule::dealer_statement {
                continue;
            }
            let Some(inner) = statement.into_inner().next() else {
                continue;
            };
            match inner.as_rule() {
                // `condition <expr>`: the expression alone, so the keyword stays.
                Rule::condition_stmt => {
                    let statement = inner.as_span().start();
                    if let Some(expr) = inner.into_inner().next() {
                        let span = expr.as_span();
                        let (expr_start, end) = trimmed(input, span.start(), span.end());
                        found = Some(ConditionSpan {
                            statement,
                            expr: expr_start,
                            end,
                        });
                    }
                }
                // A bare expression is a condition too, and is its own start.
                Rule::expr => {
                    let span = inner.as_span();
                    let (expr_start, end) = trimmed(input, span.start(), span.end());
                    found = Some(ConditionSpan {
                        statement: expr_start,
                        expr: expr_start,
                        end,
                    });
                }
                _ => {}
            }
        }
    }
    found
}

/// An expression is not an atomic rule, so its span swallows the whitespace
/// that follows it. Anything appended wants to land against the expression
/// rather than after a newline.
fn trimmed(input: &str, start: usize, end: usize) -> (usize, usize) {
    let end = input[start..end].trim_end().len() + start;
    (start, end)
}

#[cfg(test)]
mod condition_span_tests {
    use super::condition_span;

    fn text(source: &str) -> &str {
        let span = condition_span(source).expect("a condition");
        &source[span.expr..span.end]
    }

    /// What the statement position is for: everything before it can be inserted
    /// ahead of the condition without splitting the keyword from its expression.
    fn before(source: &str) -> &str {
        let span = condition_span(source).expect("a condition");
        &source[span.statement..]
    }

    #[test]
    fn it_finds_the_keyword_form_without_the_keyword() {
        assert_eq!(text("condition hcp(north) > 10\n"), "hcp(north) > 10");
    }

    #[test]
    fn it_finds_a_bare_expression() {
        // What every practice scenario writes.
        assert_eq!(
            text("nt = 1\nnt and hcp(south) > 5\n"),
            "nt and hcp(south) > 5"
        );
    }

    #[test]
    fn it_spans_several_lines() {
        let source = "condition\n  hcp(north) > 10\n  and hcp(south) > 10\naction printall\n";
        assert_eq!(text(source), "hcp(north) > 10\n  and hcp(south) > 10");
    }

    #[test]
    fn the_last_one_wins() {
        assert_eq!(
            text("condition hcp(north) > 1\ncondition hcp(south) > 2\n"),
            "hcp(south) > 2"
        );
    }

    /// The bug this exists for: `condition` alone on a line, its expression on
    /// the next. Inserting at the expression's line puts text between the two,
    /// and the keyword then reads the first line of it as its condition.
    #[test]
    fn the_statement_starts_at_the_keyword_not_the_expression() {
        let source = "condition\n  hcp(north) > 10\naction printall\n";
        assert!(
            before(source).starts_with("condition"),
            "got {:?}",
            before(source)
        );
    }

    #[test]
    fn a_bare_expression_starts_at_itself() {
        assert!(before("nt = 1\nnt and hcp(south) > 5\n").starts_with("nt and"));
    }

    #[test]
    fn a_script_with_no_condition_has_no_span() {
        assert!(condition_span("produce 5\naction printall\n").is_none());
    }
}
