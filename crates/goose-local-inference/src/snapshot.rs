use std::collections::{HashMap, HashSet};

fn safetensors_shard(filename: &str) -> Option<(&str, u32, u32)> {
    let stem = filename.strip_suffix(".safetensors")?;
    let (indexed_name, total) = stem.rsplit_once("-of-")?;
    let (family, index) = indexed_name.rsplit_once('-')?;
    let index = index.parse().ok()?;
    let total = total.parse().ok()?;
    (index > 0 && index <= total).then_some((family, index, total))
}

pub(crate) fn snapshot_files_are_complete(
    filenames: &HashSet<&str>,
    index: Option<&serde_json::Value>,
) -> bool {
    let safetensors: Vec<_> = filenames
        .iter()
        .copied()
        .filter(|filename| filename.ends_with(".safetensors"))
        .collect();
    if safetensors.is_empty() {
        return false;
    }

    if let Some(index) = index {
        let Some(weight_map) = index.get("weight_map").and_then(|value| value.as_object()) else {
            return false;
        };
        let Some(expected): Option<HashSet<_>> =
            weight_map.values().map(|value| value.as_str()).collect()
        else {
            return false;
        };
        return !expected.is_empty()
            && expected.iter().all(|filename| {
                filename.ends_with(".safetensors") && filenames.contains(filename)
            });
    }

    let mut shard_groups = HashMap::new();
    for filename in safetensors {
        if let Some((family, index, total)) = safetensors_shard(filename) {
            let (expected_total, indices) = shard_groups
                .entry(family)
                .or_insert_with(|| (total, HashSet::new()));
            if *expected_total != total {
                return false;
            }
            indices.insert(index);
        }
    }

    shard_groups.values().all(|(total, indices)| {
        indices.len() == *total as usize && (1..=*total).all(|index| indices.contains(&index))
    })
}
