As the code stands today, voidm is mostly local-model based. There is no built-in OpenAI/Anthropic-style remote model dependency in the core path. Models are downloaded on first use and cached locally. The only model that is effectively on by default in normal operation is the embedding model. See crates/voidm-core/src/config.rs:293 and crates/voidm-core/src/embeddings.rs:7.

    - Embeddings: default Xenova/all-MiniLM-L6-v2. This is the main model in day-to-day use. It is used when inserting memories to compute/store vectors, during semantic or hybrid search to embed the query, and in embedding-based helpers like semantic dedup. Supported alternatives
    include BGE, Nomic, MxBAI, and multilingual E5 variants. See crates/voidm-core/src/config.rs:293, crates/voidm-core/src/embeddings.rs:35, crates/voidm-core/src/crud.rs:100, crates/voidm-core/src/search.rs:143, and crates/voidm-core/src/semantic_dedup.rs:156.

    - Query expansion LLM: optional, not active in normal default search config. If you enable it, the default model name is tinyllama; the code also supports the GGUF model tobil/qmd-query-expansion-1.7B, and internal ONNX aliases phi-2 and gpt2-small are present. It is used before
    search to expand the user query with related terms; if it fails, the CLI falls back to the original query. See crates/voidm-core/src/config.rs:200, crates/voidm-core/src/query_expansion.rs:1, crates/voidm-core/src/query_expansion.rs:249, and crates/voidm-cli/src/commands/
    search.rs:160.

    - Search reranker: optional, also off by default in normal search config. If enabled, the default configured model is ms-marco-MiniLM-L-6-v2. Supported rerankers in code are ms-marco-TinyBERT-L-2, ms-marco-MiniLM-L-6-v2, mmarco-mMiniLMv2-L12-H384-v1, and qnli-distilroberta-base. It
    runs after the initial search pass and rescoring applies only to the top-k results. See crates/voidm-core/src/config.rs:165, crates/voidm-core/src/reranker.rs:1, crates/voidm-core/src/reranker.rs:360, and crates/voidm-core/src/search.rs:273.

    - NER model: Xenova/bert-base-NER. This is used for ontology/entity extraction work, and it also influences quality scoring if the model has already been loaded. It is not part of the plain “store and search memories” critical path. See crates/voidm-core/src/ner.rs:1, crates/voidm-
    cli/src/commands/ontology.rs:923, and crates/voidm-core/src/quality.rs:14.
  
    - NLI model: cross-encoder/nli-deberta-v3-small. This is used for ontology enrichment, relation suggestion, and contradiction detection between concepts. It is not used in the normal insert/search path. See crates/voidm-core/src/nli.rs:1 and crates/voidm-cli/src/commands/
    ontology.rs:770.

Two practical takeaways:

    - In normal use, the model you are most likely exercising is the embedding model.
    - Query expansion, reranking, NER, and NLI are feature-specific layers you opt into or trigger through specific commands.

One nuance: there is a small default mismatch between runtime config and voidm init. Runtime search defaults to no reranker and no query expansion, but voidm init will still prewarm tinyllama by default if query expansion is absent, and uses ms-marco-TinyBERT as its fallback reranker name when looking at init-time defaults. See crates/voidm-core/src/config.rs:302 and crates/voidm-cli/src/commands/init.rs:21.