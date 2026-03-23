#!/bin/bash

MODEL=${1:-gpt2}
PROMPT=${2:-"Expand query: python ->"}
MAX_TOKENS=${3:-20}

echo "=== ONNX Debug Runner ==="
echo "Model: $MODEL"
echo "Prompt: $PROMPT"
echo "Max tokens: $MAX_TOKENS"

cargo run --release --bin onnx_debug -- "$MODEL" "$PROMPT" "$MAX_TOKENS"
