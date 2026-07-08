use serde::Deserialize;
use serde_json::json;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, FuzzyTermQuery, Occur, Query, RegexQuery, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{TantivyDocument, Term};

use crate::registry::IndexState;

#[derive(Debug, Deserialize)]
struct WhereClause {
    field: String,
    value: String,
    #[serde(default)]
    occur: String, // "must" | "must_not" | "should"
}

#[derive(Debug, Deserialize)]
struct InClause {
    field: String,
    values: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct QueryInput {
    #[serde(default)]
    text: String,
    #[serde(default)]
    text_fields: Vec<String>,
    #[serde(default)]
    r#where: Vec<WhereClause>,
    #[serde(default)]
    r#in: Vec<InClause>,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    min_score: f32,
}

fn default_limit() -> usize {
    20
}

/// distancia de edición aproximada segun el largo del término (parity funcional con similarity 0.6).
fn fuzzy_distance(term_len: usize) -> u8 {
    if term_len < 3 {
        0
    } else if term_len <= 5 {
        1
    } else {
        2
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

fn occur_of(s: &str) -> Occur {
    match s {
        "must_not" => Occur::MustNot,
        "should" => Occur::Should,
        _ => Occur::Must,
    }
}

pub fn search(state: &IndexState, query_json: &str) -> Result<String, String> {
    let input: QueryInput =
        serde_json::from_str(query_json).map_err(|e| format!("query JSON inválido: {e}"))?;

    let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

    // texto: por cada palabra, un OR de [exacto, fuzzy, prefijo] sobre todos los campos de texto; requerido (MUST).
    let words = tokenize(&input.text);
    for word in &words {
        let mut per_word: Vec<(Occur, Box<dyn Query>)> = Vec::new();
        for fname in &input.text_fields {
            let field = match state.schema.get_field(fname) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let term = Term::from_field_text(field, word);
            per_word.push((
                Occur::Should,
                Box::new(TermQuery::new(term.clone(), IndexRecordOption::WithFreqs)),
            ));
            let dist = fuzzy_distance(word.chars().count());
            if dist > 0 {
                per_word.push((Occur::Should, Box::new(FuzzyTermQuery::new(term, dist, true))));
            }
            if word.chars().count() >= 2 {
                if let Ok(rq) =
                    RegexQuery::from_pattern(&format!("{}.*", regex_escape(word)), field)
                {
                    per_word.push((Occur::Should, Box::new(rq)));
                }
            }
        }
        if !per_word.is_empty() {
            clauses.push((Occur::Must, Box::new(BooleanQuery::new(per_word))));
        }
    }

    // where -> term sobre <field>_key con su occur.
    for w in &input.r#where {
        let field = state
            .schema
            .get_field(&w.field)
            .map_err(|_| format!("campo where '{}' no existe", w.field))?;
        let term = Term::from_field_text(field, &w.value);
        clauses.push((
            occur_of(&w.occur),
            Box::new(TermQuery::new(term, IndexRecordOption::Basic)),
        ));
    }

    // in -> grupo SHOULD de terms sobre <field>_key, requerido en el top.
    for clause in &input.r#in {
        let field = state
            .schema
            .get_field(&clause.field)
            .map_err(|_| format!("campo in '{}' no existe", clause.field))?;
        let shoulds: Vec<(Occur, Box<dyn Query>)> = clause
            .values
            .iter()
            .map(|v| {
                let t = Term::from_field_text(field, v);
                (
                    Occur::Should,
                    Box::new(TermQuery::new(t, IndexRecordOption::Basic)) as Box<dyn Query>,
                )
            })
            .collect();
        if !shoulds.is_empty() {
            clauses.push((Occur::Must, Box::new(BooleanQuery::new(shoulds))));
        }
    }

    // sin cláusulas -> resultado vacío (equivale a "sin match").
    if clauses.is_empty() {
        return Ok(json!({"hits": []}).to_string());
    }

    let query = BooleanQuery::new(clauses);
    let searcher = state.reader.searcher();
    let top = searcher
        .search(&query, &TopDocs::with_limit(input.limit))
        .map_err(|e| format!("search falló: {e}"))?;

    let mut hits = Vec::new();
    for (score, addr) in top {
        if score < input.min_score {
            continue;
        }
        let doc: TantivyDocument = searcher
            .doc(addr)
            .map_err(|e| format!("no se pudo leer doc: {e}"))?;
        let mut fields = serde_json::Map::new();
        for (field, field_entry) in state.schema.fields() {
            if !field_entry.is_stored() {
                continue;
            }
            if let Some(v) = doc.get_first(field) {
                if let Some(s) = v.as_str() {
                    fields.insert(field_entry.name().to_string(), json!(s));
                }
            }
        }
        hits.push(json!({"score": score, "fields": fields}));
    }

    Ok(json!({"hits": hits}).to_string())
}

/// escapa metacaracteres regex para usar la palabra como prefijo literal.
fn regex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{close, open_or_create, with_state};
    use crate::schema::{FieldsDescriptor, IndexConfig};
    use crate::writer::{add_document, commit};

    fn cfg(path: &str) -> IndexConfig {
        IndexConfig {
            path: path.to_string(),
            id_field: "id_key".into(),
            fields: FieldsDescriptor {
                text: vec!["title".into()],
                keys: vec!["id_key".into(), "visibility_type_key".into()],
                attributes: vec![],
            },
            writer_heap_bytes: 15_000_000,
        }
    }

    #[test]
    fn finds_fuzzy_and_filters_by_where() {
        let dir = std::env::temp_dir().join(format!("tv_q_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let h = open_or_create(cfg(dir.to_str().unwrap())).unwrap();
        with_state(h, |s| {
            add_document(
                s,
                r#"{"id_key":"1","title":"reset password","visibility_type_key":"public"}"#,
            )
        })
        .unwrap();
        with_state(h, |s| {
            add_document(
                s,
                r#"{"id_key":"2","title":"reset password","visibility_type_key":"private"}"#,
            )
        })
        .unwrap();
        // add ya no commitea (NRT): hay que commitear para que las búsquedas vean los docs.
        with_state(h, commit).unwrap();

        let q = r#"{"text":"pasword","text_fields":["title"],
            "where":[{"field":"visibility_type_key","value":"public","occur":"must"}],
            "limit":10}"#;
        let out = with_state(h, |s| search(s, q)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        let hits = parsed["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["fields"]["id_key"], "1");

        close(h);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
