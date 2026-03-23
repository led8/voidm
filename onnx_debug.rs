use ort::session::Session;
use tokenizers::Tokenizer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    let model_name = std::env::args().nth(1).unwrap_or_else(|| "gpt2".to_string());
    let prompt = std::env::args().nth(2).unwrap_or_else(|| "Expand query: python ->".to_string());
    let max_tokens = std::env::args().nth(3).and_then(|s| s.parse::<usize>().ok()).unwrap_or(20);

    println!("=== ONNX Model Debug ===");
    println!("Model: {}", model_name);
    println!("Prompt: {}", prompt);
    println!("Max tokens: {}\n", max_tokens);

    // Download model
    let cache_dir = dirs::cache_dir().unwrap().join("onnx-debug").join(&model_name);
    std::fs::create_dir_all(&cache_dir)?;

    let api = hf_hub::api::tokio::ApiBuilder::new()
        .with_cache_dir(cache_dir.parent().unwrap())
        .build()?;
    
    let repo = api.model(model_name.clone());
    
    // Try multiple ONNX paths
    let onnx_paths = vec!["onnx/model.onnx", "onnx/decoder_model.onnx"];
    let onnx_file = {
        let mut found = None;
        for path in onnx_paths {
            match repo.get(path).await {
                Ok(f) => {
                    println!("Found ONNX at: {}", path);
                    found = Some(f);
                    break;
                }
                Err(e) => println!("Not at {}: {}", path, e),
            }
        }
        found.ok_or_else(|| anyhow::anyhow!("No ONNX found"))?
    };

    let tok_file = repo.get("tokenizer.json").await?;
    
    println!("Loading tokenizer from: {}", tok_file.display());
    let tokenizer = Tokenizer::from_file(&tok_file)?;
    
    println!("Loading ONNX from: {}", onnx_file.display());
    let session = Session::builder()?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
        .commit_from_file(&onnx_file)?;

    // Tokenize prompt
    println!("\n=== Tokenization ===");
    let encoding = tokenizer.encode(prompt, true)?;
    let token_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
    println!("Tokens: {:?}", &token_ids[..std::cmp::min(10, token_ids.len())]);
    println!("Token count: {}", token_ids.len());

    // Generate
    println!("\n=== Generation ===");
    let mut input_ids = token_ids.clone();
    let mut generated = Vec::new();
    
    for step in 0..max_tokens {
        let seq_len = input_ids.len();
        let attention_mask: Vec<i64> = vec![1i64; seq_len];
        
        let input_tensor = ort::Value::from_array_buffer(
            vec![1 as u64, seq_len as u64],
            input_ids.clone(),
        )?;
        
        let mask_tensor = ort::Value::from_array_buffer(
            vec![1 as u64, seq_len as u64],
            attention_mask,
        )?;
        
        let outputs = session.run(ort::inputs![
            "input_ids" => input_tensor,
            "attention_mask" => mask_tensor
        ])?;
        
        // Get logits
        let logits = outputs.get("logits")
            .or_else(|| outputs.get("last_hidden_state"))
            .ok_or_else(|| anyhow::anyhow!("No logits output"))?;
        
        let logits_array = logits.try_extract_tensor::<f32>()?;
        let logits_data = logits_array.view();
        
        let vocab_size = logits_data.len() / seq_len;
        println!("Step {}: vocab_size={}, logits_len={}", step, vocab_size, logits_data.len());
        
        // Get last token logits
        let last_logits = &logits_data[(seq_len - 1) * vocab_size..];
        
        // Find argmax
        let next_token = last_logits.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(idx, _)| idx as i64)
            .unwrap_or(0);
        
        println!("  Generated token: {} (logit: {:.4})", next_token, last_logits[next_token as usize]);
        
        if next_token == 50256 || next_token == 2 {  // EOS tokens
            println!("  [EOS]");
            break;
        }
        
        generated.push(next_token as u32);
        input_ids.push(next_token);
    }
    
    // Decode
    println!("\n=== Decoding ===");
    let decoded = tokenizer.decode(&generated, true)?;
    println!("Generated: {}", decoded);
    println!("Full output: {} {}", prompt, decoded);

    Ok(())
}
