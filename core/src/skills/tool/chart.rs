//! Data visualization tool — chart generation via Python backends.
//!
//! Generates matplotlib/plotly scripts and executes them to produce
//! PNG/SVG images or interactive HTML charts.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

use crate::error::Error;
use crate::skills::tool::{Tool, ToolDefinition};
use crate::skills::runtime::python_utils;

pub struct ChartTool;

#[derive(Deserialize)]
struct ChartArgs {
    action: String,
    #[serde(default = "default_chart_type")]
    chart_type: String,
    #[serde(default)]
    data: serde_json::Value,
    #[serde(default)]
    title: String,
    #[serde(default)]
    x_label: String,
    #[serde(default)]
    y_label: String,
    #[serde(default)]
    output: String,
    #[serde(default = "default_backend")]
    backend: String,
}

fn default_chart_type() -> String { "bar".into() }
fn default_backend() -> String { "matplotlib".into() }

#[async_trait]
impl Tool for ChartTool {
    fn name(&self) -> String { "chart".to_string() }

    async fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "chart".to_string(),
            description: "Generate charts and data visualizations (bar, line, pie, scatter, histogram, heatmap)".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["generate", "info"], "description": "Action to perform" },
                    "chart_type": { "type": "string", "enum": ["bar", "line", "pie", "scatter", "histogram", "heatmap"], "description": "Chart type" },
                    "data": { "type": "object", "description": "Chart data: {labels: [...], values: [...]} or {x: [...], y: [...]}" },
                    "title": { "type": "string", "description": "Chart title" },
                    "x_label": { "type": "string", "description": "X-axis label" },
                    "y_label": { "type": "string", "description": "Y-axis label" },
                    "output": { "type": "string", "description": "Output file path (e.g., chart.png, chart.html)" },
                    "backend": { "type": "string", "enum": ["matplotlib", "plotly"], "description": "Rendering backend" }
                },
                "required": ["action"]
            }),
            parameters_ts: None,
            is_binary: false,
            is_verified: true,
            usage_guidelines: Some("Use to create visual charts. Requires Python 3 + matplotlib or plotly.".into()),
        }
    }

    async fn call(&self, arguments: &str) -> anyhow::Result<String> {
        let args: ChartArgs = serde_json::from_str(arguments).map_err(|e| Error::ToolArguments {
            tool_name: "chart".into(),
            message: e.to_string(),
        })?;

        let result = match args.action.as_str() {
            "info" => check_backends().await?,
            "generate" => generate_chart(&args).await?,
            _ => json!({"error": format!("Unknown action: {}", args.action)}),
        };

        Ok(serde_json::to_string_pretty(&result)?)
    }
}

async fn check_backends() -> anyhow::Result<serde_json::Value> {
    let python_bin = python_utils::find_python().await;
    let has_python = python_bin.is_some();
    
    // We don't check modules here as we will install them on-demand in a venv
    Ok(json!({
        "python_available": has_python,
        "managed_python": python_bin.map(|p| p.to_string_lossy().contains(".aimaxxing")).unwrap_or(false),
        "note": "Dependencies (matplotlib/plotly) are installed automatically in an isolated venv on first run."
    }))
}

async fn generate_chart(args: &ChartArgs) -> anyhow::Result<serde_json::Value> {
    let output_path = if args.output.is_empty() {
        let ext = if args.backend == "plotly" { "html" } else { "png" };
        format!("/tmp/aimaxxing_chart_{}.{}", chrono::Utc::now().timestamp(), ext)
    } else {
        args.output.clone()
    };

    // 1. Resolve Python and deps
    let base_python = match python_utils::find_python().await {
        Some(p) => p,
        None => python_utils::provision_python_via_uv().await?,
    };

    let deps = if args.backend == "plotly" {
        vec!["plotly".to_string(), "pandas".to_string()]
    } else {
        vec!["matplotlib".to_string()]
    };

    let python_bin = python_utils::ensure_venv(&base_python, "chart_tool", &deps).await?;

    let script = if args.backend == "plotly" {
        build_plotly_script(args, &output_path)?
    } else {
        build_matplotlib_script(args, &output_path)?
    };

    // Write script to temp file
    let script_path = format!("/tmp/aimaxxing_chart_{}.py", chrono::Utc::now().timestamp_millis());
    tokio::fs::write(&script_path, &script).await?;

    let output = tokio::process::Command::new(python_bin)
        .arg(&script_path)
        .output()
        .await?;

    let _ = tokio::fs::remove_file(&script_path).await;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Ok(json!({"error": format!("Python execution failed: {}", stderr)}));
    }

    Ok(json!({
        "success": true,
        "output_path": output_path,
        "chart_type": args.chart_type,
        "backend": args.backend,
    }))
}

