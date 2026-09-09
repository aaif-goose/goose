use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use umya_spreadsheet::{structs::Workbook, Worksheet};

const MAX_EXCEL_ROWS: u32 = 1_048_576;
const MAX_EXCEL_COLUMNS: u32 = 16_384;
const MAX_RANGE_CELLS: u64 = 100_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct WorksheetInfo {
    name: String,
    index: usize,
    column_count: usize,
    row_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CellValue {
    value: String,
    formula: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RangeData {
    start_row: u32,
    end_row: u32,
    start_col: u32,
    end_col: u32,
    // First dimension is rows, second dimension is columns: values[row_index][column_index]
    values: Vec<Vec<CellValue>>,
}

pub struct XlsxTool {
    workbook: Workbook,
}

impl XlsxTool {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let workbook =
            umya_spreadsheet::reader::xlsx::read(path).context("Failed to read Excel file")?;
        Ok(Self { workbook })
    }

    pub fn list_worksheets(&self) -> Result<Vec<WorksheetInfo>> {
        let mut worksheets = Vec::new();
        for (index, worksheet) in self.workbook.sheet_collection().iter().enumerate() {
            let (column_count, row_count) = self.get_worksheet_dimensions(worksheet)?;
            worksheets.push(WorksheetInfo {
                name: worksheet.name().to_string(),
                index,
                column_count,
                row_count,
            });
        }
        Ok(worksheets)
    }

    pub fn get_worksheet_by_name(&self, name: &str) -> Result<&Worksheet> {
        self.workbook
            .sheet_by_name(name)
            .context("Worksheet not found")
    }

    pub fn get_worksheet_by_index(&self, index: usize) -> Result<&Worksheet> {
        self.workbook
            .sheet_collection()
            .get(index)
            .context("Worksheet index out of bounds")
    }

    fn get_worksheet_dimensions(&self, worksheet: &Worksheet) -> Result<(usize, usize)> {
        // Returns (column_count, row_count) for the worksheet
        let mut max_col = 0;
        let mut max_row = 0;

        // Iterate through all rows
        for row_num in 1..=worksheet.highest_row() {
            for col_num in 1..=worksheet.highest_column() {
                if let Some(cell) = worksheet.cell((col_num, row_num)) {
                    let coord = cell.coordinate();
                    max_col = max_col.max(coord.col_num() as usize);
                    max_row = max_row.max(coord.row_num() as usize);
                }
            }
        }

        Ok((max_col, max_row))
    }

    pub fn get_column_names(&self, worksheet: &Worksheet) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for col_num in 1..=worksheet.highest_column() {
            if let Some(cell) = worksheet.cell((col_num, 1)) {
                names.push(cell.value().into_owned());
            } else {
                names.push(String::new());
            }
        }
        Ok(names)
    }

    pub fn get_range(&self, worksheet: &Worksheet, range: &str) -> Result<RangeData> {
        let (start_row, start_col, end_row, end_col) = parse_range(range)?;
        let (row_count, column_count) =
            validate_range_bounds(start_row, start_col, end_row, end_col)?;
        let mut values = Vec::with_capacity(row_count as usize);

        // Iterate through rows first, then columns
        for row_idx in start_row..=end_row {
            let mut row_values = Vec::with_capacity(column_count as usize);
            for col_idx in start_col..=end_col {
                let cell_value = if let Some(cell) = worksheet.cell((col_idx, row_idx)) {
                    CellValue {
                        value: cell.value().into_owned(),
                        formula: if cell.formula().is_empty() {
                            None
                        } else {
                            Some(cell.formula().to_string())
                        },
                    }
                } else {
                    CellValue {
                        value: String::new(),
                        formula: None,
                    }
                };
                row_values.push(cell_value);
            }
            values.push(row_values);
        }

        Ok(RangeData {
            start_row,
            end_row,
            start_col,
            end_col,
            values,
        })
    }

    pub fn update_cell(
        &mut self,
        worksheet_name: &str,
        row: u32,
        col: u32,
        value: &str,
    ) -> Result<()> {
        let worksheet = self
            .workbook
            .sheet_by_name_mut(worksheet_name)
            .context("Worksheet not found")?;

        worksheet.cell_mut((col, row)).set_value(value.to_string());
        Ok(())
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        umya_spreadsheet::writer::xlsx::write(&self.workbook, path)
            .context("Failed to save Excel file")?;
        Ok(())
    }

    pub fn find_in_worksheet(
        &self,
        worksheet: &Worksheet,
        search_text: &str,
        case_sensitive: bool,
    ) -> Result<Vec<(u32, u32)>> {
        // Returns a vector of (row, column) coordinates where matches are found
        let mut matches = Vec::new();
        let search_text = if !case_sensitive {
            search_text.to_lowercase()
        } else {
            search_text.to_string()
        };

        for row_num in 1..=worksheet.highest_row() {
            for col_num in 1..=worksheet.highest_column() {
                if let Some(cell) = worksheet.cell((col_num, row_num)) {
                    let cell_value = if !case_sensitive {
                        cell.value().to_lowercase()
                    } else {
                        cell.value().to_string()
                    };

                    if cell_value.contains(&search_text) {
                        let coord = cell.coordinate();
                        matches.push((coord.row_num(), coord.col_num()));
                    }
                }
            }
        }

        Ok(matches)
    }

    pub fn get_cell_value(&self, worksheet: &Worksheet, row: u32, col: u32) -> Result<CellValue> {
        let cell = worksheet.cell((col, row)).context("Cell not found")?;

        Ok(CellValue {
            value: cell.value().into_owned(),
            formula: if cell.formula().is_empty() {
                None
            } else {
                Some(cell.formula().to_string())
            },
        })
    }
}

