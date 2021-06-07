#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]
#![feature(min_type_alias_impl_trait)]
#![feature(impl_trait_in_bindings)]
#![allow(incomplete_features)]

#[path = "../example_common.rs"]
mod example_common;

use core::mem;
use cortex_m_rt::entry;
use defmt::*;
use embassy::executor::Executor;
use embassy::traits::gpio::WaitForLow;
use embassy::util::Forever;
use embassy_nrf::gpio::{AnyPin, Input, Pin as _, Pull};
use embassy_nrf::gpiote::PortInput;
use embassy_nrf::interrupt::Priority;
use futures::pin_mut;

use nrf_softdevice::ble::{gatt_server, peripheral};
use nrf_softdevice::{raw, Softdevice};

static EXECUTOR: Forever<Executor> = Forever::new();

#[embassy::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

#[nrf_softdevice::gatt_server(uuid = "9e7312e0-2354-11eb-9f10-fbc30a62cf38")]
struct FooService {
    #[characteristic(uuid = "9e7312e0-2354-11eb-9f10-fbc30a63cf38", read, write, notify)]
    foo: u16,
}

async fn run_bluetooth(sd: &'static Softdevice, server: &FooService) {
    #[rustfmt::skip]
    let adv_data = &[
        0x02, 0x01, raw::BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE as u8,
        0x03, 0x03, 0x09, 0x18,
        0x0a, 0x09, b'H', b'e', b'l', b'l', b'o', b'R', b'u', b's', b't',
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
        let conn = unwrap!(peripheral::advertise(sd, adv, &config).await);

        info!("advertising done!");

        let res = gatt_server::run(&conn, server, |e| match e {
            FooServiceEvent::FooWrite(val) => {
                info!("wrote foo level: {}", val);
                if let Err(e) = server.foo_notify(&conn, val + 1) {
                    info!("send notification error: {:?}", e);
                }
            }
            FooServiceEvent::FooNotificationsEnabled => info!("notifications enabled"),
            FooServiceEvent::FooNotificationsDisabled => info!("notifications disabled"),
        })
        .await;

        if let Err(e) = res {
            info!("gatt_server run exited with error: {:?}", e);
        }
    }
}

#[embassy::task]
async fn bluetooth_task(sd: &'static Softdevice, button1: AnyPin, button2: AnyPin) {
    let server: FooService = unwrap!(gatt_server::register(sd));

    info!("Bluetooth is OFF");
    info!("Press nrf52840-dk button 1 to enable, button 2 to disable");

    let button1 = PortInput::new(Input::new(button1, Pull::Up));
    let button2 = PortInput::new(Input::new(button2, Pull::Up));
    pin_mut!(button1);
    pin_mut!(button2);
    loop {
        button1.as_mut().wait_for_low().await;
        info!("Bluetooth ON!");

        // Create a future that will run the bluetooth loop.
        // Note the lack of `.await`! This creates the future but doesn't poll it yet.
        let bluetooth_fut = run_bluetooth(sd, &server);

        // Create a future that will resolve when the OFF button is pressed.
        let off_fut = async {
            button2.as_mut().wait_for_low().await;
            info!("Bluetooth OFF!");
        };

        pin_mut!(bluetooth_fut);
        pin_mut!(off_fut);

        // Select the two futures.
        //
        // select() returns when one of the two futures returns. The other future is dropped before completing.
        //
        // Since the bluetooth future never finishes, this can only happen when the Off button is pressed.
        // This will cause the bluetooth future to be dropped.
        //
        // If it was advertising, the nested `peripheral::advertise` future will be dropped, which will cause
        // the softdevice to stop advertising.
        // If it was connected, it will drop everything including the `Connection` instance, which
        // will tell the softdevice to disconnect it.
        //
        // This demonstrates the awesome power of Rust's async-await combined with nrf-softdevice's async wrappers.
        // It's super easy to cancel a complex tree of operations: just drop its future!
        futures::future::select(bluetooth_fut, off_fut).await;
    }
}

#[entry]
fn main() -> ! {
    info!("Hello World!");

    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = Priority::P2;
    let p = embassy_nrf::init(config);

    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_XTAL as u8,
            rc_ctiv: 0,
            rc_temp_ctiv: 0,
            accuracy: 7,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 6,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 256 }),
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
            p_value: b"HelloRust" as *const u8 as _,
            current_len: 9,
            max_len: 9,
            write_perm: unsafe { mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&config);

    let executor = EXECUTOR.put(Executor::new());
    executor.run(|spawner| {
        unwrap!(spawner.spawn(softdevice_task(sd)));
        unwrap!(spawner.spawn(bluetooth_task(sd, p.P0_11.degrade(), p.P0_12.degrade())));
    });
}
