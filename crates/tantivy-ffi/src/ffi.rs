use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use crate::error::ffi_guard;
use tantivy_core::{query, registry, writer};

/// Convierte un puntero C a &str; error si es null o no-UTF8.
fn cstr<'a>(p: *const c_char) -> Result<&'a str, String> {
    if p.is_null() {
        return Err("puntero nulo".into());
    }
    unsafe { CStr::from_ptr(p) }
        .to_str()
        .map_err(|_| "cadena no UTF-8".into())
}

fn out(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

#[no_mangle]
pub extern "C" fn tv_version() -> *mut c_char {
    out(env!("CARGO_PKG_VERSION").to_string())
}

#[no_mangle]
pub extern "C" fn tv_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)) };
    }
}

#[no_mangle]
pub extern "C" fn tv_index_open_or_create(config_json: *const c_char) -> u64 {
    ffi_guard(0, || {
        let cfg = serde_json::from_str(cstr(config_json)?)
            .map_err(|e| format!("config JSON inválido: {e}"))?;
        registry::open_or_create(cfg)
    })
}

#[no_mangle]
pub extern "C" fn tv_index_open_readonly(config_json: *const c_char) -> u64 {
    ffi_guard(0, || {
        let cfg = serde_json::from_str(cstr(config_json)?)
            .map_err(|e| format!("config JSON inválido: {e}"))?;
        registry::open_read_only(cfg)
    })
}

#[no_mangle]
pub extern "C" fn tv_index_close(handle: u64) -> i32 {
    ffi_guard(-1, || Ok(if registry::close(handle) { 0 } else { -1 }))
}

#[no_mangle]
pub extern "C" fn tv_add_document(handle: u64, doc_json: *const c_char) -> i32 {
    ffi_guard(-1, || {
        let doc = cstr(doc_json)?.to_owned();
        registry::with_state(handle, |s| writer::add_document(s, &doc))?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tv_update_document(
    handle: u64,
    key_field: *const c_char,
    key_value: *const c_char,
    doc_json: *const c_char,
) -> i32 {
    ffi_guard(-1, || {
        let kf = cstr(key_field)?.to_owned();
        let kv = cstr(key_value)?.to_owned();
        let doc = cstr(doc_json)?.to_owned();
        registry::with_state(handle, |s| writer::update_document(s, &kf, &kv, &doc))?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tv_delete_document(
    handle: u64,
    key_field: *const c_char,
    key_value: *const c_char,
) -> i32 {
    ffi_guard(-1, || {
        let kf = cstr(key_field)?.to_owned();
        let kv = cstr(key_value)?.to_owned();
        registry::with_state(handle, |s| writer::delete_by_id(s, &kf, &kv))?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tv_commit(handle: u64) -> i32 {
    ffi_guard(-1, || {
        registry::with_state(handle, writer::commit)?;
        Ok(0)
    })
}

#[no_mangle]
pub extern "C" fn tv_optimize(_handle: u64) -> i32 {
    // v1: no-op (merge se agrega en el plan de rebuild). Nunca falla.
    0
}

#[no_mangle]
pub extern "C" fn tv_doc_count(handle: u64) -> i64 {
    ffi_guard(-1, || {
        let n = registry::with_state(handle, |s| s.doc_count())?;
        Ok(n as i64)
    })
}

#[no_mangle]
pub extern "C" fn tv_search(handle: u64, query_json: *const c_char) -> *mut c_char {
    ffi_guard(std::ptr::null_mut(), || {
        let q = cstr(query_json)?.to_owned();
        let json = registry::with_state(handle, |s| query::search(s, &q))?;
        Ok(out(json))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn version_roundtrips_through_c_string() {
        let ptr = tv_version();
        let got = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_owned();
        tv_string_free(ptr);
        assert_eq!(got, env!("CARGO_PKG_VERSION"));
    }
}
