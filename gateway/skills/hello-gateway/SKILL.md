---
name: hello-gateway
description: A test skill to verify Pixi auto-provisioning and sandbox isolation.
runtime: python3
script: main.py
dependencies:
  - requests
---

This skill imports the `requests` library and prints its version to prove the environment was correctly provisioned.
