//! Noisy-OR risk-scoring model for SPL Token / Token-2022 mints. Risk
//! factors here are independent failure modes, not additive contributions,
//! so they combine as a competing-risks hazard (equivalently, noisy-OR):
//! `Λ = Σ -ln(1 - p_i)`, `score = 100 * (1 - exp(-Λ))`. This is what lets
//! one catastrophic signal (e.g. a permanent delegate) dominate the score
//! regardless of otherwise-clean signals, while several independently
//! moderate signals still compound — a property a plain `max()` over
//! factors would miss. Full derivation: see the design spec.

use crate::mint::MintExtensions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Green,
    Amber,
    Red,
}

impl Verdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Green => "green",
            Verdict::Amber => "amber",
            Verdict::Red => "red",
        }
    }
}

/// Percentage of total supply held by the top N accounts from
/// `getTokenLargestAccounts`, on a 0-100 scale. This RPC method returns at
/// most the top 20 holders, so `top20_pct` is a lower bound on true
/// concentration, never an exact figure: a low reading says little (the
/// remaining holders are invisible to us), a high reading is a hard floor.
#[derive(Debug, Clone, Copy)]
pub struct ConcentrationInput {
    pub top1_pct: f64,
    pub top5_pct: f64,
    pub top10_pct: f64,
    pub top20_pct: f64,
}

/// Combine authority, extension, and concentration signals into a single
/// 0-100 score and a verdict band. `amber_threshold`/`red_threshold` are
/// operator-configurable (see `token-risk-check`'s config, Task 8).
///
/// `concentration` is optional because `getTokenLargestAccounts` is an
/// expensive scan that public RPC endpoints throttle outright, so a caller
/// can genuinely reach this point having measured the authorities and the
/// extensions but not the holders. Passing `None` invents no probability
/// for the missing signal - the score stays an honest reading of what was
/// actually measured - and instead carries the uncertainty in the verdict,
/// which can then never come back green.
pub fn score(
    mint_authority_present: bool,
    freeze_authority_present: bool,
    extensions: &MintExtensions,
    concentration: Option<&ConcentrationInput>,
    amber_threshold: f64,
    red_threshold: f64,
) -> (f64, Verdict) {
    let mut probabilities: Vec<f64> = Vec::new();
    if mint_authority_present {
        probabilities.push(0.15);
    }
    if freeze_authority_present {
        probabilities.push(0.10);
    }
    if extensions.permanent_delegate {
        probabilities.push(0.90);
    }
    if extensions.transfer_hook {
        probabilities.push(0.50);
    }
    if extensions.transfer_fee_config {
        probabilities.push(0.20);
    }
    if extensions.default_account_state_frozen {
        probabilities.push(0.35);
    }
    if extensions.non_transferable {
        probabilities.push(0.05);
    }
    if extensions.confidential_transfer {
        probabilities.push(0.05);
    }
    if let Some(c) = concentration {
        probabilities.push(concentration_probability(c));
    }

    let hazard: f64 = probabilities.iter().map(|p| -(1.0 - p).ln()).sum();
    let value = 100.0 * (1.0 - (-hazard).exp());

    let mut verdict = if value >= red_threshold {
        Verdict::Red
    } else if value >= amber_threshold {
        Verdict::Amber
    } else {
        Verdict::Green
    };
    // An unmeasured signal is not an absent one. Green is this tool's only
    // affirmative claim, so it is the one verdict a partial reading may not
    // make; amber sends it to a human, which is where an unknown belongs.
    if concentration.is_none() && verdict == Verdict::Green {
        verdict = Verdict::Amber;
    }
    (value, verdict)
}

