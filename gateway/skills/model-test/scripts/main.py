#!/usr/bin/env python3
"""Test script to verify model provisioning pipeline."""

import os
import sys
import json

def main():
    models_path = os.environ.get("AIMAXXING_MODELS_PATH", "")
    
    result = {
        "status": "success",
        "models_path": models_path,
        "models_path_exists": os.path.isdir(models_path) if models_path else False,
        "model_files": [],
    }
    
    if models_path and os.path.isdir(models_path):
        files = os.listdir(models_path)
        result["model_files"] = files
        
        # Check for the test model
        test_model = os.path.join(models_path, "test-model.json")
        if os.path.exists(test_model):
            size = os.path.getsize(test_model)
            result["test_model_size_bytes"] = size
            result["test_model_exists"] = True
        else:
            result["test_model_exists"] = False
            result["status"] = "error"
            result["error"] = "test-model.json not found in AIMAXXING_MODELS_PATH"
    elif not models_path:
        result["status"] = "error"
        result["error"] = "AIMAXXING_MODELS_PATH environment variable not set"
    else:
        result["status"] = "error"
        result["error"] = f"AIMAXXING_MODELS_PATH directory does not exist: {models_path}"
    
    print(json.dumps(result, indent=2))

if __name__ == "__main__":
    main()
