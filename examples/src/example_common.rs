#![macro_use]

use nrf_softdevice_defmt_rtt as _; // global logger
use panic_probe as _;

use core::sync::atomic::{AtomicUsize, Ordering};
use defmt::panic;

defmt::timestamp! {"{=u64}", {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        // NOTE(no-CAS) `timestamps` runs with interrupts disabled
        let n = COUNT.load(Ordering::Relaxed);
        COUNT.store(n + 1, Ordering::Relaxed);
        n as u64
    }
}
