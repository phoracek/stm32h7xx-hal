#![deny(warnings)]
#![no_main]
#![no_std]

#[macro_use]
mod utilities;

use cortex_m_rt::entry;
use stm32h7xx_hal::{pac, prelude::*, xspi::QspiMode};

use log::info;

#[entry]
fn main() -> ! {
    utilities::logger::init();
    let dp = pac::Peripherals::take().unwrap();

    // Constrain and Freeze power
    let pwr = dp.PWR.constrain();
    let pwrcfg = example_power!(pwr).freeze();

    // Constrain and Freeze clock
    let rcc = dp.RCC.constrain();
    let ccdr = rcc.sys_ck(96.mhz()).freeze(pwrcfg, &dp.SYSCFG);

    // Acquire the GPIO peripherals. This also enables the clock for
    // the GPIOs in the RCC register.
    let gpiog = dp.GPIOG.split(ccdr.peripheral.GPIOG);
    let _gpiob = dp.GPIOB.split(ccdr.peripheral.GPIOB);
    let _gpiod = dp.GPIOD.split(ccdr.peripheral.GPIOD);
    let _gpioe = dp.GPIOE.split(ccdr.peripheral.GPIOE);
    let gpiof = dp.GPIOF.split(ccdr.peripheral.GPIOF);

    let _qspi_cs = gpiog.pg6.into_alternate_af10();

    // Pins taken from:
    // https://github.com/electro-smith/libDaisy/blob/3dda55e9ed55a2f8b6bc4fa6aa2c7ae134c317ab/src/per/qspi.c#L704
    let sck = gpiof.pf10.into_alternate_af9();
    let io0 = gpiof.pf8.into_alternate_af10();
    let io1 = gpiof.pf9.into_alternate_af10();
    let io2 = gpiof.pf7.into_alternate_af9();
    let io3 = gpiof.pf6.into_alternate_af9();

    info!("");
    info!("stm32h7xx-hal example - QSPI");
    info!("");

    // Initialise the QSPI peripheral.
    let mut qspi = dp.QUADSPI.bank1(
        (sck, io0, io1, io2, io3),
        3.mhz(),
        &ccdr.clocks,
        ccdr.peripheral.QSPI,
    );

    qspi.configure_mode(QspiMode::FourBit).unwrap();

    let test = [0xAA, 0x00, 0xFF];

    qspi.write(0x00, &test).unwrap();

    let mut read: [u8; 3] = [0; 3];
    qspi.read(0x00, &mut read).unwrap();

    assert_eq!(read, test);

    loop {}
}
