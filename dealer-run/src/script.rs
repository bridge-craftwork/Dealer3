//! Deal from a script, for a caller that wants the deals and nothing else.
//!
//! [`run`](crate::run::run) is the engine: it takes a predeal, a
//! vulnerability, a budget and a host to hand deals to, all of which the two
//! front ends work out from a script's statements and their own arguments.
//! A third caller that only wants "the deals this script produces, for this
//! seed" would have to copy that working-out, and then keep its copy in step.
//! This is that working-out, once.
//!
//! What it reads from the script is what the command line reads: `predeal`,
//! `dealer`, `vulnerable`, `produce` and `generate`. What it leaves alone is
//! everything about output: `action`, `printes` and the statistics are the
//! caller's to ask [`run`](crate::run::run) for if it wants them. Nothing is
//! levelled and nothing is solved that the script's condition does not ask for.

use dealer_core::{Deal, FastDealConfig, Position, SwapMode};
use dealer_parser::{ScriptParams, Statement, VulnerabilityType};

use crate::run::{run, Deals, Phase, Produced, RunHost, RunOptions, RunReport};

/// How many deals a script produces when neither it nor the caller says:
/// dealer's own default.
pub const DEFAULT_PRODUCE: usize = 40;
/// How many deals a run may look at when neither the script nor the caller
/// says: dealer's own default.
pub const DEFAULT_GENERATE: usize = 10_000_000;

/// What the caller chooses about a run. Everything else is the script's.
#[derive(Debug, Clone, Default)]
pub struct ScriptOptions {
    pub seed: u32,
    /// Deals to produce. `None`: the script's `produce`, else
    /// [`DEFAULT_PRODUCE`].
    pub produce: Option<usize>,
    /// Deals the run may look at. `None`: the script's `generate`, else
    /// [`DEFAULT_GENERATE`].
    pub max_generate: Option<usize>,
    /// What `$1` and friends stand for.
    pub params: ScriptParams,
    /// Threads to deal on: 0 asks the machine, 1 stays on this one. It cannot
    /// change which deals come out.
    pub threads: usize,
}

/// The deals a script produced, and what the script says of every one of them.
pub struct ScriptDeals {
    /// In the order they were produced. With each, the script's hand type it
    /// matched, where the script declares hand types.
    pub deals: Vec<(Deal, Option<String>)>,
    /// The script's `dealer`, if it names one.
    pub dealer: Option<Position>,
    /// The script's `vulnerable`, if it names one.
    pub vulnerability: Option<VulnerabilityType>,
    /// The run's own account: how many deals it looked at, whether the budget
    /// ran out before `produce` was met.
    pub report: RunReport,
}

struct Collect {
    deals: Vec<(Deal, Option<usize>)>,
}

impl RunHost for Collect {
    fn should_stop(&mut self, _: Phase, _: usize, _: usize, _: usize) -> bool {
        false
    }
    fn produced(&mut self, produced: &Produced) -> Result<(), String> {
        self.deals.push((produced.deal.clone(), produced.hand_type));
        Ok(())
    }
}

