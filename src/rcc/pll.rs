//! Phase Locked Loop Configuration

use super::{Rcc, HSI};
use crate::stm32::RCC;
use crate::time::Hertz;

const FRACN_DIVISOR: f32 = 8192.0; // 2 ** 13
const FRACN_MAX: f32 = FRACN_DIVISOR - 1.0;

/// Strategies for configuring a Phase Locked Loop (PLL)
#[derive(Copy, Clone, PartialEq)]
pub enum PllConfigStrategy {
    /// VCOL, highest PFD frequency, highest VCO frequency
    Normal,
    /// VCOH, choose PFD frequency for accuracy, highest VCO frequency
    Iterative,
    /// VCOH, choose PFD frequency for accuracy, highest VCO frequency
    /// Uses fractional mode to precisely set the P clock
    Fractional,
    /// VCOH, choose PFD frequency for accuracy, highest VCO frequency
    /// Uses fractional mode to precisely set the P clock not less than target frequency
    FractionalNotLess,
}

/// Configuration of a Phase Locked Loop (PLL)
pub struct PllConfig {
    pub(super) strategy: PllConfigStrategy,
    pub(super) p_ck: Option<u32>,
    pub(super) q_ck: Option<u32>,
    pub(super) r_ck: Option<u32>,
}
impl Default for PllConfig {
    fn default() -> PllConfig {
        loop {}
    }
}

/// Calculate VCO output divider (p-divider). Choose the highest VCO
/// frequency to give specified output.
///
/// Returns *target* VCO frequency
///
macro_rules! vco_output_divider_setup {
    ($output: ident, $vco_min: ident, $vco_max: ident $(,$pll1_p:ident)*) => {{
        let pll_x_p = 0_u32;
        let vco_ck = 0_u32;

        (vco_ck, pll_x_p)
    }};
}

/// Setup PFD input frequency and VCO output frequency
///
macro_rules! vco_setup {
    // Normal: VCOL, highest PFD frequency, highest VCO frequency
    (NORMAL: $pllsrc:ident, $output:ident,
     $rcc:ident, $pllXvcosel:ident, $pllXrge:ident $(,$pll1_p:ident)*) => {{
         let ref_x_ck = 0u32;
         let pll_x_m = 0u32;
         let pll_x_p = 0u32;
         let vco_ck_target = 0u32;

         (ref_x_ck, pll_x_m, pll_x_p, vco_ck_target)
     }};
    // Iterative: VCOH, choose PFD frequency for accuracy, highest VCO frequency
    (ITERATIVE: $pllsrc:ident, $output:ident,
     $rcc:ident, $pllXvcosel:ident, $pllXrge:ident $(,$pll1_p:ident)*) => {{
         // VCO output frequency. Choose the highest VCO frequency
         let (vco_ck_target, pll_x_p) = {
             vco_output_divider_setup! { $output, vco_min, vco_max $(, $pll1_p)* }
         };

         // Input divisor, resulting in a reference clock in the
         // range 2 to 16 MHz.
         let pll_x_m_min = ($pllsrc + 15_999_999) / 16_000_000;
         let pll_x_m_max = match $pllsrc {
             0 ..= 127_999_999 => $pllsrc / 2_000_000,
             _ => 63            // pllm < 64
         };

         // Iterative search for the lowest m value that minimizes
         // the difference between requested and actual VCO frequency
         let pll_x_m = (pll_x_m_min..=pll_x_m_max).min_by_key(|pll_x_m| {
             let ref_x_ck = $pllsrc / pll_x_m;

             // Feedback divider. Integer only
             let pll_x_n = vco_ck_target / ref_x_ck;

             vco_ck_target as i32 - (ref_x_ck * pll_x_n) as i32
         }).unwrap();

         // Calculate resulting reference clock
         let ref_x_ck = $pllsrc / pll_x_m;

         $rcc.pllcfgr.modify(|_, w| {
             match ref_x_ck {
                 2_000_000 ..= 3_999_999 => // ref_x_ck is 2 - 4 MHz
                     w.$pllXrge().range2(),
                 4_000_000 ..= 7_999_999 => // ref_x_ck is 4 - 8 MHz
                     w.$pllXrge().range4(),
                 _ =>           // ref_x_ck is 8 - 16 MHz
                     w.$pllXrge().range8(),
             }
         });

         (ref_x_ck, pll_x_m, pll_x_p, vco_ck_target)
     }};
}

