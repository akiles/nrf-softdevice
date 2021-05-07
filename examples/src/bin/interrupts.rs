#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(min_type_alias_impl_trait)]
#![feature(impl_trait_in_bindings)]
#![feature(alloc_error_handler)]

#[path = "../example_common.rs"]
mod example_common;
use example_common::*;

use cortex_m_rt::entry;
use defmt::*;
use embassy::executor::Executor;
use embassy::task;
use embassy::util::Forever;

use nrf_softdevice::interrupt;
use nrf_softdevice::Softdevice;

static EXECUTOR: Forever<Executor> = Forever::new();

#[task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

#[task]
async fn interrupt_task(_sd: &'static Softdevice) {
    let enabled = interrupt::is_enabled(interrupt::SWI0_EGU0);
    info!("enabled: {:?}", enabled);

    // This would panic with "irq RADIO is reserved for the softdevice"
    // interrupt::set_priority(interrupt::RADIO, interrupt::Priority::Level7);

    // This would panic with "priority level Level1 is reserved for the softdevice"
    // interrupt::set_priority(interrupt::SWI0_EGU0, interrupt::Priority::Level1);

    // This would panic with "irq SWI0_EGU0 has priority Level0 which is reserved for the softdevice. Set another prority before enabling it.""
    // interrupt::enable(interrupt::SWI0_EGU0);

    // If we set a non-reserved priority first, we can enable the interrupt
    interrupt::set_priority(interrupt::SWI0_EGU0, interrupt::Priority::Level7);
    interrupt::enable(interrupt::SWI0_EGU0);

    // Now it's enabled
    let enabled = interrupt::is_enabled(interrupt::SWI0_EGU0);
    info!("enabled: {:?}", enabled);

    // The interrupt will trigger instantly
    info!("before pend");
    interrupt::pend(interrupt::SWI0_EGU0);
    info!("after pend");

    interrupt::free(|_| {
        info!("Hello from critical section!");

        // The interrupt will trigger at end of critical section
        info!("before pend");
        interrupt::pend(interrupt::SWI0_EGU0);
        info!("after pend");

        // This will print true even if we're inside a critical section
        // because it reads a "backup" of the irq enabled registers.
        let enabled = interrupt::is_enabled(interrupt::SWI0_EGU0);
        info!("enabled: {:?}", enabled);

        // You can also enable/disable interrupts inside a critical section
        // They don't take effect until exiting the critical section, so it's safe.
        // (they modify the "backup" register which gets restored on CS exit)
        interrupt::set_priority(interrupt::SWI1_EGU1, interrupt::Priority::Level6);
        interrupt::enable(interrupt::SWI1_EGU1);
        interrupt::pend(interrupt::SWI1_EGU1);

        info!("exiting critical section...");
    });

    info!("exited critical section");
}

#[interrupt]
fn SWI0_EGU0() {
    info!("SWI0_EGU0 triggered!")
}

#[interrupt]
fn SWI1_EGU1() {
    info!("SWI1_EGU1 triggered!")
}

#[entry]
fn main() -> ! {
    info!("Hello World!");

    let (sdp, _p) = take_peripherals();
    let sd = Softdevice::enable(sdp, &Default::default());

    let executor = EXECUTOR.put(Executor::new());
    executor.run(|spawner| {
        unwrap!(spawner.spawn(softdevice_task(sd)));
        unwrap!(spawner.spawn(interrupt_task(sd)));
    });
}