/// The deals `script` produces for `opts.seed`.
///
/// Byte for byte the deals the command line deals for the same script, seed,
/// `-p` and `-g`: it is the same engine, given the same things.
pub fn deal_script(script: &str, opts: ScriptOptions) -> Result<ScriptDeals, String> {
    let preprocessed = dealer_parser::preprocess_all(script, &opts.params)?;
    let program =
        dealer_parser::parse_program(&preprocessed).map_err(|e| format!("Parse error: {e}"))?;

    let mut predeal = FastDealConfig::new();
    let (mut dealer, mut vulnerability) = (None, None);
    let (mut produce, mut generate) = (None, None);
    for statement in &program.statements {
        match statement {
            Statement::Dealer(position) => dealer = Some(*position),
            Statement::Vulnerable(v) => vulnerability = Some(*v),
            Statement::Produce(n) => produce = Some(*n),
            Statement::Generate(n) => generate = Some(*n),
            Statement::Predeal { position, cards } => predeal
                .predeal(*position, cards)
                .map_err(|e| format!("Predeal error: {e}"))?,
            _ => {}
        }
    }

    let mut host = Collect { deals: Vec::new() };
    let report = run(
        script,
        RunOptions {
            seed: opts.seed,
            produce: opts.produce.or(produce).unwrap_or(DEFAULT_PRODUCE),
            max_generate: opts.max_generate.or(generate).unwrap_or(DEFAULT_GENERATE),
            deals: Deals::Shuffled {
                predeal,
                swap: SwapMode::None,
            },
            leveling: None,
            round_robin: false,
            threads: opts.threads,
            batch: 0,
            params: opts.params,
            vulnerability: match vulnerability {
                Some(VulnerabilityType::NS) => dealer_core::Vulnerability::NorthSouth,
                Some(VulnerabilityType::EW) => dealer_core::Vulnerability::EastWest,
                Some(VulnerabilityType::All) => dealer_core::Vulnerability::Both,
                Some(VulnerabilityType::None) | None => dealer_core::Vulnerability::None,
            },
        },
        &mut host,
    )
    .map_err(|e| e.to_string())?;

    let labels: Vec<&str> = report
        .hand_types
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    let deals = host
        .deals
        .into_iter()
        .map(|(deal, kind)| {
            (
                deal,
                kind.and_then(|i| labels.get(i).map(|s| s.to_string())),
            )
        })
        .collect();
    Ok(ScriptDeals {
        deals,
        dealer,
        vulnerability,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(seed: u32, produce: Option<usize>) -> ScriptOptions {
        ScriptOptions {
            seed,
            produce,
            threads: 1,
            ..Default::default()
        }
    }

    #[test]
    fn produces_what_the_script_asks_for_and_only_matching_deals() {
        let script = "dealer west\nvulnerable NS\ncondition hcp(north) >= 15\nproduce 7\n";
        let dealt = deal_script(script, options(1, None)).unwrap();
        assert_eq!(dealt.deals.len(), 7);
        assert_eq!(dealt.report.produced, 7);
        assert_eq!(dealt.dealer, Some(Position::West));
        assert!(matches!(dealt.vulnerability, Some(VulnerabilityType::NS)));
        assert!(!dealt.report.hit_limit);
    }

    #[test]
    fn the_caller_may_ask_for_a_different_number() {
        let script = "condition hcp(north) >= 15\nproduce 7\n";
        assert_eq!(
            deal_script(script, options(1, Some(3)))
                .unwrap()
                .deals
                .len(),
            3
        );
        // With neither, dealer's own default.
        let bare = deal_script("condition hcp(north) >= 15\n", options(1, None)).unwrap();
        assert_eq!(bare.deals.len(), DEFAULT_PRODUCE);
    }

    #[test]
    fn the_same_seed_deals_the_same_deals_and_another_seed_does_not() {
        let script = "condition hcp(north) >= 15\nproduce 5\n";
        let show = |seed| {
            deal_script(script, options(seed, None))
                .unwrap()
                .deals
                .iter()
                .map(|(deal, _)| format!("{deal:?}"))
                .collect::<Vec<_>>()
        };
        assert_eq!(show(42), show(42));
        assert_ne!(show(42), show(43));
    }

    #[test]
    fn a_script_that_does_not_parse_says_so() {
        let Err(err) = deal_script("condition hcp(north) >= >= 15\n", options(1, None)) else {
            panic!("the script should not parse")
        };
        assert!(err.starts_with("Parse error"), "{err}");
    }

    #[test]
    fn a_budget_too_small_is_reported_not_hidden() {
        let script = "condition hcp(north) >= 30\nproduce 5\n";
        let dealt = deal_script(
            script,
            ScriptOptions {
                seed: 1,
                max_generate: Some(2_000),
                threads: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(dealt.deals.len() < 5);
        assert!(dealt.report.hit_limit);
    }
}
