//! Reset and Clock Control
//!
//! This module configures the RCC unit to provide set frequencies for
//! the input to the SCGU `sys_ck`, the AMBA High-performance Busses
//! and Advanced eXtensible Interface bus `hclk`, the AMBA Peripheral
//! Busses `pclkN` and the peripheral clock `per_ck`.
//!
//! See Fig 46 "Core and bus clock generation" in Reference Manual
//! RM0433 for information (p 336).
//!
//! HSI is 64 MHz.
//! CSI is 4 MHz.
//! HSI48 is 48MHz.
//!
//! # Usage
//!
//! This peripheral is must be used alongside the
//! [`PWR`](../pwr/index.html) peripheral to freeze voltage scaling of the
//! device.
//!
//! A builder pattern is used to specify the state and frequency of
//! possible clocks. The `freeze` method configures the RCC peripheral
//! in a best-effort attempt to generate these clocks. The actual
//! clocks configured are returned in `ccdr.clocks`.
//!
//! No clock specification overrides another. However supplying some
//! clock specifications may influence multiple resulting clocks,
//! including those corresponding to other clock specifications. This
//! is particularly the case for PLL clocks, where the frequencies of
//! adjacent 'P', 'Q, and 'R' clock outputs must have a simple integer
//! fraction relationship.
//!
//! Some clock specifications imply other clock specifications, as follows:
//!
//! * `use_hse(a)` implies `sys_ck(a)`
//!
//! * `sys_ck(b)` implies `pll1_p_ck(b)` unless `b` equals HSI or
//! `use_hse(b)` was specified
//!
//! * `pll1_p_ck(c)` implies `pll1_r_ck(c/2)`, including when
//! `pll1_p_ck` was implied by `sys_ck(c)` or `mco2_from_pll1_p_ck(c)`.
//!
//! Implied clock specifications can always be overridden by explicitly
//! specifying that clock. If this results in a configuration that cannot
//! be achieved by hardware, `freeze` will panic.
//!
//! # Configuration Example
//!
//! A simple example:
//!
//! ```rust
//!     let dp = pac::Peripherals::take().unwrap();
//!
//!     let pwr = dp.PWR.constrain();
//!     let pwrcfg = pwr.freeze();
//!
//!     let rcc = dp.RCC.constrain();
//!     let ccdr = rcc
//!         .sys_ck(96.mhz())
//!         .pclk1(48.mhz())
//!         .freeze(pwrcfg, &dp.SYSCFG);
//! ```
//!
//! A more complex example, involving the PLL:
//!
//! ```rust
//!     let dp = pac::Peripherals::take().unwrap();
//!
//!     let pwr = dp.PWR.constrain();
//!     let pwrcfg = pwr.freeze();
//!
//!     let rcc = dp.RCC.constrain();
//!     let ccdr = rcc
//!         .sys_ck(200.mhz()) // Implies pll1_p_ck
//!         // For non-integer values, round up. `freeze` will never
//!         // configure a clock faster than that specified.
//!         .pll1_q_ck(33_333_334.hz())
//!         .freeze(pwrcfg, &dp.SYSCFG);
//! ```
//!
//! A much more complex example, indicative of real usage with a
//! significant fraction of the STM32H7's capabilities.
//!
//! ```rust
//!     let dp = pac::Peripherals::take().unwrap();
//!
//!     let pwr = dp.PWR.constrain();
//!     let pwrcfg = pwr.freeze();
//!
//!     let rcc = dp.RCC.constrain();
//!     let ccdr = rcc
//!         .use_hse(25.mhz()) // XTAL X1
//!         .sys_ck(400.mhz())
//!         .pll1_r_ck(100.mhz()) // for TRACECK
//!         .pll1_q_ck(200.mhz())
//!         .hclk(200.mhz())
//!         .pll3_strategy(PllConfigStrategy::Iterative)
//!         .pll3_p_ck(240.mhz()) // for LTDC
//!         .pll3_q_ck(48.mhz()) // for LTDC
//!         .pll3_r_ck(26_666_667.hz()) // Pixel clock for LTDC
//!         .freeze(pwrcfg, &dp.SYSCFG);
//!```
//!
//! # Peripherals
//!
//! The `freeze()` method returns a [Core Clocks Distribution and Reset
//! (CCDR)](struct.Ccdr.html) object. This singleton tells you how the core
//! clocks were actually configured (in [CoreClocks](struct.CoreClocks.html))
//! and allows you to configure the remaining peripherals (see
//! [PeripheralREC](crate::rcc::rec::struct.PeripheralREC.html)).
//!
//!```rust
//! let ccdr = ...; // Returned by `freeze()`, see examples above
//!
//! // Runtime confirmation that hclk really is 200MHz
//! assert_eq!(ccdr.clocks.hclk().0, 200_000_000);
//!
//! // Panics if pll1_q_ck is not running
//! let _ = ccdr.clocks.pll1_q_ck().unwrap();
//!
//! // Enable the clock to a peripheral and reset it
//! ccdr.peripheral.FDCAN.enable().reset();
//!```
//!
//! The [PeripheralREC](struct.PeripheralREC.html) members implement move
//! semantics, so once you have passed them to a constructor they cannot be
//! modified again in safe Rust.
//!
//!```rust
//! // Constructor for custom FDCAN driver
//! my_fdcan(dp.FDCAN,
//!          &ccdr.clocks,         // Immutable reference to core clock state
//!          ccdr.peripheral.FDCAN // Ownership of reset + enable control
//! );
//!
//! // Compile error, value was moved ^^
//! ccdr.peripheral.FDCAN.disable();
//!```
//!
#![deny(missing_docs)]

use crate::stm32::RCC;

#[cfg(not_now)]
pub mod backup;
#[cfg(not_now)]
mod core_clocks;
#[cfg(not_now)]
pub mod rec;
#[cfg(not_now)]
mod mco;

mod pll;

pub use pll::{PllConfig, PllConfigStrategy};


/// Configuration of the core clocks
pub struct Config {
    hse: Option<u32>,
    bypass_hse: bool,
    sys_ck: Option<u32>,
    per_ck: Option<u32>,
    rcc_hclk: Option<u32>,
    rcc_pclk1: Option<u32>,
    rcc_pclk2: Option<u32>,
    rcc_pclk3: Option<u32>,
    rcc_pclk4: Option<u32>,
    pll1: PllConfig,
    pll2: PllConfig,
    pll3: PllConfig,
}

/// Constrained RCC peripheral
///
/// Generated by calling `constrain` on the PAC's RCC peripheral.
///
/// ```rust
/// let dp = stm32::Peripherals::take().unwrap();
/// let rcc = dp.RCC.constrain();
/// ```
pub struct Rcc {
    config: Config,
    pub(crate) rb: RCC,
}

/// Core Clock Distribution and Reset (CCDR)
///
/// Generated when the RCC is frozen. The configuration of the Sys_Ck
/// `sys_ck`, CPU Clock `c_ck`, AXI peripheral clock `aclk`, AHB
/// clocks `hclk`, APB clocks `pclkN` and PLL outputs `pllN_X_ck` are
/// frozen. However the distribution of some clocks may still be
/// modified and peripherals enabled / reset by passing this object
/// to other implementations in this stack.
pub struct Ccdr {
    // Yes, it lives (locally)! We retain the right to switch most
    // PKSUs on the fly, to fine-tune PLL frequencies, and to enable /
    // reset peripherals.
    //
    // TODO: Remove this once all permitted RCC register accesses
    // after freeze are enumerated in this struct
    pub(crate) rb: RCC,
}

const HSI: u32 = 64_000_000; // Hz
