#![no_std]
#![no_main]
#![feature(const_mut_refs)]
#![feature(const_fn)]

extern crate esp32_sys;

mod debug;
mod gatt_svr;

use core::ffi::c_void;
use core::mem::size_of;
use core::panic::PanicInfo;
use core::ptr;
use esp32_sys::*;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    unsafe { esp_log!(BLE_HR_TAG, cstr!("Panic!\n")) };

    loop {}
}

const BLINK_GPIO: gpio_num_t = gpio_num_t_GPIO_NUM_5;
const UART_NUM: uart_port_t = uart_port_t_UART_NUM_1;
const ECHO_TEST_TXD: i32 = gpio_num_t_GPIO_NUM_17 as i32;
const ECHO_TEST_RXD: i32 = gpio_num_t_GPIO_NUM_16 as i32;
const ECHO_TEST_RTS: i32 = UART_PIN_NO_CHANGE;
const ECHO_TEST_CTS: i32 = UART_PIN_NO_CHANGE;

const BUF_SIZE: i32 = 1024;
// https://github.com/espressif/esp-idf/tree/0a03a55c1eb44a354c9ad5d91d91da371fe23f84/examples/bluetooth/nimble/blehr

// blehr_sens.h
//
/*

#define BLE_UUID16_DECLARE(uuid16) \
    ((ble_uuid_t *) (&(ble_uuid16_t) BLE_UUID16_INIT(uuid16)))
((ble_uuid_t *) (&(ble_uuid16_t) BLE_UUID16_INIT(uuid16)))


#define BLE_UUID16_INIT(uuid16)         \
    {                                   \
        .u.type = BLE_UUID_TYPE_16,     \
        .value = (uuid16),              \
    }
*/

const fn ble_uuid16_declare(value: u16) -> *const ble_uuid_t {
    &ble_uuid16_t {
        u: ble_uuid_t {
            type_: BLE_UUID_TYPE_16 as u8,
        },
        value,
    } as *const ble_uuid16_t as *const ble_uuid_t
}

const fn null_ble_gatt_chr_def() -> ble_gatt_chr_def {
    return ble_gatt_chr_def {
        uuid: ptr::null(),
        access_cb: None,
        arg: ptr::null_mut(),
        descriptors: ptr::null_mut(),
        flags: 0,
        min_key_size: 0,
        val_handle: ptr::null_mut(),
    };
}

const fn null_ble_gatt_svc_def() -> ble_gatt_svc_def {
    return ble_gatt_svc_def {
        type_: BLE_GATT_SVC_TYPE_END as u8,
        uuid: ptr::null(),
        includes: ptr::null_mut(),
        characteristics: ptr::null(),
    };
}

/* Heart-rate configuration */
const GATT_HRS_UUID: u16 = 0x180D;
const GATT_HRS_MEASUREMENT_UUID: u16 = 0x2A37;
const GATT_HRS_BODY_SENSOR_LOC_UUID: u16 = 0x2A38;
const GATT_DEVICE_INFO_UUID: u16 = 0x180A;
const GATT_MANUFACTURER_NAME_UUID: u16 = 0x2A29;
const GATT_MODEL_NUMBER_UUID: u16 = 0x2A24;

//struct ble_hs_cfg;
//struct ble_gatt_register_ctxt;
//void gatt_svr_register_cb(struct ble_gatt_register_ctxt *ctxt, void *arg);
//int gatt_svr_init(void);

// gatt_svr.c

const MANUF_NAME: &str = "Apache Mynewt ESP32 devkitC\0";
const MODEL_NUM: &str = "Mynewt HR Sensor demo\0";
static mut HRS_HRM_HANDLE: u16 = 0;

const BLE_TAG: *const i8 = cstr!("BLE");

