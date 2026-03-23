use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub struct InitArgs {
    /// Force re-download of models even if already cached
    #[arg(long)]
    pub update: bool,
}

pub async fn run(args: InitArgs) -> anyhow::Result<()> {
    use voidm_core::{embeddings, ner, nli, query_expansion, reranker, Config};

    println!("Initializing voidm models...\n");
    if args.update {
        println!("⚠️  Update mode: Will re-download all models\n");
    }

    let config = Config::load();
    let embedding_model = &config.embeddings.model;

    // Get default query expansion model from config
    let qe_config = &config.search.query_expansion;
    let qe_model = qe_config
        .as_ref()
        .map(|qe| qe.model.clone())
        .unwrap_or_else(|| "tinyllama".to_string());
    let qe_enabled = qe_config.as_ref().map(|qe| qe.enabled).unwrap_or(true);

    // Get reranker model from config
    let reranker_config = &config.search.reranker;
    let reranker_model = reranker_config
        .as_ref()
        .map(|r| r.model.clone())
        .unwrap_or_else(|| "ms-marco-TinyBERT".to_string());
    let reranker_enabled = reranker_config.as_ref().map(|r| r.enabled).unwrap_or(false);

    let total = 5; // 1 embedding + NER + NLI + query expansion + reranker
    let mut initialized = 0;

    // Initialize configured embedding model
    print!(
        "[1/5] Initializing embedding model: {} ... ",
        embedding_model
    );
    std::io::Write::flush(&mut std::io::stdout())?;
    match embeddings::get_embedder(embedding_model) {
        Ok(_) => {
            println!("✓");
            initialized += 1;
        }
        Err(e) => {
            eprintln!("✗ FAILED: {}", e);
            return Err(e);
        }
    }

    // Initialize NER model
    print!("[2/5] Initializing NER model ... ");
    std::io::Write::flush(&mut std::io::stdout())?;
    match ner::ensure_ner_model().await {
        Ok(_) => {
            println!("✓");
            initialized += 1;
        }
        Err(e) => {
            eprintln!("✗ FAILED: {}", e);
            return Err(e);
        }
    }

    // Initialize NLI model
    print!("[3/5] Initializing NLI model ... ");
    std::io::Write::flush(&mut std::io::stdout())?;
    match nli::ensure_nli_model().await {
        Ok(_) => {
            println!("✓");
            initialized += 1;
        }
        Err(e) => {
            eprintln!("✗ FAILED: {}", e);
            return Err(e);
        }
    }

    // Initialize default query expansion model (only if enabled)
    if qe_enabled {
        print!(
            "[4/5] Initializing query expansion model: {} ... ",
            qe_model
        );
        std::io::Write::flush(&mut std::io::stdout())?;
        match query_expansion::ensure_llm_model(&qe_model).await {
            Ok(_) => {
                println!("✓");
                initialized += 1;
            }
            Err(e) => {
                eprintln!("✗ FAILED: {}", e);
                return Err(e);
            }
        }
    } else {
        println!("[4/5] Skipping query expansion model (disabled in config)");
        initialized += 1;
    }

    // Initialize reranker model (only if enabled)
    if reranker_enabled {
        print!("[5/5] Initializing reranker model: {} ... ", reranker_model);
        std::io::Write::flush(&mut std::io::stdout())?;

        // Check if already cached (unless update flag is set)
        let is_cached = reranker::is_model_cached(&reranker_model);
        if is_cached && !args.update {
            println!("✓ (cached)");
            initialized += 1;
        } else {
            if args.update && is_cached {
                println!("(updating)");
            }
            match reranker::load_reranker_cached(&reranker_model, args.update).await {
                Ok(_) => {
                    println!("✓");
                    initialized += 1;
                }
                Err(e) => {
                    eprintln!("✗ FAILED: {}", e);
                    return Err(e);
                }
            }
        }
    } else {
        println!("[5/5] Skipping reranker model (disabled in config)");
        initialized += 1;
    }

    println!("\n✓ Initialization complete!");
    println!("  Initialized: {}/{} models", initialized, total);

    Ok(())
}