macro_rules! pll_setup {
    ($pll_setup:ident: ($pllXvcosel:ident, $pllXrge:ident, $pllXfracen:ident,
                   $pllXdivr:ident, $divnX:ident, $divmX:ident, $pllXfracr:ident, $fracnx:ident,
                   OUTPUTS: [ $($CK:ident:
                                ($div:ident, $diven:ident, $DD:tt $(,$unsafe:ident)*)),+ ]
                   $(,$pll1_p:ident)*
    )) => {
        /// PLL Setup
        /// Returns (Option(pllX_p_ck), Option(pllX_q_ck), Option(pllX_r_ck))
        pub(super) fn $pll_setup(
            &self,
            rcc: &RCC,
            pll: &PllConfig,
        ) -> (Option<Hertz>, Option<Hertz>, Option<Hertz>) {
            // PLL sourced from either HSE or HSI
            let pllsrc = self.config.hse.unwrap_or(HSI);

            // PLL output
            match pll.p_ck {
                Some(output) => {
                    // Set VCO parameters based on VCO strategy
                    let (ref_x_ck, pll_x_m, pll_x_p, vco_ck_target) =
                        match pll.strategy {
                            PllConfigStrategy::Normal => {
                                vco_setup! { NORMAL: pllsrc, output,
                                    rcc, $pllXvcosel,
                                    $pllXrge $(, $pll1_p)* }
                            },
                            // Iterative, Fractional, FractionalNotLess
                            _ => {
                                vco_setup! { ITERATIVE: pllsrc, output,
                                    rcc, $pllXvcosel,
                                    $pllXrge $(, $pll1_p)* }

                            }

                        };
                    (None, None, None)
                },
                None => {
                    (None, None, None)
                }
            }
        }
    };
}

/// Calcuate the Fractional-N part of the divider
///
/// ref_clk - Frequency at the PFD input
/// pll_n - Integer-N part of the divider
/// pll_p - P-divider
/// output - Wanted output frequency
fn calc_fracn(_ref_clk: f32, _pll_n: f32, _pll_p: f32, _output: f32) -> u16 {
    loop {}
}

/// Calculates the {Q,R}-divider. Must NOT be used for the P-divider, as this
/// has additional restrictions on PLL1.
///
/// vco_ck - VCO output frequency
/// target_ck - Target {Q,R} output frequency
fn calc_ck_div(
    _strategy: PllConfigStrategy,
    _vco_ck: u32,
    _target_ck: u32,
) -> u32 {
    loop {}
}

/// Calculates the VCO output frequency
///
/// ref_clk - Frequency at the PFD input
/// pll_n - Integer-N part of the divider
/// pll_fracn - Fractional-N part of the divider
fn calc_vco_ck(_ref_ck: u32, _pll_n: u32, _pll_fracn: u16) -> u32 {
    loop {}
}

impl Rcc {
    pll_setup! {
    pll1_setup: (pll1vcosel, pll1rge, pll1fracen, pll1divr, divn1, divm1, pll1fracr, fracn1,
                 OUTPUTS: [
                      // unsafe as not all values are permitted: see RM0433
                     p_ck: (divp1, divp1en, 0, unsafe),
                     q_ck: (divq1, divq1en, 1),
                     r_ck: (divr1, divr1en, 2) ],
                 pll1_p)
    }
}

#[cfg(test)]
mod tests {
    use crate::rcc::pll::{
        calc_ck_div, calc_fracn, calc_vco_ck, PllConfigStrategy,
    };

    macro_rules! dummy_method {
        ($($name:ident),+) => (
            $(
                fn $name(self) -> Self {
                    self
                }
            )+
        )
    }

    // Mock PLL CFGR
    struct WPllCfgr {}
    impl WPllCfgr {
        dummy_method! { vcosel, medium_vco, wide_vco }
        dummy_method! { pllrge, range1, range2, range4, range8 }
    }
    struct MockPllCfgr {}
    impl MockPllCfgr {
        // Modify mock registers
        fn modify<F>(&self, func: F)
        where
            F: FnOnce((), WPllCfgr) -> WPllCfgr,
        {
            func((), WPllCfgr {});
        }
    }

