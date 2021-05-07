//! This example showcases how to use nrf-softdevice inside RTIC.
//!
//! It mixes RTIC's real-time interrupt-based multitasking with
//! static-executor's cooperative async/await multitasking.
//!
//! static-executor is run in RTIC's idle task, at lowest priority, so all RTIC
//! tasks will preempt async tasks if needed.
//!
//! Note that this is not fully safe: you must not use the softdevice's reserved
//! priorities for RTIC tasks. There is no compile-time checking for that for now.

#![no_main]
#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(min_type_alias_impl_trait)]
#![feature(impl_trait_in_bindings)]
#![feature(alloc_error_handler)]

#[path = "../example_common.rs"]
mod example_common;
use example_common::*;

use core::mem;
use defmt::*;
use embassy::executor::Executor;
use embassy::task;
use embassy::util::Forever;
use nrf52840_hal::pac::TIMER1;
use nrf52840_hal::prelude::*;
use nrf52840_hal::timer::{Periodic, Timer};
use rtic::app;

use nrf_softdevice::ble::peripheral;
use nrf_softdevice::{raw, temperature_celsius, Softdevice};

static EXECUTOR: Forever<Executor> = Forever::new();

#[task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

#[task]
async fn bluetooth_task(sd: &'static Softdevice) {
    #[rustfmt::skip]
    let adv_data = &[
        0x02, 0x01, raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8,
        0x03, 0x03, 0x09, 0x18,
        0x0a, 0x09, b'H', b'e', b'l', b'l', b'o', b'R', b'T', b'I', b'C',
    ];
    #[rustfmt::skip]
    let scan_data = &[
        0x03, 0x03, 0x09, 0x18,
    ];

    loop {
        let config = peripheral::Config::default();
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data,
            scan_data,
        };
        let _conn = unwrap!(peripheral::advertise(sd, adv, &config).await);

        info!("advertising done!");
    }
}

#[app(device = nrf52840_hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        timer: Timer<TIMER1, Periodic>,
        sd_peripherals: Option<nrf_softdevice::Peripherals>,
    }

    #[init()]
    fn init(cx: init::Context) -> init::LateResources {
        info!("init");

        let mut timer = Timer::new(cx.device.TIMER1);
        timer.enable_interrupt();
        let mut timer = timer.into_periodic();
        timer.start(1_000_000u32); // 1Mhz, so once per second

        let sd_peripherals = nrf_softdevice::Peripherals {
            AAR: cx.device.AAR,
            ACL: cx.device.ACL,
            CCM: cx.device.CCM,
            CLOCK: cx.device.CLOCK,
            ECB: cx.device.ECB,
            EGU1: cx.device.EGU1,
            EGU2: cx.device.EGU2,
            EGU5: cx.device.EGU5,
            MWU: cx.device.MWU,
            NVMC: cx.device.NVMC,
            POWER: cx.device.POWER,
            RADIO: cx.device.RADIO,
            RNG: cx.device.RNG,
            RTC0: cx.device.RTC0,
            SWI1: cx.device.SWI1,
            SWI2: cx.device.SWI2,
            SWI5: cx.device.SWI5,
            TEMP: cx.device.TEMP,
            TIMER0: cx.device.TIMER0,
        };

        init::LateResources {
            timer,
            sd_peripherals: Some(sd_peripherals),
        }
    }

    #[idle(resources=[sd_peripherals])]
    fn idle(cx: idle::Context) -> ! {
        let config = nrf_softdevice::Config {
            clock: Some(raw::nrf_clock_lf_cfg_t {
                source: raw::NRF_CLOCK_LF_SRC_XTAL as u8,
                rc_ctiv: 0,
                rc_temp_ctiv: 0,
                accuracy: 7,
            }),
            conn_gap: Some(raw::ble_gap_conn_cfg_t {
                conn_count: 6,
                event_length: 6,
            }),
            conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 128 }),
            gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
                attr_tab_size: 32768,
            }),
            gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
                adv_set_count: 1,
                periph_role_count: 3,
                central_role_count: 3,
                central_sec_count: 0,
                _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
            }),
            gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
                p_value: b"HelloRTIC" as *const u8 as _,
                current_len: 9,
                max_len: 9,
                write_perm: unsafe { mem::zeroed() },
                _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                    raw::BLE_GATTS_VLOC_STACK as u8,
                ),
            }),
            ..Default::default()
        };

        // Softdevice enable must not be done in RTIC init
        // because RTIC runs init with interrupts disabled, and the
        // softdevice crashes if it's enabled with interrupts disabled.
        let sdp = cx.resources.sd_peripherals.take().unwrap();
        let sd = Softdevice::enable(sdp, &config);

        let temp = unwrap!(temperature_celsius(&sd));
        info!("{}°C", temp.to_num::<i32>());

        let executor = EXECUTOR.put(Executor::new());
        executor.run(|spawner| {
            unwrap!(spawner.spawn(softdevice_task(sd)));
            unwrap!(spawner.spawn(bluetooth_task(sd)));
        });
    }

    #[task(binds = TIMER1, resources = [timer], priority = 1)]
    fn exec(cx: exec::Context) {
        cx.resources.timer.wait().unwrap();
        info!("tick");
    }
};
