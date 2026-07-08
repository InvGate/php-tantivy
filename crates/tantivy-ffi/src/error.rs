use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::c_char;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Guarda el último error del thread actual.
pub fn set_last_error(msg: &str) {
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(msg.to_owned()));
}

/// Consume y devuelve el último error del thread actual.
pub fn take_last_error() -> Option<String> {
    LAST_ERROR.with(|e| e.borrow_mut().take())
}

/// Ejecuta `f`, atrapa panics y errores, guarda el mensaje y devuelve `default` ante fallo.
pub fn ffi_guard<T>(default: T, f: impl FnOnce() -> Result<T, String> + std::panic::UnwindSafe) -> T {
    match std::panic::catch_unwind(f) {
        Ok(Ok(v)) => v,
        Ok(Err(msg)) => {
            set_last_error(&msg);
            default
        }
        Err(_) => {
            set_last_error("panic en el borde FFI");
            default
        }
    }
}

#[no_mangle]
pub extern "C" fn tv_last_error() -> *mut c_char {
    match take_last_error() {
        Some(msg) => CString::new(msg).unwrap_or_default().into_raw(),
        None => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_stores_error_and_returns_default() {
        let out = ffi_guard(-1_i64, || Err("boom".to_string()));
        assert_eq!(out, -1);
        assert_eq!(take_last_error(), Some("boom".to_string()));
    }

    #[test]
    fn guard_catches_panic() {
        let out = ffi_guard(0_u64, || panic!("kaboom"));
        assert_eq!(out, 0);
        assert!(take_last_error().unwrap().contains("panic"));
    }
}
