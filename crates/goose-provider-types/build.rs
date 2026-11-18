use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

fn main() {
    const CATALOG: &str = "src/canonical/data/canonical_models.json";
    println!("cargo:rerun-if-changed={CATALOG}");

    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR must be set"))
        .join("canonical_models.json.zst");
    let input = BufReader::new(File::open(CATALOG).expect("canonical model catalog must open"));
    let output = BufWriter::new(File::create(output).expect("compressed catalog must be created"));
    zstd::stream::copy_encode(input, output, 19).expect("canonical model catalog must compress");
}
