use std::ffi::{CStr, CString};

use tantivyphp::*;

fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

#[test]
fn full_roundtrip_through_c_abi() {
    let dir = std::env::temp_dir().join(format!("tv_ffi_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cfg = format!(
        r#"{{"path":"{}","id_field":"id_key",
            "fields":{{"text":["title"],"keys":["id_key"],"attributes":[]}},
            "writer_heap_bytes":15000000}}"#,
        dir.to_str().unwrap()
    );

    let h = tv_index_open_or_create(c(&cfg).as_ptr());
    assert!(h != 0);

    assert_eq!(tv_add_document(h, c(r#"{"id_key":"1","title":"reset password"}"#).as_ptr()), 0);
    // NRT: el add no commitea, así que todavía no es visible. Recién el commit explícito lo publica.
    assert_eq!(tv_doc_count(h), 0);
    assert_eq!(tv_commit(h), 0);
    assert_eq!(tv_doc_count(h), 1);

    let res_ptr = tv_search(h, c(r#"{"text":"reset","text_fields":["title"],"limit":5}"#).as_ptr());
    assert!(!res_ptr.is_null());
    let res = unsafe { CStr::from_ptr(res_ptr) }.to_str().unwrap().to_owned();
    tv_string_free(res_ptr);
    assert!(res.contains("\"id_key\":\"1\""));

    assert_eq!(tv_delete_document(h, c("id_key").as_ptr(), c("1").as_ptr()), 0);
    assert_eq!(tv_commit(h), 0);
    assert_eq!(tv_doc_count(h), 0);

    assert_eq!(tv_index_close(h), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
