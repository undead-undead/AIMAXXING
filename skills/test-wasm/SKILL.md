---
name: wasm_test
description: A test skill running in WASM
runtime: wasm
script: test.wasm
parameters:
  type: object
  properties:
    text:
      type: string
---
Test instructions for WASM skill
