use super::*;
use std::io::Write;
pub fn write_artifact(root: &std::path::Path) {
    let config = r#"{
          "model_type":"mistral","eos_token_id":1,"hidden_size":16,
          "num_hidden_layers":2,"intermediate_size":32,
          "num_attention_heads":4,"rms_norm_eps":0.00001,"vocab_size":64
        }"#;
    std::fs::write(root.join("config.json"), config).unwrap();
    let resolved = eredu_architectures::configuration::resolve_model_config(
        &serde_json::from_str(config).unwrap(),
    )
    .unwrap();
    let checkpoint = resolved.architecture.checkpoint();
    let mut offset = 0usize;
    let mut header = serde_json::Map::new();
    for tensor in checkpoint.common_tensors.iter().chain(
        checkpoint
            .layout_groups
            .iter()
            .filter_map(|group| group.variants.first())
            .flat_map(|variant| variant.tensors.iter()),
    ) {
        let bytes = tensor.shape.iter().product::<usize>() * std::mem::size_of::<f32>();
        header.insert(
            tensor.key.clone(),
            serde_json::json!({
                "dtype": "F32",
                "shape": tensor.shape,
                "data_offsets": [offset, offset + bytes]
            }),
        );
        offset += bytes;
    }
    let header = serde_json::to_vec(&header).unwrap();
    let mut weights = std::fs::File::create(root.join("model.safetensors")).unwrap();
    weights
        .write_all(&(header.len() as u64).to_le_bytes())
        .unwrap();
    weights.write_all(&header).unwrap();
    weights.write_all(&vec![0; offset]).unwrap();

    tokenizer()
        .save(root.join("tokenizer.json"), false)
        .unwrap();
    std::fs::write(root.join("chat_template.jinja"), TEMPLATE).unwrap();
}