extern "C" fn gatt_svr_chr_access_heart_rate(
    _conn_handle: u16,
    _attr_handle: u16,
    ctxt: *mut ble_gatt_access_ctxt,
    _arg: *mut ::core::ffi::c_void,
) -> i32 {
    /* Sensor location, set to "Chest" */
    const BODY_SENS_LOC: u8 = 0x01;

    let uuid: u16 = unsafe { ble_uuid_u16((*(*ctxt).__bindgen_anon_1.chr).uuid) };

    if uuid == GATT_HRS_BODY_SENSOR_LOC_UUID {
        let rc: i32 = unsafe {
            os_mbuf_append(
                (*ctxt).om,
                &BODY_SENS_LOC as *const u8 as *const c_void,
                size_of::<u8>() as u16,
            )
        };

        return if rc == 0 {
            0
        } else {
            BLE_ATT_ERR_INSUFFICIENT_RES as i32
        };
    }

    return BLE_ATT_ERR_UNLIKELY as i32;
}

extern "C" fn gatt_svr_chr_access_device_info(
    _conn_handle: u16,
    _attr_handle: u16,
    ctxt: *mut ble_gatt_access_ctxt,
    _arg: *mut ::core::ffi::c_void,
) -> i32 {
    let uuid: u16 = unsafe { ble_uuid_u16((*(*ctxt).__bindgen_anon_1.chr).uuid) };

    if uuid == GATT_MODEL_NUMBER_UUID {
        let rc: i32 = unsafe {
            os_mbuf_append(
                (*ctxt).om,
                MODEL_NUM.as_ptr() as *const c_void,
                MODEL_NUM.len() as u16,
            )
        };
        return if rc == 0 {
            0
        } else {
            BLE_ATT_ERR_INSUFFICIENT_RES as i32
        };
    }

    if uuid == GATT_MANUFACTURER_NAME_UUID {
        let rc: i32 = unsafe {
            os_mbuf_append(
                (*ctxt).om,
                MANUF_NAME.as_ptr() as *const c_void,
                MANUF_NAME.len() as u16,
            )
        };
        return if rc == 0 {
            0
        } else {
            BLE_ATT_ERR_INSUFFICIENT_RES as i32
        };
    }

    return BLE_ATT_ERR_UNLIKELY as i32;
}

unsafe extern "C" fn gatt_svr_register_cb(
    ctxt: *mut ble_gatt_register_ctxt,
    _arg: *mut ::core::ffi::c_void,
) {
    let mut buf_arr: [i8; BLE_UUID_STR_LEN as usize] = [0; BLE_UUID_STR_LEN as usize];
    let buf = buf_arr.as_mut_ptr();

    match (*ctxt).op as u32 {
        BLE_GATT_REGISTER_OP_SVC => {
            esp_log!(
                BLE_TAG,
                cstr!("registered service %s with handle=%d\n"),
                ble_uuid_to_str((*(*ctxt).__bindgen_anon_1.svc.svc_def).uuid, buf),
                (*ctxt).__bindgen_anon_1.svc.handle as i32
            );
        }

        BLE_GATT_REGISTER_OP_CHR => {
            esp_log!(
                BLE_TAG,
                cstr!("registering characteristic %s with def_handle=%d val_handle=%d\n"),
                ble_uuid_to_str((*(*ctxt).__bindgen_anon_1.chr.chr_def).uuid, buf),
                (*ctxt).__bindgen_anon_1.chr.def_handle as i32,
                (*ctxt).__bindgen_anon_1.chr.val_handle as i32
            );
        }

        BLE_GATT_REGISTER_OP_DSC => {
            esp_log!(
                BLE_TAG,
                cstr!("registering descriptor %s with handle=%d\n"),
                ble_uuid_to_str((*(*ctxt).__bindgen_anon_1.dsc.dsc_def).uuid, buf),
                (*ctxt).__bindgen_anon_1.dsc.handle as i32
            );
        }
        _ => esp_log!(BLE_TAG, cstr!("unknown operation: %d\n"), (*ctxt).op as u32),
    }
}

