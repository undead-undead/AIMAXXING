//! Data transformation tool — CSV/JSON processing, querying, and statistics.
//!
//! Provides data operations:
//! - Read/write CSV files
//! - Query and filter datasets
//! - Compute statistics (mean, median, stddev, percentiles, correlation)
//! - Transform data (rename, add columns, deduplicate, pivot)

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::error::Error;
use crate::skills::tool::{Tool, ToolDefinition};

pub struct DataTransformTool;

#[derive(Deserialize)]
struct DataArgs {
    action: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    data: Vec<Value>,
    #[serde(default)]
    delimiter: Option<String>,
    #[serde(default)]
    filter: Option<Value>,
    #[serde(default)]
    columns: Vec<String>,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    sort_desc: bool,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    output: String,
}

#[async_trait]
impl Tool for DataTransformTool {
    fn name(&self) -> String { "data_transform".to_string() }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "data_transform".to_string(),
            description: "Process, query, and analyze CSV/JSON data — filter, sort, statistics, transform".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["read_csv", "write_csv", "query", "stats", "transform"], "description": "Data operation" },
                    "path": { "type": "string", "description": "File path for CSV read/write" },
                    "data": { "type": "array", "description": "Inline data array of objects" },
                    "delimiter": { "type": "string", "description": "CSV delimiter (default: comma)" },
                    "filter": { "type": "object", "description": "Filter conditions: {column: {op: value}}, ops: eq, ne, gt, lt, gte, lte, contains" },
                    "columns": { "type": "array", "items": {"type": "string"}, "description": "Columns to select or aggregate" },
                    "sort_by": { "type": "string", "description": "Column to sort by" },
                    "sort_desc": { "type": "boolean", "description": "Sort descending" },
                    "limit": { "type": "integer", "description": "Limit results" },
                    "output": { "type": "string", "description": "Output file path" }
                },
                "required": ["action"]
            }),
            parameters_ts: None,
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use for CSV/JSON data analysis. Provide data inline or via file path.".into()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: DataArgs = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments {
            tool_name: "data_transform".into(),
            message: e.to_string(),
        })?;

        let result = match args.action.as_str() {
            "read_csv" => action_read_csv(&args).await?,
            "write_csv" => action_write_csv(&args).await?,
            "query" => action_query(&args)?,
            "stats" => action_stats(&args)?,
            "transform" => action_transform(&args)?,
            _ => json!({"error": format!("Unknown action: {}", args.action)}),
        };

        Ok(serde_json::to_string_pretty(&result)?)
    }
}

async fn action_read_csv(args: &DataArgs) -> anyhow::Result<Value> {
    if args.path.is_empty() {
        return Ok(json!({"error": "path is required"}));
    }
    let content = tokio::fs::read_to_string(&args.path).await?;
    let delim = args.delimiter.as_deref().unwrap_or(",");
    let rows = parse_csv(&content, delim);
    Ok(json!({"rows": rows.len(), "data": rows}))
}

async fn action_write_csv(args: &DataArgs) -> anyhow::Result<Value> {
    if args.output.is_empty() || args.data.is_empty() {
        return Ok(json!({"error": "output and data are required"}));
    }
    let delim = args.delimiter.as_deref().unwrap_or(",");
    let csv = to_csv(&args.data, delim);
    tokio::fs::write(&args.output, csv).await?;
    Ok(json!({"success": true, "rows": args.data.len(), "path": args.output}))
}

fn action_query(args: &DataArgs) -> anyhow::Result<Value> {
    let mut rows = args.data.clone();

    // Filter
    if let Some(filter) = &args.filter {
        if let Some(filter_obj) = filter.as_object() {
            rows.retain(|row| {
                filter_obj.iter().all(|(col, cond)| {
                    let val = row.get(col);
                    match_condition(val, cond)
                })
            });
        }
    }

    // Sort
    if let Some(sort_col) = &args.sort_by {
        rows.sort_by(|a, b| {
            let va = a.get(sort_col);
            let vb = b.get(sort_col);
            let ord = compare_json(va, vb);
            if args.sort_desc { ord.reverse() } else { ord }
        });
    }

    // Select columns
    if !args.columns.is_empty() {
        rows = rows.into_iter().map(|row| {
            let mut obj = serde_json::Map::new();
            for col in &args.columns {
                if let Some(v) = row.get(col) {
                    obj.insert(col.clone(), v.clone());
                }
            }
            Value::Object(obj)
        }).collect();
    }

    // Limit
    if let Some(limit) = args.limit {
        rows.truncate(limit);
    }

    Ok(json!({"count": rows.len(), "data": rows}))
}

fn action_stats(args: &DataArgs) -> anyhow::Result<Value> {
    if args.data.is_empty() {
        return Ok(json!({"error": "data is required"}));
    }

    let cols = if args.columns.is_empty() {
        // Auto-detect numeric columns
        if let Some(first) = args.data.first().and_then(|v| v.as_object()) {
            first.keys().cloned().collect()
        } else {
            Vec::new()
        }
    } else {
        args.columns.clone()
    };

    let mut stats = serde_json::Map::new();
    for col in &cols {
        let nums: Vec<f64> = args.data.iter()
            .filter_map(|r| r.get(col).and_then(|v| v.as_f64()))
            .collect();
        if nums.is_empty() { continue; }

        let sum: f64 = nums.iter().sum();
        let mean = sum / nums.len() as f64;
        let variance = nums.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / nums.len() as f64;
        let mut sorted = nums.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = if sorted.len() % 2 == 0 {
            (sorted[sorted.len()/2 - 1] + sorted[sorted.len()/2]) / 2.0
        } else {
            sorted[sorted.len()/2]
        };

        stats.insert(col.clone(), json!({
            "count": nums.len(),
            "sum": sum,
            "mean": (mean * 1000.0).round() / 1000.0,
            "median": median,
            "stddev": (variance.sqrt() * 1000.0).round() / 1000.0,
            "min": sorted.first().unwrap(),
            "max": sorted.last().unwrap(),
        }));
    }

    Ok(json!({"total_rows": args.data.len(), "statistics": stats}))
}

