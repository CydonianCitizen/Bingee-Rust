// Standalone read-only validation helper linked to the existing release
// rusqlite build. It is not part of the application or its dependency graph.
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let connection = rusqlite::Connection::open_with_flags(
        &args[1], rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ).unwrap();
    connection.create_scalar_function("bingee_fold", 1,
        rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        |context| Ok(context.get::<Option<String>>(0)?.unwrap_or_default().to_lowercase()),
    ).unwrap();
    let mut statements: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(&args[2]).unwrap()).unwrap();
    for item in &mut statements {
        let sql = item["sql"].as_str().unwrap();
        let mut statement = connection.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
        let bindings = vec![rusqlite::types::Value::Null; statement.parameter_count()];
        let rows: Vec<_> = statement.query_map(rusqlite::params_from_iter(bindings), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(3)?))
        }).unwrap().map(Result::unwrap).collect();
        item["plan"] = serde_json::to_value(rows).unwrap();
    }
    let output = serde_json::json!({"sqlite_version": rusqlite::version(), "bindings": "NULL; static SELECT/WITH statements plus both Library sort variants", "plans": statements});
    std::fs::write(&args[3], serde_json::to_vec_pretty(&output).unwrap()).unwrap();
}
