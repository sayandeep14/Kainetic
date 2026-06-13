//! [`SqlQueryTool`] — read-only SQL queries against a `SQLite` database.
//!
//! Only `SELECT` statements are permitted; any attempt to use `INSERT`,
//! `UPDATE`, `DELETE`, `DROP`, `CREATE`, or other write/DDL operations is
//! rejected by the SQL parser before the database is touched.

use kainetic_schema::RootSchema;
use rusqlite::{params_from_iter, types::Value as SqlValue};
use schemars::schema_for;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sqlparser::{ast::Statement, dialect::GenericDialect, parser::Parser};
use tracing::debug;

use crate::{Tool, ToolContext, ToolError, ToolFuture};

/// Executes a read-only SQL `SELECT` query against a `SQLite` database file.
///
/// The query is validated via `sqlparser` before execution — only `SELECT`
/// statements are allowed, preventing any accidental data mutation.
pub struct SqlQueryTool;

#[derive(Deserialize, JsonSchema)]
struct Input {
    /// Path to the `SQLite` database file.
    database_path: String,
    /// A SELECT SQL statement.
    query: String,
    /// Optional positional bind parameters (`?` placeholders).
    #[serde(default)]
    params: Vec<serde_json::Value>,
}

#[derive(Serialize, JsonSchema)]
struct Output {
    column_names: Vec<String>,
    rows: Vec<Vec<serde_json::Value>>,
    row_count: usize,
}

/// Validates that `sql` is a single `SELECT` statement.
fn validate_select_only(sql: &str) -> Result<(), ToolError> {
    let dialect = GenericDialect {};
    let statements = Parser::parse_sql(&dialect, sql)
        .map_err(|e| ToolError::InputValidation(format!("SQL parse error: {e}")))?;

    if statements.len() != 1 {
        return Err(ToolError::InputValidation(
            "exactly one SQL statement is required".into(),
        ));
    }

    match &statements[0] {
        Statement::Query(_) => Ok(()),
        other => Err(ToolError::InputValidation(format!(
            "only SELECT queries are allowed; got: {other}"
        ))),
    }
}

fn json_to_sql(v: &serde_json::Value) -> SqlValue {
    match v {
        serde_json::Value::Null => SqlValue::Null,
        serde_json::Value::Bool(b) => SqlValue::Integer(i64::from(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                SqlValue::Real(f)
            } else {
                SqlValue::Text(n.to_string())
            }
        }
        serde_json::Value::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}

impl Tool for SqlQueryTool {
    fn name(&self) -> &'static str {
        "sql_query"
    }

    fn description(&self) -> &'static str {
        "Execute a read-only SELECT query against a SQLite database file and return the results."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(Input)
    }

    fn output_schema(&self) -> RootSchema {
        schema_for!(Output)
    }

    fn call(&self, input: serde_json::Value, _ctx: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            let params: Input = serde_json::from_value(input)
                .map_err(|e| ToolError::InputValidation(e.to_string()))?;

            validate_select_only(&params.query)?;
            debug!(db = %params.database_path, query = %params.query, "sql_query");

            let db_path = params.database_path.clone();
            let query = params.query.clone();
            let bind_params: Vec<SqlValue> = params.params.iter().map(json_to_sql).collect();

            // rusqlite is synchronous; run in blocking thread pool.
            let result = tokio::task::spawn_blocking(move || -> Result<Output, String> {
                let conn = rusqlite::Connection::open_with_flags(
                    &db_path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                )
                .map_err(|e| format!("cannot open database: {e}"))?;

                let mut stmt = conn
                    .prepare(&query)
                    .map_err(|e| format!("prepare failed: {e}"))?;

                let column_names: Vec<String> = stmt
                    .column_names()
                    .into_iter()
                    .map(str::to_owned)
                    .collect();

                let mut rows = Vec::new();
                let mut result_rows = stmt
                    .query(params_from_iter(bind_params.iter()))
                    .map_err(|e| format!("query failed: {e}"))?;

                while let Some(row) = result_rows
                    .next()
                    .map_err(|e| format!("row fetch failed: {e}"))?
                {
                    let values: Vec<serde_json::Value> = (0..column_names.len())
                        .map(|i| {
                            match row.get_ref(i).map_err(|e| e.to_string())? {
                                rusqlite::types::ValueRef::Null => Ok(serde_json::Value::Null),
                                rusqlite::types::ValueRef::Integer(n) => {
                                    Ok(serde_json::Value::Number(n.into()))
                                }
                                rusqlite::types::ValueRef::Real(f) => {
                                    Ok(serde_json::Value::Number(
                                        serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into()),
                                    ))
                                }
                                rusqlite::types::ValueRef::Text(t) => Ok(serde_json::Value::String(
                                    String::from_utf8_lossy(t).into_owned(),
                                )),
                                rusqlite::types::ValueRef::Blob(b) => Ok(serde_json::Value::String(
                                    format!("<blob {} bytes>", b.len()),
                                )),
                            }
                        })
                        .collect::<Result<_, String>>()?;
                    rows.push(values);
                }

                let row_count = rows.len();
                Ok(Output {
                    column_names,
                    rows,
                    row_count,
                })
            })
            .await
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?
            .map_err(ToolError::ExecutionFailed)?;

            serde_json::to_value(result).map_err(|e| ToolError::ExecutionFailed(e.to_string()))
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kainetic_schema::RunId;
    use tokio_util::sync::CancellationToken;

    fn ctx() -> ToolContext {
        ToolContext::new(RunId::new(), CancellationToken::new())
    }

    fn temp_db() -> String {
        let path = std::env::temp_dir().join(format!(
            "kainetic_sql_test_{}.sqlite",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
             INSERT INTO users VALUES (1, 'Alice', 30);
             INSERT INTO users VALUES (2, 'Bob', 25);",
        )
        .unwrap();
        path.display().to_string()
    }

    #[tokio::test]
    async fn select_returns_rows() {
        let db = temp_db();
        let tool = SqlQueryTool;
        let result = tool
            .call(
                serde_json::json!({
                    "database_path": db,
                    "query": "SELECT name, age FROM users ORDER BY id"
                }),
                ctx(),
            )
            .await
            .unwrap();

        assert_eq!(result["row_count"], 2);
        assert_eq!(result["column_names"][0], "name");
        assert_eq!(result["rows"][0][0], "Alice");
    }

    #[tokio::test]
    async fn rejects_insert() {
        let db = temp_db();
        let tool = SqlQueryTool;
        let err = tool
            .call(
                serde_json::json!({
                    "database_path": db,
                    "query": "INSERT INTO users VALUES (3, 'Charlie', 22)"
                }),
                ctx(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::InputValidation(_)));
    }

    #[tokio::test]
    async fn rejects_drop() {
        let db = temp_db();
        let err = SqlQueryTool
            .call(
                serde_json::json!({
                    "database_path": db,
                    "query": "DROP TABLE users"
                }),
                ctx(),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InputValidation(_)));
    }

    #[test]
    fn validate_select_ok() {
        assert!(validate_select_only("SELECT 1").is_ok());
        assert!(validate_select_only("SELECT * FROM t WHERE id = 1").is_ok());
    }

    #[test]
    fn validate_write_err() {
        assert!(validate_select_only("DELETE FROM t").is_err());
        assert!(validate_select_only("UPDATE t SET x = 1").is_err());
        assert!(validate_select_only("CREATE TABLE t (id INT)").is_err());
    }
}
