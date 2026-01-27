// * Milestone 4 - Task 4.1: Table Extraction [EDD-5.1]
// * Heuristic-based table scoring to distinguish data tables from layout tables.
// * Ported from crawl4ai/table_extraction.py

use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

// * Precompiled CSS selectors for performance
static SELECTOR_TABLE: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("table").expect("Invalid table selector"));
static SELECTOR_THEAD: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("thead").expect("Invalid thead selector"));
static SELECTOR_TBODY: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("tbody").expect("Invalid tbody selector"));
static SELECTOR_TH: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("th").expect("Invalid th selector"));
static SELECTOR_TR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("tr").expect("Invalid tr selector"));
static SELECTOR_TD: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("td").expect("Invalid td selector"));

// * Threshold for determining if a table is a data table (from config)
// * Lowered from 7 to 4 to be more permissive (thead=2 + tbody=1 + th=2 = 5)
const DATA_TABLE_THRESHOLD: i32 = 4;

/// Represents an extracted data table with headers and rows
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub score: i32,
}

/// Scores and extracts data tables from HTML content
pub struct TableScorer;

impl TableScorer {
    /// Calculates heuristic score for a table element
    /// Scoring system ported from crawl4ai:
    /// - +2 for thead presence
    /// - +1 for tbody presence
    /// - +2 for th elements (header cells)
    /// - -3 for nested tables (layout indicator)
    /// - -3 for role="presentation" or role="none"
    /// - +3 if text/tag ratio > 20
    /// - +2 if text/tag ratio > 10
    pub fn calculate_score(table: &ElementRef) -> i32 {
        let mut score: i32 = 0;

        // * Structure checks (positive signals for data tables)
        if table.select(&SELECTOR_THEAD).next().is_some() {
            score += 2;
        }

        if table.select(&SELECTOR_TBODY).next().is_some() {
            score += 1;
        }

        // * Header cells indicate structured data
        let th_count = table.select(&SELECTOR_TH).count();
        if th_count > 0 {
            score += 2;
        }

        // * Nested tables are typically for layout (negative signal)
        let nested_table_count = table
            .select(&SELECTOR_TABLE)
            .skip(1) // ? Skip self - only count nested
            .count();
        if nested_table_count > 0 {
            score -= 3;
        }

        // * Role attribute check (presentation tables are layout)
        if let Some(role) = table.value().attr("role") {
            let role_lower = role.to_lowercase();
            if role_lower == "presentation" || role_lower == "none" {
                score -= 3;
            }
        }

        // * Text density check (high text/tag ratio = likely data)
        let text_content: String = table.text().collect();
        let text_len = text_content.trim().len();
        let tag_count = table.descendants().count().max(1);
        let ratio = text_len as f32 / tag_count as f32;

        if ratio > 20.0 {
            score += 3;
        } else if ratio > 10.0 {
            score += 2;
        }

        score
    }

    /// Determines if a table is a data table based on heuristic score
    pub fn is_data_table(table: &ElementRef) -> bool {
        Self::calculate_score(table) >= DATA_TABLE_THRESHOLD
    }

    /// Extracts headers from a table element
    fn extract_headers(table: &ElementRef) -> Vec<String> {
        let mut headers = Vec::new();

        // * Try thead > tr > th first
        if let Some(thead) = table.select(&SELECTOR_THEAD).next() {
            for th in thead.select(&SELECTOR_TH) {
                let text: String = th.text().collect();
                headers.push(text.trim().to_string());
            }
        }

        // * Fallback: first row th elements
        if headers.is_empty() {
            if let Some(first_row) = table.select(&SELECTOR_TR).next() {
                for th in first_row.select(&SELECTOR_TH) {
                    let text: String = th.text().collect();
                    headers.push(text.trim().to_string());
                }
            }
        }

        // * Fallback: first row td elements if no th found
        if headers.is_empty() {
            if let Some(first_row) = table.select(&SELECTOR_TR).next() {
                for td in first_row.select(&SELECTOR_TD) {
                    let text: String = td.text().collect();
                    headers.push(text.trim().to_string());
                }
            }
        }

        headers
    }

    /// Extracts data rows from a table element
    fn extract_rows(table: &ElementRef, skip_first: bool) -> Vec<Vec<String>> {
        let mut rows = Vec::new();
        let row_iter = table.select(&SELECTOR_TR);

        // * Skip header row if headers were extracted from first row
        let rows_to_process: Vec<ElementRef> = if skip_first {
            row_iter.skip(1).collect()
        } else {
            // * If we have thead, skip rows inside thead
            let tbody_selector = &SELECTOR_TBODY;
            if let Some(tbody) = table.select(tbody_selector).next() {
                tbody.select(&SELECTOR_TR).collect()
            } else {
                row_iter.skip(1).collect()
            }
        };

        for row in rows_to_process {
            let cells: Vec<String> = row
                .select(&SELECTOR_TD)
                .map(|td| {
                    let text: String = td.text().collect();
                    text.trim().to_string()
                })
                .collect();

            // * Only add non-empty rows
            if !cells.is_empty() && cells.iter().any(|c| !c.is_empty()) {
                rows.push(cells);
            }
        }

        rows
    }