fn quote_py(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn build_matplotlib_script(args: &ChartArgs, output_path: &str) -> anyhow::Result<String> {
    let labels = args.data.get("labels").and_then(|v| v.as_array());
    let values = args.data.get("values").and_then(|v| v.as_array());
    let x = args.data.get("x").and_then(|v| v.as_array());
    let y = args.data.get("y").and_then(|v| v.as_array());

    let mut script = String::from("import matplotlib\nmatplotlib.use('Agg')\nimport matplotlib.pyplot as plt\n\n");

    match args.chart_type.as_str() {
        "bar" => {
            if let (Some(l), Some(v)) = (labels, values) {
                script.push_str(&format!("plt.bar({:?}, {:?})\n", l, v));
            }
        }
        "line" => {
            let xd = x.or(labels);
            let yd = y.or(values);
            if let (Some(xv), Some(yv)) = (xd, yd) {
                script.push_str(&format!("plt.plot({:?}, {:?})\n", xv, yv));
            }
        }
        "pie" => {
            if let (Some(l), Some(v)) = (labels, values) {
                script.push_str(&format!("plt.pie({:?}, labels={:?}, autopct='%1.1f%%')\n", v, l));
            }
        }
        "scatter" => {
            if let (Some(xv), Some(yv)) = (x, y) {
                script.push_str(&format!("plt.scatter({:?}, {:?})\n", xv, yv));
            }
        }
        "histogram" => {
            if let Some(v) = values.or(y) {
                script.push_str(&format!("plt.hist({:?}, bins=20)\n", v));
            }
        }
        _ => {}
    }

    if !args.title.is_empty() {
        script.push_str(&format!("plt.title('{}')\n", quote_py(&args.title)));
    }
    if !args.x_label.is_empty() {
        script.push_str(&format!("plt.xlabel('{}')\n", quote_py(&args.x_label)));
    }
    if !args.y_label.is_empty() {
        script.push_str(&format!("plt.ylabel('{}')\n", quote_py(&args.y_label)));
    }
    script.push_str("plt.tight_layout()\n");
    script.push_str(&format!("plt.savefig('{}', dpi=150)\n", quote_py(output_path)));
    script.push_str("plt.close()\n");

    Ok(script)
}

fn build_plotly_script(args: &ChartArgs, output_path: &str) -> anyhow::Result<String> {
    let labels = args.data.get("labels").and_then(|v| v.as_array());
    let values = args.data.get("values").and_then(|v| v.as_array());
    let x = args.data.get("x").and_then(|v| v.as_array());
    let y = args.data.get("y").and_then(|v| v.as_array());

    let mut script = String::from("import plotly.graph_objects as go\n\n");

    match args.chart_type.as_str() {
        "bar" => {
            if let (Some(l), Some(v)) = (labels, values) {
                script.push_str(&format!("fig = go.Figure(go.Bar(x={:?}, y={:?}))\n", l, v));
            }
        }
        "line" => {
            let xd = x.or(labels);
            let yd = y.or(values);
            if let (Some(xv), Some(yv)) = (xd, yd) {
                script.push_str(&format!("fig = go.Figure(go.Scatter(x={:?}, y={:?}, mode='lines'))\n", xv, yv));
            }
        }
        "pie" => {
            if let (Some(l), Some(v)) = (labels, values) {
                script.push_str(&format!("fig = go.Figure(go.Pie(labels={:?}, values={:?}))\n", l, v));
            }
        }
        _ => {
            script.push_str("fig = go.Figure()\n");
        }
    }

    if !args.title.is_empty() {
        script.push_str(&format!("fig.update_layout(title='{}')\n", quote_py(&args.title)));
    }
    script.push_str(&format!("fig.write_html('{}')\n", quote_py(output_path)));

    Ok(script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_definition() {
        let tool = ChartTool;
        let def = tool.definition().await;
        assert_eq!(def.name, "chart");
    }

    #[test]
    fn test_matplotlib_script_generation() {
        let args = ChartArgs {
            action: "generate".into(),
            chart_type: "bar".into(),
            data: json!({"labels": ["A", "B", "C"], "values": [10, 20, 30]}),
            title: "Test Chart".into(),
            x_label: "Category".into(),
            y_label: "Value".into(),
            output: "/tmp/test.png".into(),
            backend: "matplotlib".into(),
        };
        let script = build_matplotlib_script(&args, "/tmp/test.png").unwrap();
        assert!(script.contains("plt.bar"));
        assert!(script.contains("Test Chart"));
    }
}