    // Mock RCC
    struct MockRcc {
        pub pllcfgr: MockPllCfgr,
    }
    impl MockRcc {
        pub fn new() -> Self {
            MockRcc {
                pllcfgr: MockPllCfgr {},
            }
        }
    }

    #[test]
    /// Test PFD input frequency PLL and VCO output frequency
    fn vco_setup_normal() {
        let rcc = MockRcc::new();

        let pllsrc = 25_000_000; // PLL source frequency eg. 25MHz crystal
        let pll_p_target = 242_000_000; // PLL output frequency (P_CK)
        let pll_q_target = 120_900_000; // PLL output frequency (Q_CK)
        let pll_r_target = 30_200_000; // PLL output frequency (R_CK)
        println!(
            "PLL2/3 {} MHz -> {} MHz",
            pllsrc as f32 / 1e6,
            pll_p_target as f32 / 1e6
        );

        // ----------------------------------------

        // VCO Setup
        println!("NORMAL");
        let (ref_x_ck, pll_x_m, pll_x_p, vco_ck_target) = vco_setup! {
            NORMAL: pllsrc, pll_p_target, rcc, vcosel, pllrge
        };
        // Feedback divider. Integer only
        let pll_x_n = vco_ck_target / ref_x_ck;
        // Resulting achieved vco_ck
        let vco_ck_achieved = calc_vco_ck(ref_x_ck, pll_x_n, 0);
        // {Q,R} output clocks
        let pll_x_q = calc_ck_div(
            PllConfigStrategy::Normal,
            vco_ck_achieved,
            pll_q_target,
        );
        let pll_x_r = calc_ck_div(
            PllConfigStrategy::Normal,
            vco_ck_achieved,
            pll_r_target,
        );

        // ----------------------------------------

        // Input
        println!("M Divider {}", pll_x_m);
        let input = pllsrc as f32 / pll_x_m as f32;
        println!("==> Input {} MHz", input / 1e6);
        println!();
        assert!((input > 1e6) && (input < 2e6));

        println!("VCO CK Target {} MHz", vco_ck_target as f32 / 1e6);
        println!("VCO CK Achieved {} MHz", vco_ck_achieved as f32 / 1e6);
        println!();

        // Output
        println!("P Divider {}", pll_x_p);
        let output_p = vco_ck_achieved as f32 / pll_x_p as f32;
        println!("==> Output {} MHz", output_p / 1e6);

        let error = output_p - pll_p_target as f32;
        println!(
            "Error {} {}",
            f32::abs(error),
            (pll_p_target as f32 / 100.0)
        );
        assert!(f32::abs(error) < (pll_p_target as f32 / 100.0)); // < ±1% error
        println!();

        let output_q = vco_ck_achieved as f32 / pll_x_q as f32;
        println!("Q Divider {}", pll_x_q);
        println!("==> Output Q {} MHz", output_q / 1e6);
        println!();
        let error = output_q - pll_q_target as f32;
        assert!(f32::abs(error) < (pll_q_target as f32 / 100.0)); // < ±1% error

        let output_r = vco_ck_achieved as f32 / pll_x_r as f32;
        println!("R Divider {}", pll_x_r);
        println!("==> Output Q {} MHz", output_r / 1e6);
        println!();
        let error = output_r - pll_r_target as f32;
        assert!(f32::abs(error) < (pll_r_target as f32 / 100.0)); // < ±1% error
    }

