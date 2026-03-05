---
name: model-test
description: A test skill that demonstrates model hosting. Downloads a small test model and verifies access.
runtime: python3
script: main.py
dependencies:
  - python
models:
  - name: test-model.json
    source: "https://raw.githubusercontent.com/huggingface/transformers/main/examples/pytorch/text-classification/requirements.txt"
    format: custom
    size_mb: 1
---

This skill downloads a small test file as a "model" to verify the model provisioning pipeline.
The script checks that AIMAXXING_MODELS_PATH is set and the model file exists.
