#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

#[path = "../example_common.rs"]
mod example_common;
use example_common::*;

use core::mem;
use cortex_m_rt::entry;
use defmt::info;

use nrf_softdevice::ble::gatt_server::{Characteristic, CharacteristicHandles, RegisterError};
use nrf_softdevice::ble::{gatt_server, peripheral, Connection, Uuid};
use nrf_softdevice::{raw, RawError, Softdevice};

#[static_executor::task]
async fn softdevice_task(sd: &'static Softdevice) {
    sd.run().await;
}

const GATT_BAS_SVC_UUID: Uuid = Uuid::new_16(0x180F);
const GATT_BAS_BATTERY_LEVEL_CHAR_UUID: Uuid = Uuid::new_16(0x2A19);

struct BatteryServiceServer {
    battery_level_value_handle: u16,
    battery_level_cccd_handle: u16,
}

enum BatteryServiceEvent {
    BatteryLevelNotificationsEnabled,
    BatteryLevelNotificationsDisabled,
}

// This is boilerplate, ideally it'll be generated with a proc macro in the future.
impl BatteryServiceServer {
    fn battery_level_get(&self, sd: &Softdevice) -> Result<u8, gatt_server::GetValueError> {
        let buf = &mut [0u8; 0];
        gatt_server::get_value(sd, self.battery_level_value_handle, buf)?;
        Ok(buf[0])
    }

    fn battery_level_set(
        &self,
        sd: &Softdevice,
        val: u8,
    ) -> Result<(), gatt_server::SetValueError> {
        gatt_server::set_value(sd, self.battery_level_value_handle, &[val])
    }

    fn battery_level_notify(
        &self,
        conn: &Connection,
        val: u8,
    ) -> Result<(), gatt_server::NotifyValueError> {
        gatt_server::notify_value(conn, self.battery_level_value_handle, &[val])
    }
}

// This is boilerplate, ideally it'll be generated with a proc macro in the future.
impl gatt_server::Server for BatteryServiceServer {
    type Event = BatteryServiceEvent;

    fn uuid() -> Uuid {
        GATT_BAS_SVC_UUID
    }

    fn register<F>(service_handle: u16, mut register_char: F) -> Result<Self, RegisterError>
    where
        F: FnMut(Characteristic, &[u8]) -> Result<CharacteristicHandles, RegisterError>,
    {
        let battery_level = register_char(
            Characteristic {
                uuid: GATT_BAS_BATTERY_LEVEL_CHAR_UUID,
                can_indicate: false,
                can_notify: true,
                can_read: true,
                can_write: false,
                max_len: 1,
            },
            &[123],
        )?;

        Ok(Self {
            battery_level_cccd_handle: battery_level.cccd_handle,
            battery_level_value_handle: battery_level.value_handle,
        })
    }

    fn on_write(&self, handle: u16, data: &[u8]) -> Option<Self::Event> {
        if handle == self.battery_level_cccd_handle {
            if !data.is_empty() && data[0] & 0x01 != 0 {
                return Some(BatteryServiceEvent::BatteryLevelNotificationsEnabled);
            } else {
                return Some(BatteryServiceEvent::BatteryLevelNotificationsDisabled);
            }
        }
        None
    }
}

#[static_executor::task]
async fn bluetooth_task(sd: &'static Softdevice) {
    let server: BatteryServiceServer = gatt_server::register(sd).dewrap();

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
        let conn = peripheral::advertise(
            sd,
            peripheral::ConnectableAdvertisement::ScannableUndirected {
                adv_data,
                scan_data,
            },
        )
        .await
        .dewrap();

        info!("advertising done!");

        // Run the GATT server on the connection. This returns when the connection gets disconnected.
        let res = gatt_server::run(&conn, &server, |e| match e {
            BatteryServiceEvent::BatteryLevelNotificationsEnabled => {
                info!("battery notifications enabled")
            }
            BatteryServiceEvent::BatteryLevelNotificationsDisabled => {
                info!("battery notifications disabled")
            }
        })
        .await;

        if let Err(e) = res {
            info!("gatt_server run exited with error: {:?}", e);
        }
    }
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

    unsafe {
        softdevice_task.spawn(sd).dewrap();
        bluetooth_task.spawn(sd).dewrap();

        static_executor::run();
    }
}
