use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use umya_spreadsheet::{Spreadsheet, Worksheet};

// Single-entry cache for the most recently parsed workbook. Sequential
// xlsx ops in one MCP turn (list → get_columns → get_range) all parse
// the same file; this skips the re-parse on cache hit. Mtime-keyed so
// any external write busts the cache. Cap of 1 entry keeps memory
// bounded with no eviction logic; concurrent access to different files
// just thrashes harmlessly.
struct CachedWorkbook {
    canonical: PathBuf,
    mtime: SystemTime,
    spreadsheet: Spreadsheet,
}

static WORKBOOK_CACHE: Lazy<Mutex<Option<CachedWorkbook>>> = Lazy::new(|| Mutex::new(None));

#[cfg(test)]
static CACHE_PARSE_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub fn _test_reset_cache() {
    if let Ok(mut g) = WORKBOOK_CACHE.lock() {
        *g = None;
    }
    CACHE_PARSE_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
pub fn _test_parse_count() -> usize {
    CACHE_PARSE_COUNT.load(std::sync::atomic::Ordering::SeqCst)
}

fn invalidate_cache(canonical: &Path) {
    if let Ok(mut g) = WORKBOOK_CACHE.lock() {
        if let Some(c) = g.as_ref() {
            if c.canonical == canonical {
                *g = None;
            }
        }
    }
}

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
    workbook: Spreadsheet,
}

impl XlsxTool {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_ref = path.as_ref();
        let canonical = path_ref
            .canonicalize()
            .unwrap_or_else(|_| path_ref.to_path_buf());
        let mtime = std::fs::metadata(&canonical)
            .and_then(|m| m.modified())
            .context("Failed to read xlsx mtime")?;

        if let Ok(guard) = WORKBOOK_CACHE.lock() {
            if let Some(cached) = guard.as_ref() {
                if cached.canonical == canonical && cached.mtime == mtime {
                    return Ok(Self {
                        workbook: cached.spreadsheet.clone(),
                    });
                }
            }
        }

