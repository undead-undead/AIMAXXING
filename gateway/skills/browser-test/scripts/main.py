import os
import json
import subprocess

def run():
    chrome_path = os.environ.get("CHROME_PATH")
    puppeteer_path = os.environ.get("PUPPETEER_EXECUTABLE_PATH")
    
    exists = os.path.exists(chrome_path) if chrome_path else False
    
    # Try to get version if exists
    version = "N/A"
    if exists:
        try:
            version = subprocess.check_output([chrome_path, "--version"]).decode().strip()
        except Exception as e:
            version = f"Error: {e}"

    print(json.dumps({
        "chrome_path": chrome_path,
        "puppeteer_path": puppeteer_path,
        "exists": exists,
        "version": version,
        "env": {k: v for k, v in os.environ.items() if "PATH" in k or "CHROME" in k}
    }))

if __name__ == "__main__":
    run()