/// Saturation points are "already maxed out" reference levels, not
/// precise breakpoints: T1=20% held by one address, T5=45% by the top 5,
/// T10=60% by the top 10, T20=75% by the top 20. The quadratic ramp means
/// mild concentration barely registers and severe concentration escalates
/// fast. The 0.03 floor exists so the score never claims zero risk from a
/// signal (holders 21+) we simply cannot see.
fn concentration_probability(c: &ConcentrationInput) -> f64 {
    const T1: f64 = 0.20;
    const T5: f64 = 0.45;
    const T10: f64 = 0.60;
    const T20: f64 = 0.75;
    const FLOOR: f64 = 0.03;
    const CEILING_SPAN: f64 = 0.87;

    let ratio = |pct: f64, threshold: f64| {
        let r = (pct / 100.0) / threshold;
        (r * r).min(1.0)
    };
    let worst = ratio(c.top1_pct, T1)
        .max(ratio(c.top5_pct, T5))
        .max(ratio(c.top10_pct, T10))
        .max(ratio(c.top20_pct, T20));
    FLOOR + CEILING_SPAN * worst
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mint::MintExtensions;

    fn no_extensions() -> MintExtensions {
        MintExtensions::default()
    }

    #[test]
    fn clean_mint_scores_green() {
        let concentration = ConcentrationInput {
            top1_pct: 3.0,
            top5_pct: 8.0,
            top10_pct: 12.0,
            top20_pct: 18.0,
        };
        let (s, verdict) = score(false, false, &no_extensions(), Some(&concentration), 25.0, 60.0);
        assert!((s - 8.0).abs() < 0.2, "expected ~8.0, got {s}");
        assert_eq!(verdict, Verdict::Green);
    }

    #[test]
    fn renounced_authorities_but_moderate_concentration_is_high_amber() {
        let concentration = ConcentrationInput {
            top1_pct: 15.0,
            top5_pct: 35.0,
            top10_pct: 48.0,
            top20_pct: 55.0,
        };
        let (s, verdict) = score(false, false, &no_extensions(), Some(&concentration), 25.0, 60.0);
        assert!((s - 58.7).abs() < 0.2, "expected ~58.7, got {s}");
        assert_eq!(verdict, Verdict::Amber);
    }

    #[test]
    fn permanent_delegate_dominates_regardless_of_clean_concentration() {
        let concentration = ConcentrationInput {
            top1_pct: 3.0,
            top5_pct: 8.0,
            top10_pct: 12.0,
            top20_pct: 18.0,
        };
        let extensions = MintExtensions {
            permanent_delegate: true,
            ..MintExtensions::default()
        };
        let (s, verdict) = score(false, false, &extensions, Some(&concentration), 25.0, 60.0);
        assert!((s - 90.8).abs() < 0.2, "expected ~90.8, got {s}");
        assert_eq!(verdict, Verdict::Red);
    }

    #[test]
    fn three_moderate_factors_compound_past_any_single_one() {
        let concentration = ConcentrationInput {
            top1_pct: 8.0,
            top5_pct: 20.0,
            top10_pct: 30.0,
            top20_pct: 35.0,
        };
        let extensions = MintExtensions {
            transfer_fee_config: true,
            ..MintExtensions::default()
        };
        let (s, verdict) = score(true, false, &extensions, Some(&concentration), 25.0, 60.0);
        assert!((s - 48.8).abs() < 0.2, "expected ~48.8, got {s}");
        assert_eq!(verdict, Verdict::Amber);

        // Each factor alone would be green — this is the case that justifies
        // noisy-OR over a plain max() across factors.
        let (mint_only, v1) = score(true, false, &no_extensions(), Some(&ConcentrationInput {
            top1_pct: 0.0, top5_pct: 0.0, top10_pct: 0.0, top20_pct: 0.0,
        }), 25.0, 60.0);
        assert!(mint_only < 25.0);
        assert_eq!(v1, Verdict::Green);
    }

    #[test]
    fn verdict_bands_are_config_overridable() {
        let concentration = ConcentrationInput {
            top1_pct: 3.0, top5_pct: 8.0, top10_pct: 12.0, top20_pct: 18.0,
        };
        // Same ~8.0 score as the clean-mint test, but a tighter amber
        // threshold now classifies it amber instead of green.
        let (_, verdict) = score(false, false, &no_extensions(), Some(&concentration), 5.0, 60.0);
        assert_eq!(verdict, Verdict::Amber);
    }

    #[test]
    fn unmeasured_concentration_can_never_score_green() {
        // The same mint that scores a clean green with holder data must not
        // keep that green once the holder reading is missing.
        let clean = ConcentrationInput {
            top1_pct: 3.0, top5_pct: 8.0, top10_pct: 12.0, top20_pct: 18.0,
        };
        let (measured_score, measured) = score(false, false, &no_extensions(), Some(&clean), 25.0, 60.0);
        assert_eq!(measured, Verdict::Green);

        let (unknown_score, unknown) = score(false, false, &no_extensions(), None, 25.0, 60.0);
        assert_eq!(unknown, Verdict::Amber);
        // No probability was invented for the missing signal, so the score
        // drops to what was actually measured even as the verdict hardens.
        assert!(unknown_score < measured_score);
        assert!(unknown_score.abs() < f64::EPSILON, "expected 0.0, got {unknown_score}");
    }

    #[test]
    fn unmeasured_concentration_never_softens_a_bad_verdict() {
        let extensions = MintExtensions {
            permanent_delegate: true,
            ..MintExtensions::default()
        };
        let (s, verdict) = score(false, false, &extensions, None, 25.0, 60.0);
        assert!((s - 90.0).abs() < 0.2, "expected ~90.0, got {s}");
        assert_eq!(verdict, Verdict::Red);
    }
}
