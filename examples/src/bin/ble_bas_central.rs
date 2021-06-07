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
use defmt::info;
use defmt::*;
use embassy::executor::Executor;
use embassy::util::Forever;

use nrf_softdevice::ble::{central, gatt_client, Address, AddressType};
use nrf_softdevice::raw;
use nrf_softdevice::Softdevice;

static EXECUTOR: Forever<Executor> = Forever::new();

#[embassy::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

#[nrf_softdevice::gatt_client(uuid = "180f")]
struct BatteryServiceClient {
    #[characteristic(uuid = "2a19", read, write, notify)]
    battery_level: u8,
}

#[embassy::task]
async fn ble_central_task(sd: &'static Softdevice) {
    let addrs = &[&Address::new(
        AddressType::RandomStatic,
        [0x06, 0x6b, 0x71, 0x2c, 0xf5, 0xc0],
    )];
    let mut config = central::ConnectConfig::default();
    config.scan_config.whitelist = Some(addrs);
    let conn = unwrap!(central::connect(sd, &config).await);
    info!("connected");

    let client: BatteryServiceClient = unwrap!(gatt_client::discover(&conn).await);

    // Read
    let val = unwrap!(client.battery_level_read().await);
    info!("read battery level: {}", val);

    // Write, set it to 42
    unwrap!(client.battery_level_write(42).await);
    info!("Wrote battery level!");

    // Read to check it's changed
    let val = unwrap!(client.battery_level_read().await);
    info!("read battery level: {}", val);
}

#[entry]
fn main() -> ! {
    info!("Hello World!");

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
        unwrap!(spawner.spawn(ble_central_task(sd)));
    });
}
