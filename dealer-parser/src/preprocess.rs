use regex::Regex;

/// Preprocess input to mark 4-digit numbers inside shape() functions
/// with %s prefix so they can be distinguished from regular literals
///
/// Example: "shape(north, 5242)" becomes "shape(north, %s5242)"
/// Example: "shape(north, any 4333 - 4333)" becomes "shape(north, any 4333 - %s4333)"
/// Everything that happens to a script before it is parsed.
///
/// Two passes, and the order between them matters: the François Dellacherie
/// shapes expand into exactly the four-digit literals the second pass exists to
/// mark, so they have to be written before it runs.
pub fn preprocess_all(input: &str) -> Result<String, String> {
    Ok(preprocess(&crate::fdshape::expand(input)?))
}

pub fn preprocess(input: &str) -> String {
    // Mark all 4-digit numbers that appear inside shape() function calls
    // (except those following "any " keyword)
    //
    // Strategy: Find shape(...) blocks and mark 4-digit numbers that:
    // 1. Are NOT preceded by "any "
    // 2. Are NOT already marked with %s
    // 3. Are standalone (not part of wildcards like 54xx)

    let shape_re = Regex::new(r"shape\s*\([^)]+\)").unwrap();

    shape_re
        .replace_all(input, |caps: &regex::Captures| {
            let shape_call = &caps[0];

            // Match 4-digit numbers, capturing position info
            let digit_re = Regex::new(r"\b(\d{4})\b").unwrap();

            digit_re
                .replace_all(shape_call, |inner_caps: &regex::Captures| {
                    let digits = &inner_caps[1];
                    let match_start = inner_caps.get(0).unwrap().start();

                    // Check if this 4-digit number follows "any "
                    let before_match = &shape_call[..match_start];
                    if before_match.ends_with("any ") {
                        // Don't mark it - "any" disambiguates
                        digits.to_string()
                    } else {
                        // Mark it with %s prefix
                        format!("%s{}", digits)
                    }
                })
                .to_string()
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocess_shape_function() {
        assert_eq!(preprocess("shape(north, 5242)"), "shape(north, %s5242)");
    }

    #[test]
    fn test_preprocess_shape_with_any() {
        assert_eq!(
            preprocess("shape(north, any 4333)"),
            "shape(north, any 4333)" // 'any' prevents marking
        );
    }

    #[test]
    fn test_preprocess_preserves_regular_numbers() {
        assert_eq!(
            preprocess("cccc(north) >= 1500"),
            "cccc(north) >= 1500" // Not in shape(), not marked
        );
    }

    #[test]
    fn test_preprocess_multiple_shapes() {
        assert_eq!(
            preprocess("shape(north, 5332) && shape(south, 4441)"),
            "shape(north, %s5332) && shape(south, %s4441)"
        );
    }

    #[test]
    fn test_preprocess_mixed_expression() {
        assert_eq!(
            preprocess("cccc(north) >= 1500 && shape(north, 5332)"),
            "cccc(north) >= 1500 && shape(north, %s5332)"
        );
    }

    #[test]
    fn test_preprocess_shape_exclusion() {
        assert_eq!(
            preprocess("shape(north, any 4333 - 4333)"),
            "shape(north, any 4333 - %s4333)" // Only mark 4333 after -, not after "any "
        );
    }

    #[test]
    fn test_preprocess_shape_combination() {
        assert_eq!(
            preprocess("shape(north, any 4333 + 5242 - 4441)"),
            "shape(north, any 4333 + %s5242 - %s4441)"
        );
    }
}