        let workbook =
            umya_spreadsheet::reader::xlsx::read(path_ref).context("Failed to read Excel file")?;
        #[cfg(test)]
        CACHE_PARSE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        if let Ok(mut guard) = WORKBOOK_CACHE.lock() {
            *guard = Some(CachedWorkbook {
                canonical: canonical.clone(),
                mtime,
                spreadsheet: workbook.clone(),
            });
        }
        Ok(Self { workbook })
    }

    pub fn list_worksheets(&self) -> Result<Vec<WorksheetInfo>> {
        let mut worksheets = Vec::new();
        for (index, worksheet) in self.workbook.get_sheet_collection().iter().enumerate() {
            let (column_count, row_count) = self.get_worksheet_dimensions(worksheet)?;
            worksheets.push(WorksheetInfo {
                name: worksheet.get_name().to_string(),
                index,
                column_count,
                row_count,
            });
        }
        Ok(worksheets)
    }

    pub fn get_worksheet_by_name(&self, name: &str) -> Result<&Worksheet> {
        self.workbook
            .get_sheet_by_name(name)
            .context("Worksheet not found")
    }

    pub fn get_worksheet_by_index(&self, index: usize) -> Result<&Worksheet> {
        self.workbook
            .get_sheet_collection()
            .get(index)
            .context("Worksheet index out of bounds")
    }

    fn get_worksheet_dimensions(&self, worksheet: &Worksheet) -> Result<(usize, usize)> {
        // Returns (column_count, row_count) for the worksheet
        let mut max_col = 0;
        let mut max_row = 0;

        // Iterate through all rows
        for row_num in 1..=worksheet.get_highest_row() {
            for col_num in 1..=worksheet.get_highest_column() {
                if let Some(cell) = worksheet.get_cell((col_num, row_num)) {
                    let coord = cell.get_coordinate();
                    max_col = max_col.max(*coord.get_col_num() as usize);
                    max_row = max_row.max(*coord.get_row_num() as usize);
                }
            }
        }

        Ok((max_col, max_row))
    }

    pub fn get_column_names(&self, worksheet: &Worksheet) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for col_num in 1..=worksheet.get_highest_column() {
            if let Some(cell) = worksheet.get_cell((col_num, 1)) {
                names.push(cell.get_value().into_owned());
            } else {
                names.push(String::new());
            }
        }
        Ok(names)
    }

    pub fn get_range(&self, worksheet: &Worksheet, range: &str) -> Result<RangeData> {
        let (start_row, start_col, end_row, end_col) = parse_range(range)?;
        let mut values = Vec::new();

        // Iterate through rows first, then columns
        for row_idx in start_row..=end_row {
            let mut row_values = Vec::new();
            for col_idx in start_col..=end_col {
                let cell_value = if let Some(cell) = worksheet.get_cell((col_idx, row_idx)) {
                    CellValue {
                        value: cell.get_value().into_owned(),
                        formula: if cell.get_formula().is_empty() {
                            None
                        } else {
                            Some(cell.get_formula().to_string())
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
            .get_sheet_by_name_mut(worksheet_name)
            .context("Worksheet not found")?;

        worksheet
            .get_cell_mut((col, row))
            .set_value(value.to_string());
        Ok(())
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path_ref = path.as_ref();
        umya_spreadsheet::writer::xlsx::write(&self.workbook, path_ref)
            .context("Failed to save Excel file")?;
        // Bust the cached parse so the next read sees the new bytes/mtime.
        let canonical = path_ref
            .canonicalize()
            .unwrap_or_else(|_| path_ref.to_path_buf());
        invalidate_cache(&canonical);
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

        for row_num in 1..=worksheet.get_highest_row() {
            for col_num in 1..=worksheet.get_highest_column() {
                if let Some(cell) = worksheet.get_cell((col_num, row_num)) {
                    let cell_value = if !case_sensitive {
                        cell.get_value().to_lowercase()
                    } else {
                        cell.get_value().to_string()
                    };

                    if cell_value.contains(&search_text) {
                        let coord = cell.get_coordinate();
                        matches.push((*coord.get_row_num(), *coord.get_col_num()));
                    }
                }
            }
        }

        Ok(matches)
    }

    pub fn get_cell_value(&self, worksheet: &Worksheet, row: u32, col: u32) -> Result<CellValue> {
        let cell = worksheet.get_cell((col, row)).context("Cell not found")?;

        Ok(CellValue {
            value: cell.get_value().into_owned(),
            formula: if cell.get_formula().is_empty() {
                None
            } else {
                Some(cell.get_formula().to_string())
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
        result = result * 26 + (c.to_ascii_uppercase() as u32 - 'A' as u32 + 1);
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

    #[test]
    fn cache_skips_reparse_for_same_path_and_mtime() -> Result<()> {
        // Other tests in this module run concurrently and share the parse
        // counter, so use deltas instead of absolute values.
        _test_reset_cache();
        let baseline = _test_parse_count();

        let path = get_test_file();
        let _ = XlsxTool::new(&path)?;
        let _ = XlsxTool::new(&path)?;
        let _ = XlsxTool::new(&path)?;

        // Three opens, but only the first should have parsed. Concurrent
        // tests opening *other* files may bump the counter further; we
        // assert at least one parse occurred for our path.
        let delta = _test_parse_count().saturating_sub(baseline);
        assert!(delta >= 1, "first open should parse (delta {} >= 1)", delta);
        // The cache holds at most one entry, and concurrent tests opening
        // different paths will evict ours. So we can't reliably assert the
        // delta is exactly 1 across the whole module run. Instead, run two
        // back-to-back opens and ensure the second one is a cache hit by
        // counting parses inside a tight loop:
        _test_reset_cache();
        let before = _test_parse_count();
        let _ = XlsxTool::new(&path)?;
        let after_first = _test_parse_count();
        let _ = XlsxTool::new(&path)?;
        let after_second = _test_parse_count();
        assert_eq!(
            after_second - after_first,
            0,
            "second back-to-back open should hit the cache"
        );
        assert_eq!(
            after_first - before,
            1,
            "first open after reset should parse exactly once"
        );

        Ok(())
    }
}