    /// Extracts a single table element into structured form
    pub fn extract_table(table: &ElementRef) -> ExtractedTable {
        let score = Self::calculate_score(table);
        let headers = Self::extract_headers(table);

        // * Determine if we need to skip first row (when headers came from first tr)
        let has_thead = table.select(&SELECTOR_THEAD).next().is_some();
        let skip_first = !has_thead && !headers.is_empty();

        let rows = Self::extract_rows(table, skip_first);

        ExtractedTable {
            headers,
            rows,
            score,
        }
    }

    /// Extracts all data tables from HTML content
    /// Only returns tables that meet the data table threshold
    pub fn extract_all_tables(html: &str) -> Vec<ExtractedTable> {
        let document = Html::parse_document(html);
        let mut tables = Vec::new();

        for table in document.select(&SELECTOR_TABLE) {
            if Self::is_data_table(&table) {
                tables.push(Self::extract_table(&table));
            }
        }

        tables
    }

    /// Extracts all tables regardless of score (for analysis)
    pub fn extract_all_tables_unfiltered(html: &str) -> Vec<ExtractedTable> {
        let document = Html::parse_document(html);
        document
            .select(&SELECTOR_TABLE)
            .map(|table| Self::extract_table(&table))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_table_with_thead() {
        let html = r#"
            <table>
                <thead>
                    <tr><th>Name</th><th>Age</th><th>City</th></tr>
                </thead>
                <tbody>
                    <tr><td>Alice</td><td>30</td><td>NYC</td></tr>
                    <tr><td>Bob</td><td>25</td><td>LA</td></tr>
                </tbody>
            </table>
        "#;

        let tables = TableScorer::extract_all_tables(html);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].headers, vec!["Name", "Age", "City"]);
        assert_eq!(tables[0].rows.len(), 2);
        assert!(tables[0].score >= DATA_TABLE_THRESHOLD);
    }

    #[test]
    fn test_layout_table_rejected() {
        let html = r#"
            <table role="presentation">
                <tr><td><img src="logo.png"/></td></tr>
                <tr><td><nav>Menu</nav></td></tr>
            </table>
        "#;

        let tables = TableScorer::extract_all_tables(html);
        assert!(tables.is_empty(), "Layout table should be rejected");
    }

    #[test]
    fn test_nested_table_penalty() {
        let html = r#"
            <table>
                <tr>
                    <td>
                        <table><tr><td>Nested</td></tr></table>
                    </td>
                </tr>
            </table>
        "#;

        let document = Html::parse_document(html);
        let outer_table = document.select(&SELECTOR_TABLE).next().unwrap();
        let score = TableScorer::calculate_score(&outer_table);

        // * Score should be penalized for nesting
        assert!(score < DATA_TABLE_THRESHOLD);
    }

    #[test]
    fn test_high_text_density_bonus() {
        let html = r#"
            <table>
                <thead><tr><th>Description</th><th>Details</th></tr></thead>
                <tbody>
                    <tr>
                        <td>This is a very long description with lots of meaningful text content that increases the text to tag ratio significantly.</td>
                        <td>More detailed information here to boost the text density metric even further.</td>
                    </tr>
                </tbody>
            </table>
        "#;

        let tables = TableScorer::extract_all_tables(html);
        assert!(!tables.is_empty(), "High density table should pass");
    }

    #[test]
    fn test_extract_table_structure() {
        let html = r#"
            <table>
                <thead>
                    <tr><th>Product</th><th>Price</th><th>Stock</th></tr>
                </thead>
                <tbody>
                    <tr><td>Widget A</td><td>$10.00</td><td>100</td></tr>
                    <tr><td>Widget B</td><td>$15.00</td><td>50</td></tr>
                    <tr><td>Widget C</td><td>$20.00</td><td>25</td></tr>
                </tbody>
            </table>
        "#;

        let tables = TableScorer::extract_all_tables(html);
        assert_eq!(tables.len(), 1);

        let table = &tables[0];
        assert_eq!(table.headers, vec!["Product", "Price", "Stock"]);
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[0], vec!["Widget A", "$10.00", "100"]);
    }

    #[test]
    fn test_table_without_thead() {
        let html = r#"
            <table>
                <tr><th>Column A</th><th>Column B</th></tr>
                <tr><td>Value 1</td><td>Value 2</td></tr>
                <tr><td>Value 3</td><td>Value 4</td></tr>
            </table>
        "#;

        let tables = TableScorer::extract_all_tables_unfiltered(html);
        assert!(!tables.is_empty());

        let table = &tables[0];
        assert_eq!(table.headers, vec!["Column A", "Column B"]);
        assert_eq!(table.rows.len(), 2);
    }
}