unsafe fn gatt_svr_init() -> i32 {
    let svcs: [ble_gatt_svc_def; 3] = [
        ble_gatt_svc_def {
            type_: BLE_GATT_SVC_TYPE_PRIMARY as u8,
            uuid: ble_uuid16_declare(GATT_HRS_UUID),
            includes: ptr::null_mut(),
            characteristics: [
                ble_gatt_chr_def {
                    uuid: ble_uuid16_declare(GATT_HRS_MEASUREMENT_UUID),
                    access_cb: Some(gatt_svr_chr_access_heart_rate),
                    arg: ptr::null_mut(),
                    descriptors: ptr::null_mut(),
                    flags: BLE_GATT_CHR_F_NOTIFY as u16,
                    min_key_size: 0,
                    val_handle: &mut HRS_HRM_HANDLE as *mut u16,
                },
                ble_gatt_chr_def {
                    uuid: ble_uuid16_declare(GATT_HRS_BODY_SENSOR_LOC_UUID),
                    access_cb: Some(gatt_svr_chr_access_heart_rate),
                    arg: ptr::null_mut(),
                    descriptors: ptr::null_mut(),
                    flags: BLE_GATT_CHR_F_READ as u16,
                    min_key_size: 0,
                    val_handle: ptr::null_mut(),
                },
                null_ble_gatt_chr_def(),
            ]
            .as_ptr(),
        },
        ble_gatt_svc_def {
            type_: BLE_GATT_SVC_TYPE_PRIMARY as u8,
            uuid: ble_uuid16_declare(GATT_DEVICE_INFO_UUID),
            includes: ptr::null_mut(),
            characteristics: [
                ble_gatt_chr_def {
                    uuid: ble_uuid16_declare(GATT_MANUFACTURER_NAME_UUID),
                    access_cb: Some(gatt_svr_chr_access_device_info),
                    arg: ptr::null_mut(),
                    descriptors: ptr::null_mut(),
                    flags: BLE_GATT_CHR_F_READ as u16,
                    min_key_size: 0,
                    val_handle: ptr::null_mut(),
                },
                ble_gatt_chr_def {
                    uuid: ble_uuid16_declare(GATT_MODEL_NUMBER_UUID),
                    access_cb: Some(gatt_svr_chr_access_device_info),
                    arg: ptr::null_mut(),
                    descriptors: ptr::null_mut(),
                    flags: BLE_GATT_CHR_F_READ as u16,
                    min_key_size: 0,
                    val_handle: ptr::null_mut(),
                },
                null_ble_gatt_chr_def(),
            ]
            .as_ptr(),
        },
        null_ble_gatt_svc_def(),
    ];

    print_ptr(cstr!("type"), &svcs[0].type_);
    print_ptr(cstr!("uuid"), &svcs[0].uuid);
    print_ptr(cstr!("includes"), &svcs[0].includes);
    print_ptr(cstr!("characteristics"), &svcs[0].characteristics);
    print_ptr(cstr!("svcs[0]"), &svcs[0]);
    print_ptr(cstr!("svcs.as_ptr()"), svcs.as_ptr());
    print_ptr(cstr!("svcs[1]"), &svcs[0]);
    print_ptr(cstr!("svcs.as_ptr().add(1)"), svcs.as_ptr().add(1));
    printf(cstr!("type[0] %d\n"), (*svcs.as_ptr()).type_ as u32);
    printf(cstr!("type[1] %d\n"), (*svcs.as_ptr().add(1)).type_ as u32);

    ble_svc_gap_init();
    ble_svc_gatt_init();

    let mut rc;

    rc = ble_gatts_count_cfg(svcs.as_ptr());
    esp_log!(BLE_HR_TAG, cstr!("RC is %d\n"), rc);
    esp_assert!(rc == 0, cstr!("RC err after ble_gatts_count_cfg\n"));

    rc = ble_gatts_add_svcs(svcs.as_ptr());
    esp_assert!(rc == 0, cstr!("RC err after ble_gatts_add_svcs\n"));

    return 0;
}

// main.c

static mut BLE_HR_TAG: *const i8 = cstr!("NimBLE_BLE_HeartRate");

static mut BLEHR_TX_TIMER: TimerHandle_t = ptr::null_mut();

static mut NOTIFY_STATE: bool = false;

static mut CONN_HANDLE: u16 = 0;

const DEVICE_NAME: &str = "blehr_sensor_1.0\0";

static mut BLEHR_ADDRESS_TYPE: u8 = 0;

/* Variable to simulate heart beats */
static mut HEARTRATE: u8 = 90;

unsafe fn print_bytes(bytes: *const u8, len: usize) {
    let u8p: &[u8];

    u8p = core::slice::from_raw_parts(bytes, len);

    for i in 0..len {
        if (i & 0b1111) == 0 && i > 0 {
            printf(cstr!("\n"));
        } else if (i & 0b1) == 0 {
            printf(cstr!(" "));
        }
        printf(cstr!("%02x"), u8p[i] as u32);
    }
}

