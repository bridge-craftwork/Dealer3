//! dealer3's `shape{...}` expansion against the reference implementation's own.
//!
//! Every case here is copied from `docs/FD_Shapes_examples.txt` in the
//! DealerV2_4 distribution — François Dellacherie's shape source on one line
//! and what his Perl pre-processor prints for it on the next. They are the
//! edge cases its author thought worth recording, which makes them the best
//! conformance suite available.
//!
//! The comparison is between the sets of distributions each denotes, not the
//! text. dealer3 groups its patterns differently — it works out the set and
//! then generalises, rather than developing patterns and reducing them — so the
//! two agree on meaning and rarely on wording.
//!
//! One difference is real and deliberate, and the test pins it down rather than
//! papering over it: a pattern is one character per suit, so `fdp` cannot write
//! a ten-card suit and quietly drops every shape holding one. `5+Mxxx` means a
//! ten-card major as much as a five-card one. dealer3 writes those shapes,
//! `:;<=` standing for ten through thirteen, so its answer is a superset and
//! the difference is never anything else.

use dealer_parser::{preprocess_all, ScriptParams};
use std::collections::BTreeSet;

type Shape = [u8; 4];

fn all_shapes() -> Vec<Shape> {
    let mut out = Vec::with_capacity(560);
    for s in 0..=13u8 {
        for h in 0..=13 - s {
            for d in 0..=13 - s - h {
                out.push([s, h, d, 13 - s - h - d]);
            }
        }
    }
    out
}

/// The distributions a `shape(...)` term list covers.
fn shapes_of_patterns(list: &str) -> BTreeSet<Shape> {
    let mut out = BTreeSet::new();
    for term in list.split('+') {
        let term = term.trim().trim_start_matches("%s");
        if term.is_empty() {
            continue;
        }
        let chars: Vec<char> = term.chars().collect();
        assert_eq!(chars.len(), 4, "`{term}` is not four characters");
        for shape in all_shapes() {
            if chars
                .iter()
                .zip(shape)
                .all(|(c, n)| *c == 'x' || *c == 'X' || (*c as u8).wrapping_sub(b'0') == n)
            {
                out.insert(shape);
            }
        }
    }
    out
}

/// What dealer3 makes of an FD shape body.
fn dealer3(body: &str) -> BTreeSet<Shape> {
    let expanded = preprocess_all(
        &format!("condition shape{{north, {body}}}\n"),
        &ScriptParams::default(),
    )
    .unwrap_or_else(|e| panic!("`{body}` should expand: {e}"));
    let open = expanded.find("shape(").expect("a shape call");
    let close = expanded[open..].find(')').expect("a closing paren") + open;
    let inner = &expanded[open + "shape(".len()..close];
    let list = inner.split_once(',').expect("a compass").1;
    shapes_of_patterns(list)
}

