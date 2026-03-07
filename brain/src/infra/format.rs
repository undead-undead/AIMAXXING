//! Tool for formatting structured data into compact, token-efficient formats.

/// Formats a list of records into a Markdown table.
pub struct MarkdownTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl MarkdownTable {
    /// Create a new table with headers
    pub fn new(headers: Vec<impl Into<String>>) -> Self {
        Self {
            headers: headers.into_iter().map(|s| s.into()).collect(),
            rows: Vec::new(),
        }
    }

    /// Add a row to the table
    pub fn add_row(&mut self, row: Vec<impl Into<String>>) {
        self.rows.push(row.into_iter().map(|s| s.into()).collect());
    }

    /// Convert to a Markdown string
    pub fn render(&self) -> String {
        if self.headers.is_empty() {
            return String::new();
        }

        let mut output = String::new();

        // Headers
        output.push('|');
        for header in &self.headers {
            output.push(' ');
            output.push_str(header);
            output.push_str(" |");
        }
        output.push('\n');

        // Separator
        output.push('|');
        for _ in &self.headers {
            output.push_str(" --- |");
        }
        output.push('\n');

        // Rows
        for row in &self.rows {
            output.push('|');
            for (i, cell) in row.iter().enumerate() {
                if i >= self.headers.len() {
                    break;
                }
                output.push(' ');
                // Escape pipes in content
                output.push_str(&cell.replace('|', "\\|"));
                output.push_str(" |");
            }
            // Fill missing cells
            for _ in row.len()..self.headers.len() {
                output.push_str("  |");
            }
            output.push('\n');
        }

        output
    }
}