unsafe fn print_ptr<T>(name: *const u8, p: *const T) {
    printf(cstr!("%p - %s:\n"), p, name);
    print_bytes(p as *const _, size_of::<T>());
    printf(cstr!("\n"));
}

unsafe fn print_addr(addr: *const c_void) {
    let u8p: &[u8];

    u8p = core::slice::from_raw_parts(addr as *const u8, 6);
    esp_log!(
        BLE_HR_TAG,
        cstr!("%02x:%02x:%02x:%02x:%02x:%02x"),
        u8p[5] as u32,
        u8p[4] as u32,
        u8p[3] as u32,
        u8p[2] as u32,
        u8p[1] as u32,
        u8p[0] as u32
    );
}

unsafe fn blehr_tx_hrate_stop() {
    xTimerStop(BLEHR_TX_TIMER, 1000 / portTICK_PERIOD_MS);
}

/* Reset heartrate measurment */
unsafe fn blehr_tx_hrate_reset() {
    let rc;

    if xTimerReset(BLEHR_TX_TIMER, 1000 / portTICK_PERIOD_MS) == pdPASS {
        rc = 0;
    } else {
        rc = 1;
    }
    assert!(rc == 0);
}

/* This function simulates heart beat and notifies it to the client */
unsafe extern "C" fn blehr_tx_hrate(_ev: TimerHandle_t) {
    let mut hrm: [u8; 2] = [0; 2];
    let rc;
    let om: *mut os_mbuf;

    if !NOTIFY_STATE {
        blehr_tx_hrate_stop();
        HEARTRATE = 90;
        return;
    }

    hrm[0] = 0x06; /* contact of a sensor */
    hrm[1] = HEARTRATE; /* storing dummy data */

    /* Simulation of heart beats */
    HEARTRATE += 1;
    if HEARTRATE == 160 {
        HEARTRATE = 90;
    }

    om = ble_hs_mbuf_from_flat(hrm.as_ptr() as *const _, size_of::<[u8; 2]>() as u16);
    rc = ble_gattc_notify_custom(CONN_HANDLE, HRS_HRM_HANDLE, om);

    assert!(rc == 0);

    blehr_tx_hrate_reset();
}

unsafe extern "C" fn blehr_gap_event(
    event: *mut ble_gap_event,
    _arg: *mut ::core::ffi::c_void,
) -> i32 {
    match (*event).type_ as u32 {
        BLE_GAP_EVENT_CONNECT => {
            /* A new connection was established or a connection attempt failed */
            esp_log!(
                BLE_HR_TAG,
                cstr!("connection %s; status=%d\n"),
                if (*event).__bindgen_anon_1.connect.status == 0 {
                    cstr!("established")
                } else {
                    cstr!("failed")
                },
                (*event).__bindgen_anon_1.connect.status
            );

            if (*event).__bindgen_anon_1.connect.status != 0 {
                /* Connection failed; resume advertising */
                blehr_advertise();
            }
            CONN_HANDLE = (*event).__bindgen_anon_1.connect.conn_handle;
        }

        BLE_GAP_EVENT_DISCONNECT => {
            esp_log!(
                BLE_HR_TAG,
                cstr!("disconnect; reason=%d\n"),
                (*event).__bindgen_anon_1.disconnect.reason
            );

            /* Connection terminated; resume advertising */
            blehr_advertise();
        }

        BLE_GAP_EVENT_ADV_COMPLETE => {
            esp_log!(BLE_HR_TAG, cstr!("adv complete\n"));
            blehr_advertise();
        }

        BLE_GAP_EVENT_SUBSCRIBE => {
            esp_log!(
                BLE_HR_TAG,
                cstr!("subscribe event; cur_notify=%d\n value handle; val_handle=%d\n"),
                (*event).__bindgen_anon_1.subscribe.cur_notify() as u32,
                HRS_HRM_HANDLE as u32
            );
            if (*event).__bindgen_anon_1.subscribe.attr_handle == HRS_HRM_HANDLE {
                NOTIFY_STATE = (*event).__bindgen_anon_1.subscribe.cur_notify() != 0;
                blehr_tx_hrate_reset();
            } else if (*event).__bindgen_anon_1.subscribe.attr_handle != HRS_HRM_HANDLE {
                NOTIFY_STATE = (*event).__bindgen_anon_1.subscribe.cur_notify() != 0;
                blehr_tx_hrate_stop();
            }
            esp_log!(
                cstr!("BLE_GAP_SUBSCRIBE_EVENT"),
                cstr!("conn_handle from subscribe=%d\n"),
                CONN_HANDLE as u32,
            );
        }

        BLE_GAP_EVENT_MTU => {
            esp_log!(
                BLE_HR_TAG,
                cstr!("mtu update event; conn_handle=%d mtu=%d\n"),
                (*event).__bindgen_anon_1.mtu.conn_handle as u32,
                (*event).__bindgen_anon_1.mtu.value as u32,
            );
        }
        _ => esp_log!(
            BLE_HR_TAG,
            cstr!("unknown operation: %d\n"),
            (*event).type_ as u32
        ),
    }

    return 0;
}