    #[test]
    /// Test PFD input frequency PLL and VCO output frequency
    fn vco_setup_iterative() {
        let rcc = MockRcc::new();

        let pllsrc = 25_000_000; // PLL source frequency eg. 25MHz crystal
        let pll_p_target = 240_000_000; // PLL output frequency (P_CK)
        let pll_q_target = 120_000_000; // PLL output frequency (Q_CK)
        let pll_r_target = 30_000_000; // PLL output frequency (R_CK)
        println!(
            "PLL2/3 {} MHz -> {} MHz",
            pllsrc as f32 / 1e6,
            pll_p_target as f32 / 1e6
        );

        // ----------------------------------------

        // VCO Setup
        println!("ITERATIVE");
        let (ref_x_ck, pll_x_m, pll_x_p, vco_ck_target) = vco_setup! {
            ITERATIVE: pllsrc, pll_p_target, rcc, vcosel, pllrge
        };
        // Feedback divider. Integer only
        let pll_x_n = vco_ck_target / ref_x_ck;
        // Resulting achieved vco_ck
        let vco_ck_achieved = calc_vco_ck(ref_x_ck, pll_x_n, 0);
        // {Q,R} output clocks
        let pll_x_q = calc_ck_div(
            PllConfigStrategy::Iterative,
            vco_ck_target,
            pll_q_target,
        );
        let pll_x_r = calc_ck_div(
            PllConfigStrategy::Iterative,
            vco_ck_target,
            pll_r_target,
        );

        // ----------------------------------------

        // Input
        println!("M Divider {}", pll_x_m);
        let input = pllsrc as f32 / pll_x_m as f32;
        println!("==> Input {} MHz", input / 1e6);
        println!();
        assert_eq!(input, 5e6);

        println!("VCO CK Target {} MHz", vco_ck_target as f32 / 1e6);
        println!("VCO CK Achieved {} MHz", pll_x_n as f32 * input / 1e6);
        println!();

        // Output
        println!("P Divider {}", pll_x_p);
        let output_p = pll_x_n as f32 * input / pll_x_p as f32;
        println!("==> Output P {} MHz", output_p / 1e6);
        println!();
        assert_eq!(output_p, 240e6);

        let output_q = vco_ck_achieved as f32 / pll_x_q as f32;
        println!("Q Divider {}", pll_x_q);
        println!("==> Output Q {} MHz", output_q / 1e6);
        println!();
        assert_eq!(output_q, pll_q_target as f32);

        let output_r = vco_ck_achieved as f32 / pll_x_r as f32;
        println!("R Divider {}", pll_x_r);
        println!("==> Output R {} MHz", output_r / 1e6);
        println!();
        assert_eq!(output_r, pll_r_target as f32);
    }

    #[test]
    /// Test PFD input frequency PLL and VCO output frequency
    fn vco_setup_fractional() {
        let rcc = MockRcc::new();

        let pllsrc = 16_000_000; // PLL source frequency eg. 16MHz crystal
        let pll_p_target = 48_000 * 256; // Target clock
        let pll_q_target = 48_000 * 128; // Target clock
        let pll_r_target = 48_000 * 63; // Target clock
        let output = pll_p_target; // PLL output frequency (P_CK)
        println!(
            "PLL2/3 {} MHz -> {} MHz",
            pllsrc as f32 / 1e6,
            output as f32 / 1e6
        );

        // ----------------------------------------

        // VCO Setup
        println!("Fractional");
        let (ref_x_ck, pll_x_m, pll_x_p, vco_ck_target) = vco_setup! {
            ITERATIVE: pllsrc, output, rcc, vcosel, pllrge
        };
        let input = pllsrc as f32 / pll_x_m as f32;

        // Feedback divider. Integer only
        let pll_x_n = vco_ck_target / ref_x_ck;
        let pll_x_fracn = calc_fracn(
            input as f32,
            pll_x_n as f32,
            pll_x_p as f32,
            output as f32,
        );
        println!("FRACN Divider {}", pll_x_fracn);
        // Resulting achieved vco_ck
        let vco_ck_achieved = calc_vco_ck(ref_x_ck, pll_x_n, pll_x_fracn);

        // Calulate additional output dividers
        let pll_x_q = calc_ck_div(
            PllConfigStrategy::Fractional,
            vco_ck_achieved,
            pll_q_target,
        );
        let pll_x_r = calc_ck_div(
            PllConfigStrategy::Fractional,
            vco_ck_achieved,
            pll_r_target,
        );

        // ----------------------------------------

        // Input
        println!("M Divider {}", pll_x_m);
        println!("==> Input {} MHz", input / 1e6);
        println!();

        println!("VCO CK Target {} MHz", vco_ck_target as f32 / 1e6);
        println!("VCO CK Achieved {} MHz", vco_ck_achieved as f32 / 1e6);
        println!();

        // Output
        let output_p = vco_ck_achieved as f32 / pll_x_p as f32;
        println!("P Divider {}", pll_x_p);
        println!("==> Output P {} MHz", output_p / 1e6);
        println!();

        // The P_CK should be very close to the target with a finely tuned FRACN
        //
        // The other clocks accuracy will vary depending on how close
        // they are to an integer fraction of the P_CK
        assert!(output_p <= pll_p_target as f32);
        let error = output_p - pll_p_target as f32;
        assert!(f32::abs(error) < (pll_p_target as f32 / 500_000.0)); // < ±.0002% = 2ppm error

        let output_q = vco_ck_achieved as f32 / pll_x_q as f32;
        println!("Q Divider {}", pll_x_q);
        println!("==> Output Q {} MHz", output_q / 1e6);
        println!();
        assert!(output_q <= pll_q_target as f32);

        let output_r = vco_ck_achieved as f32 / pll_x_r as f32;
        println!("R Divider {}", pll_x_r);
        println!("==> Output R {} MHz", output_r / 1e6);
        println!();
        assert!(output_r <= pll_r_target as f32);
    }