fn parse_range(range: &str) -> Result<(u32, u32, u32, u32)> {
    // Handle ranges like "A1:B10" and return (start_row, start_col, end_row, end_col)
    let parts: Vec<&str> = range.split(':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid range format. Expected format: 'A1:B10'");
    }

    let start = parse_cell_reference(parts[0])?;
    let end = parse_cell_reference(parts[1])?;

    // parse_cell_reference returns (row, col), so start.0 is row, start.1 is col
    Ok((start.0, start.1, end.0, end.1))
}

fn validate_range_bounds(
    start_row: u32,
    start_col: u32,
    end_row: u32,
    end_col: u32,
) -> Result<(u32, u32)> {
    anyhow::ensure!(
        (1..=MAX_EXCEL_ROWS).contains(&start_row) && (1..=MAX_EXCEL_ROWS).contains(&end_row),
        "Row must be between 1 and {MAX_EXCEL_ROWS}"
    );
    anyhow::ensure!(
        (1..=MAX_EXCEL_COLUMNS).contains(&start_col) && (1..=MAX_EXCEL_COLUMNS).contains(&end_col),
        "Column must be between 1 and {MAX_EXCEL_COLUMNS}"
    );
    anyhow::ensure!(
        start_row <= end_row && start_col <= end_col,
        "Range start must not follow range end"
    );

    let row_count = end_row
        .checked_sub(start_row)
        .and_then(|span| span.checked_add(1))
        .context("Row span overflow")?;
    let column_count = end_col
        .checked_sub(start_col)
        .and_then(|span| span.checked_add(1))
        .context("Column span overflow")?;
    let cell_count = u64::from(row_count)
        .checked_mul(u64::from(column_count))
        .context("Range area overflow")?;
    anyhow::ensure!(
        cell_count <= MAX_RANGE_CELLS,
        "Range contains {cell_count} cells; maximum is {MAX_RANGE_CELLS}"
    );

    Ok((row_count, column_count))
}

fn parse_cell_reference(reference: &str) -> Result<(u32, u32)> {
    // Parse Excel cell reference (e.g., "A1") and return (row, column) to match umya_spreadsheet's expectation
    let mut col_str = String::new();
    let mut row_str = String::new();
    let mut parsing_row = false;

    for c in reference.chars() {
        if c.is_alphabetic() {
            if parsing_row {
                anyhow::bail!("Invalid cell reference format");
            }
            col_str.push(c.to_ascii_uppercase());
        } else if c.is_numeric() {
            parsing_row = true;
            row_str.push(c);
        } else {
            anyhow::bail!("Invalid character in cell reference");
        }
    }

    let col = column_letter_to_number(&col_str)?;
    let row = row_str.parse::<u32>().context("Invalid row number")?;

    Ok((row, col))
}