/*
 * Enables advertising with parameters:
 *     o General discoverable mode
 *     o Undirected connectable mode
 */
unsafe fn blehr_advertise() {
    let mut fields: ble_hs_adv_fields = core::mem::MaybeUninit::zeroed().assume_init();
    /*
     * Advertise two flags:
     *      o Discoverability in forthcoming advertisement (general)
     *      o BLE-only (BR/EDR unsupported)
     */
    fields.flags = BLE_HS_ADV_F_DISC_GEN as u8 | BLE_HS_ADV_F_BREDR_UNSUP as u8;

    /*
     * Indicate that the TX power level field should be included; have the
     * stack fill this value automatically.  This is done by assigning the
     * special value BLE_HS_ADV_TX_PWR_LVL_AUTO.
     */
    fields.set_tx_pwr_lvl_is_present(1);
    fields.tx_pwr_lvl = BLE_HS_ADV_TX_PWR_LVL_AUTO as i8;

    fields.name = DEVICE_NAME.as_ptr() as *mut u8;
    fields.name_len = DEVICE_NAME.len() as u8;
    fields.set_name_is_complete(1);

    let mut rc = ble_gap_adv_set_fields(&fields);
    if rc != 0 {
        esp_log!(
            BLE_HR_TAG,
            cstr!("error setting advertisement data; rc=%d\n"),
            rc
        );
        return;
    }

    /* Begin advertising */
    let mut adv_params: ble_gap_adv_params = std::mem::MaybeUninit::zeroed().assume_init();
    adv_params.conn_mode = BLE_GAP_CONN_MODE_UND as u8;
    adv_params.disc_mode = BLE_GAP_DISC_MODE_GEN as u8;
    rc = ble_gap_adv_start(
        BLEHR_ADDRESS_TYPE,
        ptr::null(),
        core::i32::MAX,
        &adv_params,
        Some(blehr_gap_event),
        ptr::null_mut(),
    );
    if rc != 0 {
        esp_log!(
            BLE_HR_TAG,
            cstr!("error enabling advertisement; rc=%d\n"),
            rc
        );
        return;
    }
}

unsafe extern "C" fn blehr_on_sync() {
    let mut rc;

    rc = ble_hs_id_infer_auto(0, &mut BLEHR_ADDRESS_TYPE as *mut _);
    assert!(rc == 0);

    let addr_val: *mut u8 = [0; 6].as_mut_ptr();
    rc = ble_hs_id_copy_addr(BLEHR_ADDRESS_TYPE, addr_val, ptr::null_mut());
    assert!(rc == 0);

    esp_log!(BLE_HR_TAG, cstr!("Device Address: "));
    print_addr(addr_val as *const c_void);
    esp_log!(BLE_HR_TAG, cstr!("\n"));

    /* Begin advertising */
    blehr_advertise();
}

unsafe extern "C" fn blehr_on_reset(reason: i32) {
    esp_log!(BLE_HR_TAG, cstr!("Resetting state; reason=%d\n"), reason);
}

