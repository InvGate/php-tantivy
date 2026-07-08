// las funciones del borde C-ABI reciben punteros crudos por diseño (contrato con el caller C/PHP);
// el lint not_unsafe_ptr_arg_deref no aplica a funciones extern "C" pensadas para llamarse desde C.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod error;
mod ffi;

pub use error::tv_last_error;
pub use ffi::*;