/// `body`, and the expansion the reference prints for it.
const CASES: &[(&str, &str)] = &[
    (
        // from DealerV2_4 docs/FD_Shapes_examples.txt
        "4+M6-m(xx):c<d,h+s>=10 + x[5-9]x[23]",
        "1921 + 1930 + 2821 + 2830 + 2920 + 3721 + 3730 + 3820 + 3910 + 4621 + 4630 + 4720 + 4810 + 5521 + 5530 + 5620 + 5710 + 6421 + 6430 + 6520 + 6610 + 7321 + 7330 + 7420 + 7510 + 8221 + 8230 + 8320 + 8410 + 9121 + 9130 + 9220 + 9310 + x5x2 + x5x3 + x6x2 + x6x3 + x7x2 + x7x3 + x8x2 + x8x3 + x9x2 + x9x3",
    ),
    (
        // from DealerV2_4 docs/FD_Shapes_examples.txt
        "4+s4+h(xx):d>c,h+s==10",
        "4621 + 4630 + 5521 + 5530 + 6421 + 6430",
    ),
    (
        // from DealerV2_4 docs/FD_Shapes_examples.txt
        "5+Mxxx:s>=d,s>=c or h>=d,h>=c",
        "0535 + 0544 + 0553 + 0616 + 0625 + 0634 + 0643 + 0652 + 0661 + 0706 + 0715 + 0724 + 0733 + 0742 + 0751 + 0760 + 0805 + 0814 + 0823 + 0832 + 0841 + 0850 + 0904 + 0913 + 0922 + 0931 + 0940 + 1525 + 1534 + 1543 + 1552 + 1606 + 1615 + 1624 + 1633 + 1642 + 1651 + 1660 + 1705 + 1714 + 1723 + 1732 + 1741 + 1750 + 1804 + 1813 + 1822 + 1831 + 1840 + 1903 + 1912 + 1921 + 1930 + 2515 + 2524 + 2533 + 2542 + 2551 + 2605 + 2614 + 2623 + 2632 + 2641 + 2650 + 2704 + 2713 + 2722 + 2731 + 2740 + 2803 + 2812 + 2821 + 2830 + 2902 + 2911 + 2920 + 3505 + 3514 + 3523 + 3532 + 3541 + 3550 + 3604 + 3613 + 3622 + 3631 + 3640 + 3703 + 3712 + 3721 + 3730 + 3802 + 3811 + 3820 + 3901 + 3910 + 4504 + 4513 + 4522 + 4531 + 4540 + 4603 + 4612 + 4621 + 4630 + 4702 + 4711 + 4720 + 4801 + 4810 + 4900 + 5035 + 5044 + 5053 + 5125 + 5134 + 5143 + 5152 + 5215 + 5224 + 5233 + 5242 + 5251 + 5305 + 5314 + 5323 + 5332 + 5341 + 5350 + 5404 + 5413 + 5422 + 5431 + 5440 + 5503 + 5512 + 5521 + 5530 + 5602 + 5611 + 5620 + 5701 + 5710 + 5800 + 6016 + 6025 + 6034 + 6043 + 6052 + 6061 + 6106 + 6115 + 6124 + 6133 + 6142 + 6151 + 6160 + 6205 + 6214 + 6223 + 6232 + 6241 + 6250 + 6304 + 6313 + 6322 + 6331 + 6340 + 6403 + 6412 + 6421 + 6430 + 6502 + 6511 + 6520 + 6601 + 6610 + 6700 + 7006 + 7015 + 7024 + 7033 + 7042 + 7051 + 7060 + 7105 + 7114 + 7123 + 7132 + 7141 + 7150 + 7204 + 7213 + 7222 + 7231 + 7240 + 7303 + 7312 + 7321 + 7330 + 7402 + 7411 + 7420 + 7501 + 7510 + 7600 + 8005 + 8014 + 8023 + 8032 + 8041 + 8050 + 8104 + 8113 + 8122 + 8131 + 8140 + 8203 + 8212 + 8221 + 8230 + 8302 + 8311 + 8320 + 8401 + 8410 + 8500 + 9004 + 9013 + 9022 + 9031 + 9040 + 9103 + 9112 + 9121 + 9130 + 9202 + 9211 + 9220 + 9301 + 9310 + 9400",
    ),
    (
        // from DealerV2_4 docs/FD_Shapes_examples.txt
        "7+xxx:h<2 or d<2 or c<2",
        "7006 + 7015 + 7024 + 7033 + 7042 + 7051 + 7060 + 7105 + 7114 + 7123 + 7132 + 7141 + 7150 + 7204 + 7213 + 7231 + 7240 + 7303 + 7312 + 7321 + 7330 + 7402 + 7411 + 7420 + 7501 + 7510 + 7600 + 8005 + 8014 + 8023 + 8032 + 8041 + 8050 + 8104 + 8113 + 8122 + 8131 + 8140 + 8203 + 8212 + 8221 + 8230 + 8302 + 8311 + 8320 + 8401 + 8410 + 8500 + 9004 + 9013 + 9022 + 9031 + 9040 + 9103 + 9112 + 9121 + 9130 + 9202 + 9211 + 9220 + 9301 + 9310 + 9400",
    ),
    (
        // from DealerV2_4 docs/FD_Shapes_examples.txt
        "7+xxx:h<2 or m<2",
        "7006 + 7015 + 7024 + 7033 + 7042 + 7051 + 7060 + 7105 + 7114 + 7123 + 7132 + 7141 + 7150 + 7204 + 7213 + 7231 + 7240 + 7303 + 7312 + 7321 + 7330 + 7402 + 7411 + 7420 + 7501 + 7510 + 7600 + 8005 + 8014 + 8023 + 8032 + 8041 + 8050 + 8104 + 8113 + 8122 + 8131 + 8140 + 8203 + 8212 + 8221 + 8230 + 8302 + 8311 + 8320 + 8401 + 8410 + 8500 + 9004 + 9013 + 9022 + 9031 + 9040 + 9103 + 9112 + 9121 + 9130 + 9202 + 9211 + 9220 + 9301 + 9310 + 9400",
    ),
    (
        // from DealerV2_4 docs/FD_Shapes_examples.txt
        "4M+4m-(xx):c<d,h+s>=8 + x[5-9]x[23]",
        "0832 + 0841 + 0850 + 0931 + 0940 + 1732 + 1741 + 1750 + 1831 + 1840 + 1921 + 1930 + 2632 + 2641 + 2650 + 2731 + 2740 + 2821 + 2830 + 2920 + 3532 + 3541 + 3550 + 3631 + 3640 + 3721 + 3730 + 3820 + 3910 + 4432 + 4441 + 4450 + 4531 + 4540 + 4621 + 4630 + 4720 + 4810 + 5332 + 5341 + 5350 + 5431 + 5440 + 5521 + 5530 + 5620 + 5710 + 6232 + 6241 + 6250 + 6331 + 6340 + 6421 + 6430 + 6520 + 6610 + 7132 + 7141 + 7150 + 7231 + 7240 + 7321 + 7330 + 7420 + 7510 + 8032 + 8041 + 8050 + 8131 + 8140 + 8221 + 8230 + 8320 + 8410 + 9031 + 9040 + 9121 + 9130 + 9220 + 9310 + x5x2 + x5x3 + x6x2 + x6x3 + x7x2 + x7x3 + x8x2 + x8x3 + x9x2 + x9x3",
    ),
];

#[test]
fn every_reference_expansion_agrees() {
    for (body, expected) in CASES {
        let ours = dealer3(body);
        let theirs = shapes_of_patterns(expected);

        let missing: Vec<Shape> = theirs.difference(&ours).copied().collect();
        assert!(
            missing.is_empty(),
            "`{body}`: dealer3 leaves out {missing:?}, which the reference includes"
        );

        let extra: Vec<Shape> = ours.difference(&theirs).copied().collect();
        assert!(
            extra.iter().all(|s| s.iter().any(|n| *n >= 10)),
            "`{body}`: dealer3 adds {extra:?}, and only ten-card suits are expected"
        );
    }
}

/// The other half of the claim above: where the reference can express itself
/// fully, the two agree exactly.
#[test]
fn they_agree_exactly_where_no_suit_runs_past_nine() {
    for (body, expected) in CASES {
        let ours: BTreeSet<Shape> = dealer3(body)
            .into_iter()
            .filter(|s| s.iter().all(|n| *n < 10))
            .collect();
        assert_eq!(
            ours,
            shapes_of_patterns(expected),
            "`{body}` should match the reference once ten-card suits are set aside"
        );
    }
}