fn column_letter_to_number(column: &str) -> Result<u32> {
    let mut result = 0u32;
    for c in column.chars() {
        if !c.is_ascii_alphabetic() {
            anyhow::bail!("Invalid column letter");
        }
        let digit = c.to_ascii_uppercase() as u32 - 'A' as u32 + 1;
        result = result
            .checked_mul(26)
            .and_then(|value| value.checked_add(digit))
            .context("Column number out of range")?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn get_test_file() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("computercontroller")
            .join("tests")
            .join("data")
            .join("FinancialSample.xlsx")
    }

    #[test]
    fn test_open_xlsx() -> Result<()> {
        let xlsx = XlsxTool::new(get_test_file())?;
        let worksheets = xlsx.list_worksheets()?;
        assert!(!worksheets.is_empty());
        Ok(())
    }

    #[test]
    fn test_get_column_names() -> Result<()> {
        let xlsx = XlsxTool::new(get_test_file())?;
        let worksheet = xlsx.get_worksheet_by_index(0)?;
        let columns = xlsx.get_column_names(worksheet)?;
        assert!(!columns.is_empty());
        println!("Columns: {:?}", columns);
        Ok(())
    }

    #[test]
    fn test_get_range() -> Result<()> {
        let xlsx = XlsxTool::new(get_test_file())?;
        let worksheet = xlsx.get_worksheet_by_index(0)?;
        let range = xlsx.get_range(worksheet, "A1:C5")?;
        assert_eq!(range.values.len(), 5);
        println!("Range data: {:?}", range);
        Ok(())
    }

    #[test]
    fn test_range_budget_boundaries() -> Result<()> {
        assert_eq!(validate_range_bounds(1, 1, 100_000, 1)?, (100_000, 1));
        assert!(validate_range_bounds(1, 1, 100_001, 1).is_err());
        assert!(validate_range_bounds(1, 1, MAX_EXCEL_ROWS, MAX_EXCEL_COLUMNS).is_err());
        Ok(())
    }

    #[test]
    fn test_get_range_rejects_oversized_area_before_materialization() -> Result<()> {
        let xlsx = XlsxTool::new(get_test_file())?;
        let worksheet = xlsx.get_worksheet_by_index(0)?;
        assert!(xlsx.get_range(worksheet, "A1:A100001").is_err());
        Ok(())
    }

    #[test]
    fn test_range_rejects_invalid_excel_coordinates_and_ordering() {
        assert!(parse_range("A0:A1")
            .and_then(|range| validate_range_bounds(range.0, range.1, range.2, range.3))
            .is_err());
        assert!(parse_range("A1048577:A1048577")
            .and_then(|range| validate_range_bounds(range.0, range.1, range.2, range.3))
            .is_err());
        assert!(parse_range("XFE1:XFE1")
            .and_then(|range| validate_range_bounds(range.0, range.1, range.2, range.3))
            .is_err());
        assert!(parse_range("B2:A1")
            .and_then(|range| validate_range_bounds(range.0, range.1, range.2, range.3))
            .is_err());
        assert!(column_letter_to_number("ZZZZZZZZZZ").is_err());
    }

    #[test]
    fn test_find_in_worksheet() -> Result<()> {
        let xlsx = XlsxTool::new(get_test_file())?;
        let worksheet = xlsx.get_worksheet_by_index(0)?;
        let matches = xlsx.find_in_worksheet(worksheet, "Government", false)?;
        assert!(!matches.is_empty());
        println!("Found matches at: {:?}", matches);
        Ok(())
    }

    #[test]
    fn test_get_cell_value() -> Result<()> {
        let xlsx = XlsxTool::new(get_test_file())?;
        let worksheet = xlsx.get_worksheet_by_index(0)?;

        // Test header cell (known value from FinancialSample.xlsx)
        let header_cell = xlsx.get_cell_value(worksheet, 1, 1)?;
        assert_eq!(header_cell.value, "Segment");
        assert!(header_cell.formula.is_none());

        // Test data cell (known value from FinancialSample.xlsx)
        let data_cell = xlsx.get_cell_value(worksheet, 2, 2)?;
        assert_eq!(data_cell.value, "Canada");
        assert!(data_cell.formula.is_none());

        // Test B1 cell (known value from FinancialSample.xlsx)
        let b1_cell = xlsx.get_cell_value(worksheet, 1, 2)?;
        assert_eq!(b1_cell.value, "Country");
        assert!(b1_cell.formula.is_none());

        // Test A2 cell (known value from FinancialSample.xlsx)
        let a2_cell = xlsx.get_cell_value(worksheet, 2, 1)?;
        assert_eq!(a2_cell.value, "Government");
        assert!(a2_cell.formula.is_none());

        println!(
            "Header cell: {:#?}\nData cell: {:#?}",
            header_cell, data_cell
        );
        println!("B1: {:#?}\nA2: {:#?}", b1_cell, a2_cell);
        Ok(())
    }

    #[test]
    fn test_coordinate_mapping() -> Result<()> {
        let xlsx = XlsxTool::new(get_test_file())?;
        let worksheet = xlsx.get_worksheet_by_index(0)?;

        // Verify the coordinate system mapping
        // A1 should be row=1, col=1
        let a1 = xlsx.get_cell_value(worksheet, 1, 1)?;
        println!("A1 (1,1): {}", a1.value);
        assert_eq!(a1.value, "Segment");

        // A2 should be row=2, col=1
        let a2 = xlsx.get_cell_value(worksheet, 2, 1)?;
        println!("A2 (2,1): {}", a2.value);
        assert_eq!(a2.value, "Government");

        // B1 should be row=1, col=2
        let b1 = xlsx.get_cell_value(worksheet, 1, 2)?;
        println!("B1 (1,2): {}", b1.value);
        assert_eq!(b1.value, "Country");

        // B2 should be row=2, col=2
        let b2 = xlsx.get_cell_value(worksheet, 2, 2)?;
        println!("B2 (2,2): {}", b2.value);
        assert_eq!(b2.value, "Canada");

        // Verify that parse_cell_reference works correctly (row, col)
        assert_eq!(parse_cell_reference("A1")?, (1, 1));
        assert_eq!(parse_cell_reference("A2")?, (2, 1));
        assert_eq!(parse_cell_reference("B1")?, (1, 2));
        assert_eq!(parse_cell_reference("B2")?, (2, 2));
        assert_eq!(parse_cell_reference("Z1")?, (1, 26));
        assert_eq!(parse_cell_reference("AA1")?, (1, 27));

        Ok(())
    }

    #[test]
    fn test_issue_4550_row_column_transposition() -> Result<()> {
        // This test specifically addresses issue #4550 where A2 was returning B1's value
        let xlsx = XlsxTool::new(get_test_file())?;
        let worksheet = xlsx.get_worksheet_by_index(0)?;

        // Test that A2 (row 2, column 1) returns the correct value
        let a2_value = xlsx.get_cell_value(worksheet, 2, 1)?;
        assert_eq!(
            a2_value.value, "Government",
            "A2 should contain 'Government'"
        );

        // Test that B1 (row 1, column 2) returns its own value, not A2's
        let b1_value = xlsx.get_cell_value(worksheet, 1, 2)?;
        assert_eq!(b1_value.value, "Country", "B1 should contain 'Country'");

        // Additional verification with ranges
        let range = xlsx.get_range(worksheet, "A1:B2")?;
        assert_eq!(
            range.values[0][0].value, "Segment",
            "A1 should be 'Segment'"
        );
        assert_eq!(
            range.values[0][1].value, "Country",
            "B1 should be 'Country'"
        );
        assert_eq!(
            range.values[1][0].value, "Government",
            "A2 should be 'Government'"
        );
        assert_eq!(range.values[1][1].value, "Canada", "B2 should be 'Canada'");

        Ok(())
    }
}