unsafe extern "C" fn blehr_host_task(_param: *mut c_void) {
    esp_log!(BLE_HR_TAG, cstr!("BLE Host Task Started\n"));
    /* This function will return only when nimble_port_stop() is executed */
    nimble_port_run();

    nimble_port_freertos_deinit();
}

unsafe fn init_bt() {
    /* Initialize NVS — it is used to store PHY calibration data */
    let mut ret: esp_err_t = nvs_flash_init();
    if ret == ESP_ERR_NVS_NO_FREE_PAGES as i32 || ret == ESP_ERR_NVS_NEW_VERSION_FOUND as i32 {
        esp_error_check!(nvs_flash_erase());
        ret = nvs_flash_init();
    }
    esp_error_check!(ret);

    esp_error_check!(esp_nimble_hci_and_controller_init());

    nimble_port_init();
    /* Initialize the NimBLE host configuration */
    ble_hs_cfg.sync_cb = Some(blehr_on_sync);
    ble_hs_cfg.reset_cb = Some(blehr_on_reset);
    ble_hs_cfg.gatts_register_cb = Some(gatt_svr_register_cb);

    /* name, period/time,  auto reload, timer ID, callback */
    BLEHR_TX_TIMER = xTimerCreate(
        cstr!("blehr_tx_timer"),
        pdMS_TO_TICKS!(1000),
        pdTRUE,
        ptr::null_mut(),
        Some(blehr_tx_hrate),
    );

    let mut rc: i32 = 0;
    rc = gatt_svr_init();
    esp_assert!(rc == 0, cstr!("gatt_svr_init failed\n"));

    /* Set the default device name */
    rc = ble_svc_gap_device_name_set(cstr!("Fake Device Name"));
    esp_assert!(rc == 0, cstr!("ble_svc_gap_device_name_set failed\n"));

    /* Start the task */
    nimble_port_freertos_init(Some(blehr_host_task));
}

#[no_mangle]
pub fn app_main() {
    unsafe {
        esp_log!(BLE_HR_TAG, cstr!("Setting up!\n"));
        init_bt();
        esp_log!(BLE_HR_TAG, cstr!("BT init!\n"));

        rust_blink_and_write();
    }
}

unsafe fn rust_blink_and_write() {
    gpio_pad_select_gpio(BLINK_GPIO as u8);

    /* Set the GPIO as a push/pull output */
    gpio_set_direction(BLINK_GPIO, gpio_mode_t_GPIO_MODE_OUTPUT);

    /* Configure parameters of an UART driver,
     * communication pins and install the driver */
    let uart_config = uart_config_t {
        baud_rate: 115200,
        data_bits: uart_word_length_t_UART_DATA_8_BITS,
        parity: uart_parity_t_UART_PARITY_DISABLE,
        stop_bits: uart_stop_bits_t_UART_STOP_BITS_1,
        flow_ctrl: uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE,
        rx_flow_ctrl_thresh: 0,
        use_ref_tick: false,
    };

    uart_param_config(UART_NUM, &uart_config);
    uart_set_pin(
        UART_NUM,
        ECHO_TEST_TXD,
        ECHO_TEST_RXD,
        ECHO_TEST_RTS,
        ECHO_TEST_CTS,
    );
    uart_driver_install(UART_NUM, BUF_SIZE * 2, 0, 0, ptr::null_mut(), 0);

    loop {
        /* Blink off (output low) */
        gpio_set_level(BLINK_GPIO, 0);

        vTaskDelay(1000 / portTICK_PERIOD_MS);

        // Write data to UART.
        //let test_str = "This is a test string.\n";
        // uart_write_bytes(UART_NUM, test_str.as_ptr() as *const _, test_str.len());
        let tag = "Rust\0";

        esp_log_write(
            esp_log_level_t_ESP_LOG_INFO,
            tag.as_ptr() as *const _,
            " (%d) %s: %s\n\0".as_ptr() as *const _,
            esp_log_timestamp(),
            tag.as_ptr() as *const _,
            "I live again!.\0".as_ptr() as *const _,
        );

        // esp_log!(BLE_TAG, "a string\n",);

        /* Blink on (output high) */
        gpio_set_level(BLINK_GPIO, 1);

        vTaskDelay(1000 / portTICK_PERIOD_MS);
    }
}
