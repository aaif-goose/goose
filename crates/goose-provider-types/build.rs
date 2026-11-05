#[cfg(feature = "std")]
use std::fs::File;
#[cfg(feature = "std")]
use std::io::{BufReader, BufWriter};
#[cfg(feature = "std")]
use std::path::PathBuf;

#[cfg(feature = "std")]
fn main() {
    const CATALOG: &str = "src/canonical/data/canonical_models.json";
    println!("cargo:rerun-if-changed={CATALOG}");

    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR must be set"))
        .join("canonical_models.json.zst");
    let input = BufReader::new(File::open(CATALOG).expect("canonical model catalog must open"));
    let output = BufWriter::new(File::create(output).expect("compressed catalog must be created"));
    zstd::stream::copy_encode(input, output, 19).expect("canonical model catalog must compress");
}

#[cfg(not(feature = "std"))]
fn main() {}