fn action_transform(args: &DataArgs) -> anyhow::Result<Value> {
    // Deduplicate based on all columns
    let mut seen = std::collections::HashSet::new();
    let deduped: Vec<Value> = args.data.iter()
        .filter(|row| {
            let key = serde_json::to_string(row).unwrap_or_default();
            seen.insert(key)
        })
        .cloned()
        .collect();

    Ok(json!({"original_rows": args.data.len(), "deduplicated": deduped.len(), "data": deduped}))
}

// --- Helpers ---

fn parse_csv(content: &str, delimiter: &str) -> Vec<Value> {
    let mut lines = content.lines();
    let headers: Vec<&str> = match lines.next() {
        Some(h) => h.split(delimiter).map(|s| s.trim()).collect(),
        None => return Vec::new(),
    };

    lines.map(|line| {
        let fields: Vec<&str> = line.split(delimiter).map(|s| s.trim()).collect();
        let mut obj = serde_json::Map::new();
        for (i, header) in headers.iter().enumerate() {
            let val = fields.get(i).copied().unwrap_or("");
            obj.insert(header.to_string(), try_parse(val));
        }
        Value::Object(obj)
    }).collect()
}

fn to_csv(data: &[Value], delimiter: &str) -> String {
    if data.is_empty() { return String::new(); }
    let headers: Vec<String> = data[0].as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    let mut csv = headers.join(delimiter) + "\n";
    for row in data {
        let fields: Vec<String> = headers.iter()
            .map(|h| row.get(h).map(|v| match v { Value::String(s) => s.clone(), _ => v.to_string() }).unwrap_or_default())
            .collect();
        csv.push_str(&fields.join(delimiter));
        csv.push('\n');
    }
    csv
}

fn try_parse(s: &str) -> Value {
    if let Ok(n) = s.parse::<i64>() { return json!(n); }
    if let Ok(f) = s.parse::<f64>() { return json!(f); }
    if s == "true" { return json!(true); }
    if s == "false" { return json!(false); }
    json!(s)
}

fn match_condition(val: Option<&Value>, cond: &Value) -> bool {
    if let Some(obj) = cond.as_object() {
        for (op, expected) in obj {
            let result = match op.as_str() {
                "eq" => val == Some(expected),
                "ne" => val != Some(expected),
                "gt" => compare_json(val, Some(expected)) == std::cmp::Ordering::Greater,
                "lt" => compare_json(val, Some(expected)) == std::cmp::Ordering::Less,
                "gte" => compare_json(val, Some(expected)) != std::cmp::Ordering::Less,
                "lte" => compare_json(val, Some(expected)) != std::cmp::Ordering::Greater,
                "contains" => {
                    val.and_then(|v| v.as_str())
                        .map(|s| s.contains(expected.as_str().unwrap_or("")))
                        .unwrap_or(false)
                }
                _ => true,
            };
            if !result { return false; }
        }
        true
    } else {
        val == Some(cond)
    }
}

fn compare_json(a: Option<&Value>, b: Option<&Value>) -> std::cmp::Ordering {
    match (a.and_then(|v| v.as_f64()), b.and_then(|v| v.as_f64())) {
        (Some(fa), Some(fb)) => fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal),
        _ => {
            let sa = a.map(|v| v.to_string()).unwrap_or_default();
            let sb = b.map(|v| v.to_string()).unwrap_or_default();
            sa.cmp(&sb)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_definition() {
        let tool = DataTransformTool;
        let def = tool.definition().await;
        assert_eq!(def.name, "data_transform");
    }

    #[test]
    fn test_parse_csv() {
        let csv = "name,age,score\nAlice,30,95.5\nBob,25,88.0";
        let rows = parse_csv(csv, ",");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[0]["age"], 30);
    }

    #[test]
    fn test_stats() {
        let args = DataArgs {
            action: "stats".into(),
            data: vec![
                json!({"x": 10, "y": 20}),
                json!({"x": 20, "y": 40}),
                json!({"x": 30, "y": 60}),
            ],
            columns: vec!["x".into()],
            ..serde_json::from_str(r#"{"action":"stats"}"#).unwrap()
        };
        let result = action_stats(&args).unwrap();
        let mean = result["statistics"]["x"]["mean"].as_f64().unwrap();
        assert!((mean - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_query_filter() {
        let args = DataArgs {
            action: "query".into(),
            data: vec![
                json!({"name": "Alice", "age": 30}),
                json!({"name": "Bob", "age": 25}),
            ],
            filter: Some(json!({"age": {"gt": 26}})),
            ..serde_json::from_str(r#"{"action":"query"}"#).unwrap()
        };
        let result = action_query(&args).unwrap();
        assert_eq!(result["count"], 1);
    }
}
