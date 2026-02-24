use std::ffi::{c_char, c_int, CStr};

#[repr(C)]
pub struct NfcBridgeHandle {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn nfc_bridge_connect(out: *mut *mut NfcBridgeHandle, err: *mut c_char, err_len: usize) -> c_int;
    fn nfc_bridge_disconnect(handle: *mut NfcBridgeHandle);
    fn nfc_bridge_get_device_name(
        handle: *mut NfcBridgeHandle,
        out: *mut c_char,
        out_len: usize,
        err: *mut c_char,
        err_len: usize,
    ) -> c_int;
    fn nfc_bridge_scan(
        handle: *mut NfcBridgeHandle,
        uid_hex: *mut c_char,
        uid_len: usize,
        atqa_hex: *mut c_char,
        atqa_len: usize,
        sak_hex: *mut c_char,
        sak_len: usize,
        card_type: *mut c_char,
        card_type_len: usize,
        err: *mut c_char,
        err_len: usize,
    ) -> c_int;
    fn nfc_bridge_read_sector(
        handle: *mut NfcBridgeHandle,
        sector: u8,
        key: *const u8,
        key_type: u8,
        out_data: *mut u8,
        out_len: usize,
        err: *mut c_char,
        err_len: usize,
    ) -> c_int;
    fn nfc_bridge_write_block(
        handle: *mut NfcBridgeHandle,
        sector: u8,
        block: u8,
        data: *const u8,
        data_len: usize,
        key: *const u8,
        key_type: u8,
        err: *mut c_char,
        err_len: usize,
    ) -> c_int;
    fn nfc_bridge_probe(
        count_out: *mut usize,
        first_connstring: *mut c_char,
        first_connstring_len: usize,
        err: *mut c_char,
        err_len: usize,
    ) -> c_int;
}

pub struct NfcHandle {
    raw: *mut NfcBridgeHandle,
}

unsafe impl Send for NfcHandle {}

impl Drop for NfcHandle {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe { nfc_bridge_disconnect(self.raw) };
            self.raw = std::ptr::null_mut();
        }
    }
}

impl NfcHandle {
    pub fn probe() -> Result<(usize, String), String> {
        let mut count = 0usize;
        let mut first = [0 as c_char; 128];
        let mut err = [0 as c_char; 256];
        let ret = unsafe {
            nfc_bridge_probe(
                &mut count,
                first.as_mut_ptr(),
                first.len(),
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if ret != 0 {
            return Err(read_c_error(&err));
        }
        Ok((count, read_cstr(&first)))
    }

    pub fn connect() -> Result<Self, String> {
        let mut out = std::ptr::null_mut();
        let mut err = [0 as c_char; 256];
        let ret = unsafe { nfc_bridge_connect(&mut out, err.as_mut_ptr(), err.len()) };
        if ret != 0 {
            return Err(read_c_error(&err));
        }
        if out.is_null() {
            return Err("failed to create nfc handle".to_string());
        }
        Ok(Self { raw: out })
    }

    pub fn device_name(&self) -> Result<String, String> {
        let mut name = [0 as c_char; 128];
        let mut err = [0 as c_char; 256];
        let ret = unsafe {
            nfc_bridge_get_device_name(
                self.raw,
                name.as_mut_ptr(),
                name.len(),
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if ret != 0 {
            return Err(read_c_error(&err));
        }
        Ok(read_cstr(&name))
    }

    pub fn scan(&self) -> Result<(String, String, String, String), String> {
        let mut uid = [0 as c_char; 64];
        let mut atqa = [0 as c_char; 16];
        let mut sak = [0 as c_char; 16];
        let mut card_type = [0 as c_char; 64];
        let mut err = [0 as c_char; 256];

        let ret = unsafe {
            nfc_bridge_scan(
                self.raw,
                uid.as_mut_ptr(),
                uid.len(),
                atqa.as_mut_ptr(),
                atqa.len(),
                sak.as_mut_ptr(),
                sak.len(),
                card_type.as_mut_ptr(),
                card_type.len(),
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if ret != 0 {
            return Err(read_c_error(&err));
        }

        Ok((
            read_cstr(&uid),
            read_cstr(&atqa),
            read_cstr(&sak),
            read_cstr(&card_type),
        ))
    }

    pub fn read_sector(&self, sector: u8, key: [u8; 6], key_type: u8) -> Result<[u8; 64], String> {
        let mut out = [0u8; 64];
        let mut err = [0 as c_char; 256];
        let ret = unsafe {
            nfc_bridge_read_sector(
                self.raw,
                sector,
                key.as_ptr(),
                key_type,
                out.as_mut_ptr(),
                out.len(),
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if ret != 0 {
            return Err(read_c_error(&err));
        }
        Ok(out)
    }

    pub fn write_block(
        &self,
        sector: u8,
        block: u8,
        data: [u8; 16],
        key: [u8; 6],
        key_type: u8,
    ) -> Result<(), String> {
        let mut err = [0 as c_char; 256];
        let ret = unsafe {
            nfc_bridge_write_block(
                self.raw,
                sector,
                block,
                data.as_ptr(),
                data.len(),
                key.as_ptr(),
                key_type,
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if ret != 0 {
            return Err(read_c_error(&err));
        }
        Ok(())
    }
}

fn read_c_error(buf: &[c_char]) -> String {
    let msg = read_cstr(buf);
    if msg.is_empty() {
        "unknown nfc error".to_string()
    } else {
        msg
    }
}

fn read_cstr(buf: &[c_char]) -> String {
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .trim()
        .to_string()
}
