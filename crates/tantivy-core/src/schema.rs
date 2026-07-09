use serde::Deserialize;
use tantivy::schema::{FieldEntry, FieldType, Schema, STORED, STRING, TEXT};

#[derive(Debug, Deserialize)]
pub struct FieldsDescriptor {
    #[serde(default)]
    pub text: Vec<String>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub attributes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct IndexConfig {
    pub path: String,
    pub id_field: String,
    pub fields: FieldsDescriptor,
    #[serde(default = "default_heap")]
    pub writer_heap_bytes: usize,
}

fn default_heap() -> usize {
    50_000_000
}

/// Construye el Schema de tantivy desde el descriptor:
/// text -> TEXT|STORED (tokenizado + guardado), keys -> STRING|STORED (exacto), attributes -> STORED (solo guardado).
pub fn build_schema(fields: &FieldsDescriptor) -> Schema {
    let mut b = Schema::builder();
    for f in &fields.text {
        b.add_text_field(f, TEXT | STORED);
    }
    for f in &fields.keys {
        b.add_text_field(f, STRING | STORED);
    }
    for f in &fields.attributes {
        b.add_text_field(f, STORED);
    }
    b.build()
}

/// ¿Es `entry` un campo clave de match exacto (bucket `keys` -> STRING)?
///
/// Sólo estos campos pueden usarse como key_field en delete_by_id/update_document: el valor
/// completo se indexa como UN único término, así `delete_term` matchea exactamente el doc de esa
/// clave. Un campo `text` (TEXT, tokenizado con "default") indexa tokens sueltos, no el valor
/// entero, y un `attribute` (STORED, sin indexar) no tiene términos: usar cualquiera de los dos
/// como clave corrompe el índice (duplicados o borrado en masa).
///
/// El discriminante fiable en tantivy 0.25 es el tokenizer de las opciones de indexado: STRING usa
/// "raw" (valor tal cual = un término), TEXT usa "default". Un campo sin opciones de indexado
/// (attribute) no es indexado -> no es clave.
pub fn field_is_exact_key(entry: &FieldEntry) -> bool {
    matches!(
        entry.field_type(),
        FieldType::Str(opts)
            if opts.get_indexing_options().is_some_and(|o| o.tokenizer() == "raw")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> FieldsDescriptor {
        FieldsDescriptor {
            text: vec!["title".into()],
            keys: vec!["id_key".into()],
            attributes: vec!["content_attr".into()],
        }
    }

    #[test]
    fn maps_buckets_to_field_types() {
        let schema = build_schema(&descriptor());
        assert!(schema.get_field("title").is_ok());
        assert!(schema.get_field("id_key").is_ok());
        assert!(schema.get_field("content_attr").is_ok());
    }

    #[test]
    fn text_field_is_tokenized_and_stored() {
        let schema = build_schema(&descriptor());
        let field = schema.get_field("title").unwrap();
        let entry = schema.get_field_entry(field);
        assert!(entry.is_stored());
        assert!(entry.is_indexed());
    }

    #[test]
    fn attribute_field_is_stored_but_not_indexed() {
        let schema = build_schema(&descriptor());
        let field = schema.get_field("content_attr").unwrap();
        let entry = schema.get_field_entry(field);
        assert!(entry.is_stored());
        assert!(!entry.is_indexed());
    }
}