    #[test]
    fn vco_setup_fractional_not_less() {
        let rcc = MockRcc::new();

        let pllsrc = 16_000_000; // PLL source frequency eg. 16MHz crystal
        let pll_p_target = 48_000 * 256; // Target clock
        let pll_q_target = 48_000 * 128; // Target clock
        let pll_r_target = 48_000 * 63; // Target clock
        let output = pll_p_target; // PLL output frequency (P_CK)
        println!(
            "PLL2/3 {} MHz -> {} MHz",
            pllsrc as f32 / 1e6,
            output as f32 / 1e6
        );

        // ----------------------------------------

        // VCO Setup
        println!("FractionalNotLess");
        let (ref_x_ck, pll_x_m, pll_x_p, vco_ck_target) = vco_setup! {
            ITERATIVE: pllsrc, output, rcc, vcosel, pllrge
        };
        let input = pllsrc as f32 / pll_x_m as f32;

        // Feedback divider. Integer only
        let pll_x_n = vco_ck_target / ref_x_ck;
        let pll_x_fracn = calc_fracn(
            input as f32,
            pll_x_n as f32,
            pll_x_p as f32,
            output as f32,
        ) + 1;
        println!("FRACN Divider {}", pll_x_fracn);
        // Resulting achieved vco_ck
        let vco_ck_achieved = calc_vco_ck(ref_x_ck, pll_x_n, pll_x_fracn);

        // Calulate additional output dividers
        let pll_x_q = calc_ck_div(
            PllConfigStrategy::FractionalNotLess,
            vco_ck_achieved,
            pll_q_target,
        );
        let pll_x_r = calc_ck_div(
            PllConfigStrategy::FractionalNotLess,
            vco_ck_achieved,
            pll_r_target,
        );

        // ----------------------------------------

        // Input
        println!("M Divider {}", pll_x_m);
        println!("==> Input {} MHz", input / 1e6);
        println!();
        // assert_eq!(input, 2e6);

        println!("VCO CK Target {} MHz", vco_ck_target as f32 / 1e6);
        println!("VCO CK Achieved {} MHz", vco_ck_achieved as f32 / 1e6);
        println!();

        // Output
        let output_p = vco_ck_achieved as f32 / pll_x_p as f32;
        println!("P Divider {}", pll_x_p);
        println!("==> Output P {} MHz", output_p / 1e6);
        println!();

        // The P_CK should be very close to the target with a finely tuned FRACN
        //
        // The other clocks accuracy will vary depending on how close
        // they are to an integer fraction of the P_CK
        assert!(output_p >= pll_p_target as f32);
        let error = output_p - pll_p_target as f32;
        assert!(f32::abs(error) < (pll_p_target as f32 / 500_000.0)); // < ±.0002% = 2ppm error

        let output_q = vco_ck_achieved as f32 / pll_x_q as f32;
        println!("Q Divider {}", pll_x_q);
        println!("==> Output Q {} MHz", output_q / 1e6);
        println!();
        assert!(output_q >= pll_q_target as f32);

        let output_r = vco_ck_achieved as f32 / pll_x_r as f32;
        println!("R Divider {}", pll_x_r);
        println!("==> Output R {} MHz", output_r / 1e6);
        println!();
        assert!(output_r >= pll_r_target as f32);
    }
}
